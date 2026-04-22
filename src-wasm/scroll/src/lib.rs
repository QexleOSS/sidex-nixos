use wasm_bindgen::prelude::*;

static mut OUT_BUF: [f64; 8] = [0.0; 8];

#[wasm_bindgen]
pub fn get_output_ptr() -> *const f64 {
    unsafe { OUT_BUF.as_ptr() }
}

// --- Easing Functions ---

#[wasm_bindgen]
pub fn ease_out_cubic(t: f64) -> f64 {
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}

#[wasm_bindgen]
pub fn ease_in_cubic(t: f64) -> f64 {
    t * t * t
}

// --- Scroll State Validation ---

#[wasm_bindgen]
pub struct ScrollStateResult {
    pub width: f64,
    pub scroll_width: f64,
    pub scroll_left: f64,
    pub height: f64,
    pub scroll_height: f64,
    pub scroll_top: f64,
}

#[wasm_bindgen]
pub fn validate_scroll_state(
    mut width: f64,
    scroll_width: f64,
    mut scroll_left: f64,
    mut height: f64,
    scroll_height: f64,
    mut scroll_top: f64,
    force_int: bool,
) -> ScrollStateResult {
    if force_int {
        width = width.floor();
        scroll_left = scroll_left.floor();
        height = height.floor();
        scroll_top = scroll_top.floor();
    }

    if width < 0.0 {
        width = 0.0;
    }
    if scroll_left + width > scroll_width {
        scroll_left = scroll_width - width;
    }
    if scroll_left < 0.0 {
        scroll_left = 0.0;
    }
    if height < 0.0 {
        height = 0.0;
    }
    if scroll_top + height > scroll_height {
        scroll_top = scroll_height - height;
    }
    if scroll_top < 0.0 {
        scroll_top = 0.0;
    }

    ScrollStateResult {
        width,
        scroll_width,
        scroll_left,
        height,
        scroll_height,
        scroll_top,
    }
}

// --- Smooth Scroll Animation Tick ---

#[wasm_bindgen]
pub struct SmoothScrollResult {
    pub scroll_left: f64,
    pub scroll_top: f64,
    pub is_done: bool,
}

#[wasm_bindgen]
pub fn smooth_scroll_tick(
    now: f64,
    start_time: f64,
    duration: f64,
    from_left: f64,
    to_left: f64,
    from_top: f64,
    to_top: f64,
    viewport_width: f64,
    viewport_height: f64,
) -> SmoothScrollResult {
    let completion = (now - start_time) / duration;

    if completion >= 1.0 {
        return SmoothScrollResult {
            scroll_left: to_left,
            scroll_top: to_top,
            is_done: true,
        };
    }

    let new_left = animate_axis(from_left, to_left, viewport_width, completion);
    let new_top = animate_axis(from_top, to_top, viewport_height, completion);

    SmoothScrollResult {
        scroll_left: new_left,
        scroll_top: new_top,
        is_done: false,
    }
}

fn animate_axis(from: f64, to: f64, viewport_size: f64, completion: f64) -> f64 {
    let delta = (from - to).abs();
    if delta > 2.5 * viewport_size {
        let (stop1, stop2) = if from < to {
            (from + 0.75 * viewport_size, to - 0.75 * viewport_size)
        } else {
            (from - 0.75 * viewport_size, to + 0.75 * viewport_size)
        };
        composed_ease(from, stop1, stop2, to, 0.33, completion)
    } else {
        ease_between(from, to, completion)
    }
}

fn ease_between(from: f64, to: f64, completion: f64) -> f64 {
    from + (to - from) * ease_out_cubic(completion)
}

fn composed_ease(from: f64, stop1: f64, stop2: f64, to: f64, cut: f64, completion: f64) -> f64 {
    if completion < cut {
        ease_between(from, stop1, completion / cut)
    } else {
        ease_between(stop2, to, (completion - cut) / (1.0 - cut))
    }
}

// --- Inertial Scroll Physics ---

#[wasm_bindgen]
pub struct InertialState {
    pub speed_x: f64,
    pub speed_y: f64,
    pub active: bool,
}

#[wasm_bindgen]
pub fn inertial_tick(
    speed_x: f64,
    speed_y: f64,
    decay: f64,
    threshold: f64,
) -> InertialState {
    let mut sx = speed_x * decay;
    let mut sy = speed_y * decay;

    if sx.abs() < threshold {
        sx = 0.0;
    }
    if sy.abs() < threshold {
        sy = 0.0;
    }

    InertialState {
        speed_x: sx,
        speed_y: sy,
        active: sx != 0.0 || sy != 0.0,
    }
}

// --- Mouse Wheel Classifier (port of JS version) ---

const CLASSIFIER_CAPACITY: usize = 5;

#[wasm_bindgen]
pub struct WheelClassifier {
    timestamps: Vec<f64>,
    delta_xs: Vec<f64>,
    delta_ys: Vec<f64>,
    scores: Vec<f64>,
    front: i32,
    rear: i32,
}

#[wasm_bindgen]
impl WheelClassifier {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        WheelClassifier {
            timestamps: vec![0.0; CLASSIFIER_CAPACITY],
            delta_xs: vec![0.0; CLASSIFIER_CAPACITY],
            delta_ys: vec![0.0; CLASSIFIER_CAPACITY],
            scores: vec![0.0; CLASSIFIER_CAPACITY],
            front: -1,
            rear: -1,
        }
    }

    pub fn accept(&mut self, timestamp: f64, delta_x: f64, delta_y: f64) {
        let cap = CLASSIFIER_CAPACITY as i32;

        let prev_idx = if self.rear >= 0 { Some(self.rear as usize) } else { None };

        if self.front == -1 && self.rear == -1 {
            self.front = 0;
            self.rear = 0;
        } else {
            self.rear = (self.rear + 1) % cap;
            if self.rear == self.front {
                self.front = (self.front + 1) % cap;
            }
        }

        let idx = self.rear as usize;
        self.timestamps[idx] = timestamp;
        self.delta_xs[idx] = delta_x;
        self.delta_ys[idx] = delta_y;

        let score = self.compute_score(idx, prev_idx);
        self.scores[idx] = score;
    }

    pub fn is_physical_mouse_wheel(&self) -> bool {
        if self.front == -1 && self.rear == -1 {
            return false;
        }

        let cap = CLASSIFIER_CAPACITY as i32;
        let mut remaining = 1.0_f64;
        let mut score = 0.0_f64;
        let mut iteration = 1_u32;
        let mut index = self.rear;

        loop {
            let influence = if index == self.front {
                remaining
            } else {
                2.0_f64.powi(-(iteration as i32))
            };
            remaining -= influence;
            score += self.scores[index as usize] * influence;

            if index == self.front {
                break;
            }
            index = (cap + index - 1) % cap;
            iteration += 1;
        }

        score <= 0.5
    }

    fn compute_score(&self, idx: usize, prev_idx: Option<usize>) -> f64 {
        let dx = self.delta_xs[idx];
        let dy = self.delta_ys[idx];

        if dx.abs() > 0.0 && dy.abs() > 0.0 {
            return 1.0;
        }

        let mut score: f64 = 0.5;

        if !is_almost_int(dx) || !is_almost_int(dy) {
            score += 0.25;
        }

        if let Some(pi) = prev_idx {
            let abs_dx = dx.abs();
            let abs_dy = dy.abs();
            let abs_pdx = self.delta_xs[pi].abs();
            let abs_pdy = self.delta_ys[pi].abs();

            let min_dx = abs_dx.min(abs_pdx).max(1.0);
            let min_dy = abs_dy.min(abs_pdy).max(1.0);
            let max_dx = abs_dx.max(abs_pdx);
            let max_dy = abs_dy.max(abs_pdy);

            if max_dx % min_dx == 0.0 && max_dy % min_dy == 0.0 {
                score -= 0.5;
            }
        }

        score.clamp(0.0, 1.0)
    }
}

fn is_almost_int(value: f64) -> bool {
    let delta = (value.round() - value).abs();
    delta < 0.01 + f64::EPSILON * 100.0
}

// --- Zero-alloc flat output variants for per-frame hot paths ---

#[wasm_bindgen]
pub fn smooth_scroll_tick_flat(
    now: f64, start_time: f64, duration: f64,
    from_left: f64, to_left: f64,
    from_top: f64, to_top: f64,
    viewport_width: f64, viewport_height: f64,
) {
    let r = smooth_scroll_tick(
        now, start_time, duration,
        from_left, to_left, from_top, to_top,
        viewport_width, viewport_height,
    );
    unsafe {
        OUT_BUF[0] = r.scroll_left;
        OUT_BUF[1] = r.scroll_top;
        OUT_BUF[2] = if r.is_done { 1.0 } else { 0.0 };
    }
}

#[wasm_bindgen]
pub fn inertial_tick_flat(speed_x: f64, speed_y: f64, decay: f64, threshold: f64) {
    let r = inertial_tick(speed_x, speed_y, decay, threshold);
    unsafe {
        OUT_BUF[0] = r.speed_x;
        OUT_BUF[1] = r.speed_y;
        OUT_BUF[2] = if r.active { 1.0 } else { 0.0 };
    }
}

#[wasm_bindgen]
pub fn compute_scrollbar_state_flat(
    arrow_size: f64, scrollbar_size: f64, opposite_scrollbar_size: f64,
    visible_size: f64, scroll_size: f64, scroll_position: f64,
    min_slider_size: f64,
) {
    let r = compute_scrollbar_state(
        arrow_size, scrollbar_size, opposite_scrollbar_size,
        visible_size, scroll_size, scroll_position, min_slider_size,
    );
    unsafe {
        OUT_BUF[0] = r.slider_size;
        OUT_BUF[1] = r.slider_position;
        OUT_BUF[2] = r.slider_ratio;
    }
}

#[wasm_bindgen]
pub fn process_wheel_delta_flat(
    raw_delta_x: f64, raw_delta_y: f64,
    sensitivity: f64, scroll_predominant_axis: bool,
    flip_axes: bool, scroll_y_to_x: bool,
    is_shift: bool, is_alt: bool,
    fast_sensitivity: f64, is_mac: bool,
) {
    let r = process_wheel_delta(
        raw_delta_x, raw_delta_y, sensitivity,
        scroll_predominant_axis, flip_axes, scroll_y_to_x,
        is_shift, is_alt, fast_sensitivity, is_mac,
    );
    unsafe {
        OUT_BUF[0] = r.delta_x;
        OUT_BUF[1] = r.delta_y;
    }
}

#[wasm_bindgen]
pub fn validate_scroll_state_flat(
    width: f64, scroll_width: f64, scroll_left: f64,
    height: f64, scroll_height: f64, scroll_top: f64,
    force_int: bool,
) {
    let r = validate_scroll_state(width, scroll_width, scroll_left, height, scroll_height, scroll_top, force_int);
    unsafe {
        OUT_BUF[0] = r.width;
        OUT_BUF[1] = r.scroll_width;
        OUT_BUF[2] = r.scroll_left;
        OUT_BUF[3] = r.height;
        OUT_BUF[4] = r.scroll_height;
        OUT_BUF[5] = r.scroll_top;
    }
}

// --- Batch Scrollbar State Computation ---

#[wasm_bindgen]
pub struct ScrollbarValues {
    pub slider_size: f64,
    pub slider_position: f64,
    pub slider_ratio: f64,
}

#[wasm_bindgen]
pub fn compute_scrollbar_state(
    arrow_size: f64,
    scrollbar_size: f64,
    opposite_scrollbar_size: f64,
    visible_size: f64,
    scroll_size: f64,
    scroll_position: f64,
    min_slider_size: f64,
) -> ScrollbarValues {
    let visible = visible_size - opposite_scrollbar_size;
    let available = scrollbar_size - 2.0 * arrow_size;

    if scroll_size <= 0.0 || visible <= 0.0 || available <= 0.0 {
        return ScrollbarValues {
            slider_size: min_slider_size,
            slider_position: 0.0,
            slider_ratio: 1.0,
        };
    }

    let ratio = visible / scroll_size;
    let mut slider_size = (available * ratio).round().max(min_slider_size);
    if slider_size > available {
        slider_size = available;
    }

    let remaining = available - slider_size;
    let max_scroll = scroll_size - visible;
    let slider_position = if max_scroll > 0.0 {
        arrow_size + (remaining * scroll_position / max_scroll).round()
    } else {
        arrow_size
    };

    ScrollbarValues {
        slider_size,
        slider_position,
        slider_ratio: ratio,
    }
}

// --- Delta Processing ---

#[wasm_bindgen]
pub struct WheelDelta {
    pub delta_x: f64,
    pub delta_y: f64,
}

#[wasm_bindgen]
pub fn process_wheel_delta(
    raw_delta_x: f64,
    raw_delta_y: f64,
    sensitivity: f64,
    scroll_predominant_axis: bool,
    flip_axes: bool,
    scroll_y_to_x: bool,
    is_shift: bool,
    is_alt: bool,
    fast_sensitivity: f64,
    is_mac: bool,
) -> WheelDelta {
    let mut dx = raw_delta_x * sensitivity;
    let mut dy = raw_delta_y * sensitivity;

    if scroll_predominant_axis {
        if scroll_y_to_x && dx + dy == 0.0 {
            dx = 0.0;
            dy = 0.0;
        } else if dy.abs() >= dx.abs() {
            dx = 0.0;
        } else {
            dy = 0.0;
        }
    }

    if flip_axes {
        std::mem::swap(&mut dx, &mut dy);
    }

    let shift_convert = !is_mac && is_shift;
    if (scroll_y_to_x || shift_convert) && dx == 0.0 {
        dx = dy;
        dy = 0.0;
    }

    if is_alt {
        dx *= fast_sensitivity;
        dy *= fast_sensitivity;
    }

    WheelDelta {
        delta_x: dx,
        delta_y: dy,
    }
}
