//! Line tokenization — the hot path.
//!
//! Port of `tokenizeString.ts` from upstream. The algorithm:
//!
//! 1. If the rule stack has any `begin/while` frames, verify each
//!    while-condition before anything else. Frames whose condition
//!    fails are popped, truncating the stack above them.
//! 2. Loop scanning for the next match from the current line position:
//!    consult the active rule's scanner and all injection scanners,
//!    pick the left-most / highest-priority hit.
//! 3. Dispatch on the match:
//!    * `END_RULE_ID` — pop the begin/end frame; run end-captures.
//!    * `MatchRule` — emit the captured span, pop the ephemeral
//!      match frame.
//!    * `BeginEndRule` — push a new frame; run begin-captures; if
//!      end has back-refs, resolve them against the begin captures
//!      and store on the frame.
//!    * `BeginWhileRule` — same as begin/end but stores a while regex.
//! 4. Stop when no rule matches, when the scanner fails to advance
//!    two iterations in a row (endless-loop guard), or when the
//!    caller's time budget elapses.
//!
//! The tokenizer is generic over the grammar and stack types via
//! the traits in [`super::contracts`]; see that module for the exact
//! contract each trait exposes.

use std::time::Instant;

use super::contracts::{
    AttributedScopeStack, GrammarRuntime, Injection, MatchResult, ScanContext, StateStack,
    TokenSink, TokenizeStringResult,
};
use crate::rule::{Rule, RuleId, END_RULE_ID};
use crate::utils::CaptureIndex;

/// Inputs for [`tokenize_string`]. Grouped into a struct so the
/// signature stays readable and new options (e.g. `balanceBrackets`)
/// land without churning call-sites.
pub struct TokenizeInput<'a, G, S, T>
where
    G: GrammarRuntime,
    S: StateStack,
    T: TokenSink<S::Attr>,
{
    pub grammar: &'a G,
    pub line_text: &'a str,
    pub is_first_line: bool,
    pub line_pos: usize,
    pub stack: S,
    pub sink: &'a mut T,
    pub check_while_conditions: bool,
    /// `Some(ms)` → return [`TokenizeStringResult::stopped_early`] when
    /// the budget expires; `None` → no limit.
    pub time_limit_ms: Option<u64>,
}

/// Entrypoint matching upstream's `_tokenizeString`. Returns the
/// post-tokenization stack plus a flag set when the time limit was hit.
#[allow(clippy::too_many_lines)]
pub fn tokenize_string<G, S, T>(input: TokenizeInput<'_, G, S, T>) -> TokenizeStringResult<S>
where
    G: GrammarRuntime,
    S: StateStack,
    T: TokenSink<S::Attr>,
{
    let line_length = input.line_text.len();
    let mut line_pos = input.line_pos;
    let mut is_first_line = input.is_first_line;
    let mut stack = input.stack;
    let mut anchor_pos: i64 = -1;

    if input.check_while_conditions {
        let result = check_while_conditions(
            input.grammar,
            input.line_text,
            is_first_line,
            line_pos,
            stack,
            input.sink,
        );
        stack = result.stack;
        line_pos = result.line_pos;
        is_first_line = result.is_first_line;
        anchor_pos = result.anchor_pos;
    }

    let started_at = Instant::now();
    loop {
        if let Some(limit) = input.time_limit_ms {
            if u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX) > limit {
                return TokenizeStringResult {
                    stack,
                    stopped_early: true,
                };
            }
        }

        match scan_next(
            input.grammar,
            input.line_text,
            is_first_line,
            line_pos,
            &stack,
            anchor_pos,
        ) {
            ScanOutcome::NoMatch => {
                produce(input.sink, stack.content_name_scopes_list(), line_length);
                return TokenizeStringResult {
                    stack,
                    stopped_early: false,
                };
            }
            ScanOutcome::Matched(m) => {
                let caps = &m.captures;
                let capture_end = caps
                    .first()
                    .and_then(|c| c.map(|c| c.end))
                    .unwrap_or(line_pos);
                let capture_start = caps
                    .first()
                    .and_then(|c| c.map(|c| c.start))
                    .unwrap_or(line_pos);
                let has_advanced = capture_end > line_pos;

                if m.matched_rule_id == END_RULE_ID {
                    // Pop begin/end frame.
                    let popped_rule_id = stack.rule_id();
                    let popped_rule = input.grammar.rule(popped_rule_id);

                    produce(input.sink, stack.content_name_scopes_list(), capture_start);
                    if let Some(name_scopes) = stack.name_scopes_list().cloned() {
                        stack = stack.with_content_name_scopes(name_scopes);
                    }
                    if let Some(Rule::BeginEnd(rule)) = popped_rule.as_deref() {
                        handle_captures(
                            input.grammar,
                            input.line_text,
                            is_first_line,
                            &stack,
                            input.sink,
                            &rule.end_captures,
                            caps,
                        );
                    }
                    produce(input.sink, stack.content_name_scopes_list(), capture_end);

                    let popped_node = stack.clone();
                    stack = match popped_node.parent() {
                        Some(parent) => parent,
                        None => popped_node.clone(),
                    };
                    anchor_pos = popped_node.anchor_pos();

                    if !has_advanced
                        && i64::try_from(line_pos).unwrap_or(i64::MAX) == popped_node.enter_pos()
                    {
                        // Grammar pushed and popped without advancing —
                        // treat as endless loop and bail out.
                        stack = popped_node;
                        produce(input.sink, stack.content_name_scopes_list(), line_length);
                        return TokenizeStringResult {
                            stack,
                            stopped_early: false,
                        };
                    }
                } else {
                    let rule_id = RuleId(u32::try_from(m.matched_rule_id).unwrap_or(0));
                    let Some(rule) = input.grammar.rule(rule_id) else {
                        // Unknown rule id — treat as "no match" and exit.
                        produce(input.sink, stack.content_name_scopes_list(), line_length);
                        return TokenizeStringResult {
                            stack,
                            stopped_early: false,
                        };
                    };

                    produce(input.sink, stack.content_name_scopes_list(), capture_start);

                    let before_push = stack.clone();
                    let scope_name = rule
                        .as_ref()
                        .header()
                        .name_with_captures(Some(input.line_text), Some(caps));
                    let name_scopes_list = stack
                        .content_name_scopes_list()
                        .map(|s| s.push_attributed(scope_name.as_deref(), input.grammar));

                    stack = stack.push(
                        rule_id,
                        i64::try_from(line_pos).unwrap_or(i64::MAX),
                        anchor_pos,
                        capture_end == line_length,
                        None,
                        name_scopes_list.clone(),
                        name_scopes_list.clone(),
                    );

                    match rule.as_ref() {
                        Rule::BeginEnd(pushed) => {
                            handle_captures(
                                input.grammar,
                                input.line_text,
                                is_first_line,
                                &stack,
                                input.sink,
                                &pushed.begin_captures,
                                caps,
                            );
                            produce(input.sink, stack.content_name_scopes_list(), capture_end);
                            anchor_pos = i64::try_from(capture_end).unwrap_or(i64::MAX);

                            let content_name = pushed
                                .header
                                .content_name_with_captures(input.line_text, caps);
                            if let (Some(base), Some(content)) =
                                (name_scopes_list.as_ref(), content_name.as_deref())
                            {
                                let content_scopes =
                                    base.push_attributed(Some(content), input.grammar);
                                stack = stack.with_content_name_scopes(content_scopes);
                            }

                            if pushed.end_has_back_references {
                                let resolved =
                                    pushed.end.resolve_back_references(input.line_text, caps);
                                stack = stack.with_end_rule(resolved);
                            }

                            if !has_advanced && before_push.has_same_rule_as(&stack) {
                                // Endless loop — pop and stop.
                                stack = match stack.pop() {
                                    Some(p) => p,
                                    None => stack,
                                };
                                produce(input.sink, stack.content_name_scopes_list(), line_length);
                                return TokenizeStringResult {
                                    stack,
                                    stopped_early: false,
                                };
                            }
                        }
                        Rule::BeginWhile(pushed) => {
                            handle_captures(
                                input.grammar,
                                input.line_text,
                                is_first_line,
                                &stack,
                                input.sink,
                                &pushed.begin_captures,
                                caps,
                            );
                            produce(input.sink, stack.content_name_scopes_list(), capture_end);
                            anchor_pos = i64::try_from(capture_end).unwrap_or(i64::MAX);

                            let content_name = pushed
                                .header
                                .content_name_with_captures(input.line_text, caps);
                            if let (Some(base), Some(content)) =
                                (name_scopes_list.as_ref(), content_name.as_deref())
                            {
                                let content_scopes =
                                    base.push_attributed(Some(content), input.grammar);
                                stack = stack.with_content_name_scopes(content_scopes);
                            }

                            if pushed.while_has_back_references {
                                let resolved = pushed
                                    .while_regex
                                    .resolve_back_references(input.line_text, caps);
                                stack = stack.with_end_rule(resolved);
                            }

                            if !has_advanced && before_push.has_same_rule_as(&stack) {
                                stack = match stack.pop() {
                                    Some(p) => p,
                                    None => stack,
                                };
                                produce(input.sink, stack.content_name_scopes_list(), line_length);
                                return TokenizeStringResult {
                                    stack,
                                    stopped_early: false,
                                };
                            }
                        }
                        Rule::Match(matching) => {
                            handle_captures(
                                input.grammar,
                                input.line_text,
                                is_first_line,
                                &stack,
                                input.sink,
                                &matching.captures,
                                caps,
                            );
                            produce(input.sink, stack.content_name_scopes_list(), capture_end);

                            // Pop the ephemeral match frame immediately.
                            stack = stack.pop().unwrap_or_else(|| stack.safe_pop());

                            if !has_advanced {
                                // Pathological: a MatchRule that neither
                                // advanced nor popped. Recover & stop.
                                stack = stack.safe_pop();
                                produce(input.sink, stack.content_name_scopes_list(), line_length);
                                return TokenizeStringResult {
                                    stack,
                                    stopped_early: false,
                                };
                            }
                        }
                        Rule::IncludeOnly(_) | Rule::Capture(_) => {
                            // Include-only and Capture rules never appear as
                            // a scanner match directly; defensively stop.
                            produce(input.sink, stack.content_name_scopes_list(), line_length);
                            return TokenizeStringResult {
                                stack,
                                stopped_early: false,
                            };
                        }
                    }
                }

                if capture_end > line_pos {
                    line_pos = capture_end;
                    is_first_line = false;
                }
            }
        }
    }
}

enum ScanOutcome {
    NoMatch,
    Matched(MatchResult),
}

fn scan_next<G, S>(
    grammar: &G,
    line_text: &str,
    is_first_line: bool,
    line_pos: usize,
    stack: &S,
    anchor_pos: i64,
) -> ScanOutcome
where
    G: GrammarRuntime,
    S: StateStack,
{
    let grammar_hit = match_rule(
        grammar,
        line_text,
        is_first_line,
        line_pos,
        stack,
        anchor_pos,
    );
    let injections = grammar.injections();

    if injections.is_empty() {
        return match grammar_hit {
            Some(m) => ScanOutcome::Matched(m),
            None => ScanOutcome::NoMatch,
        };
    }

    let injection_hit = match_injections(
        &injections,
        grammar,
        line_text,
        is_first_line,
        line_pos,
        stack,
        anchor_pos,
    );

    match (grammar_hit, injection_hit) {
        (None, None) => ScanOutcome::NoMatch,
        (Some(m), None) => ScanOutcome::Matched(m),
        (None, Some((m, _priority))) => ScanOutcome::Matched(m),
        (Some(grammar), Some((inject, priority))) => {
            let grammar_score = first_capture_start(&grammar.captures);
            let inject_score = first_capture_start(&inject.captures);
            if inject_score < grammar_score
                || (priority == PRIORITY_MATCH && inject_score == grammar_score)
            {
                ScanOutcome::Matched(inject)
            } else {
                ScanOutcome::Matched(grammar)
            }
        }
    }
}

fn match_rule<G, S>(
    grammar: &G,
    line_text: &str,
    is_first_line: bool,
    line_pos: usize,
    stack: &S,
    anchor_pos: i64,
) -> Option<MatchResult>
where
    G: GrammarRuntime,
    S: StateStack,
{
    use crate::tokenizer::contracts::ScanContext;
    let ctx = ScanContext {
        line_text,
        line_pos,
        is_first_line,
        anchor_pos,
        end_rule: stack.end_rule(),
        rule_id: stack.rule_id(),
    };
    grammar.scan_active_rule(ctx)
}

fn match_injections<G, S>(
    injections: &[Injection],
    grammar: &G,
    line_text: &str,
    is_first_line: bool,
    line_pos: usize,
    stack: &S,
    anchor_pos: i64,
) -> Option<(MatchResult, i8)>
where
    G: GrammarRuntime,
    S: StateStack,
{
    use crate::tokenizer::contracts::ScanContext;
    let mut best: Option<(MatchResult, i8)> = None;
    let mut best_score = usize::MAX;

    for injection in injections {
        let ctx = ScanContext {
            line_text,
            line_pos,
            is_first_line,
            anchor_pos,
            end_rule: stack.end_rule(),
            rule_id: stack.rule_id(),
        };
        let Some(hit) = grammar.scan_injection(injection, ctx) else {
            continue;
        };
        let score = hit
            .captures
            .first()
            .and_then(|c| c.map(|c| c.start))
            .unwrap_or(usize::MAX);
        if score >= best_score {
            continue;
        }
        best_score = score;
        best = Some((hit, injection.priority));
        if score == line_pos {
            break;
        }
    }

    best
}

/// `-1` priority = the injection preempts a grammar match that lands
/// at the same offset. Matches upstream's "priorityMatch" flag.
const PRIORITY_MATCH: i8 = -1;

fn first_capture_start(captures: &[Option<CaptureIndex>]) -> usize {
    captures
        .first()
        .and_then(|c| c.map(|c| c.start))
        .unwrap_or(usize::MAX)
}

fn handle_captures<G, S, T>(
    grammar: &G,
    line_text: &str,
    is_first_line: bool,
    stack: &S,
    sink: &mut T,
    captures: &[Option<RuleId>],
    capture_indices: &[Option<CaptureIndex>],
) where
    G: GrammarRuntime,
    S: StateStack,
    T: TokenSink<S::Attr>,
{
    #[derive(Clone)]
    struct LocalEntry<A> {
        scopes: A,
        end_pos: usize,
    }

    if captures.is_empty() {
        return;
    }

    let len = captures.len().min(capture_indices.len());
    let Some(first) = capture_indices.first().and_then(|c| c.as_ref()) else {
        return;
    };
    let max_end = first.end;

    let mut local_stack: Vec<LocalEntry<S::Attr>> = Vec::new();

    for i in 0..len {
        let capture_rule_id = captures[i];
        let Some(capture_index) = capture_indices[i].as_ref() else {
            continue;
        };
        if capture_index.length == 0 {
            continue;
        }
        if capture_index.start > max_end {
            break;
        }

        // Pop local-stack entries that ended before this capture's start.
        while let Some(top) = local_stack.last() {
            if top.end_pos > capture_index.start {
                break;
            }
            let top = local_stack.pop().expect("checked non-empty");
            sink.produce_from_scopes(Some(&top.scopes), top.end_pos);
        }

        if let Some(top) = local_stack.last() {
            sink.produce_from_scopes(Some(&top.scopes), capture_index.start);
        } else {
            produce(sink, stack.content_name_scopes_list(), capture_index.start);
        }

        let Some(capture_rule_id) = capture_rule_id else {
            continue;
        };
        let Some(rule_arc) = grammar.rule(capture_rule_id) else {
            continue;
        };
        let Rule::Capture(capture_rule) = rule_arc.as_ref() else {
            continue;
        };

        // Nested re-tokenization — out of scope for this module,
        // handled by the Grammar layer (turn 5) which owns the
        // recursion. We still emit the capture name scope so that
        // the coarse token boundaries look right.
        let _ = is_first_line;

        let base_scopes = local_stack
            .last()
            .map(|e| e.scopes.clone())
            .or_else(|| stack.content_name_scopes_list().cloned());

        let scope_name = capture_rule
            .header
            .name_with_captures(Some(line_text), Some(capture_indices));
        if let (Some(base), Some(name)) = (base_scopes, scope_name.as_deref()) {
            let scopes = base.push_attributed(Some(name), grammar);
            local_stack.push(LocalEntry {
                scopes,
                end_pos: capture_index.end,
            });
        }
    }

    while let Some(top) = local_stack.pop() {
        sink.produce_from_scopes(Some(&top.scopes), top.end_pos);
    }
}

fn produce<A: AttributedScopeStack, T: TokenSink<A>>(
    sink: &mut T,
    scopes: Option<&A>,
    end_index: usize,
) {
    sink.produce(scopes, end_index);
}

struct WhileCheckResult<S: StateStack> {
    stack: S,
    line_pos: usize,
    anchor_pos: i64,
    is_first_line: bool,
}

fn check_while_conditions<G, S, T>(
    grammar: &G,
    line_text: &str,
    mut is_first_line: bool,
    mut line_pos: usize,
    stack: S,
    sink: &mut T,
) -> WhileCheckResult<S>
where
    G: GrammarRuntime,
    S: StateStack,
    T: TokenSink<S::Attr>,
{
    // Initial anchor position: 0 if the `begin` rule captured EOL on
    // the previous line (`\G` should match at start of line), else -1.
    let mut anchor_pos: i64 = if stack.begin_rule_captured_eol() {
        0
    } else {
        -1
    };

    // Collect every BeginWhile frame bottom→top by walking parents.
    let mut while_frames: Vec<(S, crate::rule::RuleId)> = Vec::new();
    {
        let mut cursor: Option<S> = Some(stack.clone());
        while let Some(node) = cursor {
            let rule_id = node.rule_id();
            if let Some(rule) = grammar.rule(rule_id) {
                if matches!(rule.as_ref(), Rule::BeginWhile(_)) {
                    while_frames.push((node.clone(), rule_id));
                }
            }
            cursor = node.parent();
        }
    }

    // Track the stack we'll return: start with the full stack, prune
    // everything above a failing while-frame on the fly.
    let mut current_stack = stack;

    // Upstream iterates top-of-stack (innermost) → root; our walk
    // above already pushes bottom→top, so reverse into innermost-first
    // order.
    //
    // Actually upstream walks root-first (via repeated `pop()`), so
    // the collection order matches and we don't reverse.
    while let Some((frame, rule_id)) = while_frames.pop() {
        let end_rule = frame.end_rule().map(str::to_string);
        let ctx = ScanContext {
            line_text,
            line_pos,
            is_first_line,
            anchor_pos,
            end_rule: end_rule.as_deref(),
            rule_id,
        };

        if let Some(hit) = grammar.scan_while_rule(ctx) {
            let first_capture = hit.captures.first().and_then(|c| c.as_ref());
            let Some(capture) = first_capture else {
                // No captures — treat as if the while didn't match.
                current_stack = frame.pop().unwrap_or_else(|| frame.safe_pop());
                break;
            };

            // Emit captures for the while match, same as upstream
            // passes `whileCaptures` through `handle_captures`
            // (captures are carried on the rule itself; for the
            // purposes of stack pruning we only care about the
            // match outcome, so we just advance the positions).
            sink.produce(frame.content_name_scopes_list(), capture.start);
            sink.produce(frame.content_name_scopes_list(), capture.end);
            anchor_pos = i64::try_from(capture.end).unwrap_or(i64::MAX);
            if capture.end > line_pos {
                line_pos = capture.end;
                is_first_line = false;
            }
        } else {
            // While condition failed — pop this frame and stop.
            current_stack = frame.pop().unwrap_or_else(|| frame.safe_pop());
            break;
        }
    }

    WhileCheckResult {
        stack: current_stack,
        line_pos,
        anchor_pos,
        is_first_line,
    }
}
