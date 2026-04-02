/*---------------------------------------------------------------------------------------------
 *  SideX WASM TF-IDF Bridge
 *  Accelerated TF-IDF scoring via WebAssembly with transparent JS fallback.
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
				const wasmPath = '/wasm/tfidf/sidex_tfidf_wasm.js';
				const mod = await import(/* @vite-ignore */ wasmPath);
				await mod.default();
				wasmModule = mod;
			} catch (e) {
				console.warn('[SideX] WASM tfidf module not available, using JS fallback', e);
				initFailed = true;
			}
		})();
	}
	await initPromise;
	return wasmModule;
}

ensureWasm();

export interface WasmTfIdfEngine {
	updateDocument(key: string, chunks: string[]): void;
	deleteDocument(key: string): void;
	calculateScores(query: string): Array<{ key: string; score: number }>;
	free(): void;
}

export function createWasmTfIdfEngine(): WasmTfIdfEngine | null {
	if (!wasmModule) {
		return null;
	}

	const engine = new wasmModule.TfIdfEngine();

	return {
		updateDocument(key: string, chunks: string[]) {
			engine.update_document(key, JSON.stringify(chunks));
		},
		deleteDocument(key: string) {
			engine.delete_document(key);
		},
		calculateScores(query: string): Array<{ key: string; score: number }> {
			const json = engine.calculate_scores(query);
			try {
				return JSON.parse(json);
			} catch {
				return [];
			}
		},
		free() {
			engine.free();
		},
	};
}

export function isWasmTfIdfReady(): boolean {
	return wasmModule !== null;
}
