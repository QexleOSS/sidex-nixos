/*---------------------------------------------------------------------------------------------
 *  SideX WASM SHA-1 Bridge
 *  Accelerated SHA-1 hashing via WebAssembly with transparent JS fallback.
 *--------------------------------------------------------------------------------------------*/

let wasmModule: any = null;
let initPromise: Promise<void> | null = null;
let initFailed = false;

async function ensureWasm(): Promise<any> {
	if (wasmModule) {
		return wasmModule;
	}
	if (initFailed) {
		return null;
	}
	if (!initPromise) {
		initPromise = (async () => {
			try {
				const wasmPath = '/wasm/hash/sidex_hash_wasm.js';
				const mod = await import(/* @vite-ignore */ wasmPath);
				await mod.default();
				wasmModule = mod;
			} catch (e) {
				console.warn('[SideX] WASM hash module not available, using JS fallback', e);
				initFailed = true;
			}
		})();
	}
	await initPromise;
	return wasmModule;
}

ensureWasm();

export function wasmSha1(input: string): string | null {
	if (!wasmModule) {
		return null;
	}
	return wasmModule.sha1_hash(input);
}

export function wasmSha1Streaming(): WasmSha1Handle | null {
	if (!wasmModule) {
		return null;
	}
	const instance = new wasmModule.Sha1();
	return {
		update(str: string) {
			instance.update_str(str);
		},
		digest(): string {
			return instance.digest();
		},
	};
}

export interface WasmSha1Handle {
	update(str: string): void;
	digest(): string;
}

export function isHashWasmReady(): boolean {
	return wasmModule !== null;
}
