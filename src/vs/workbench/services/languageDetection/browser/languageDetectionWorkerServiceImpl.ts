/*---------------------------------------------------------------------------------------------
 *  SideX — Language detection backed by the `sidex-syntax` Rust crate.
 *
 *  VS Code's upstream implementation shells out to an ONNX Guesslang model in
 *  a web worker. We keep the same `ILanguageDetectionService` shape but let
 *  the Rust registry do the work: filename / extension matching first, then
 *  shebang / first-line regex fallback. Everything is a single Tauri call
 *  so the status bar and untitled-editor flows still get a crisp answer.
 *--------------------------------------------------------------------------------------------*/

import { URI } from '../../../../base/common/uri.js';
import { InstantiationType, registerSingleton } from '../../../../platform/instantiation/common/extensions.js';
import { IFileService } from '../../../../platform/files/common/files.js';
import { ILanguageDetectionService } from '../common/languageDetectionWorkerService.js';

interface TauriCore {
	invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T>;
}

async function loadInvoke(): Promise<TauriCore['invoke'] | undefined> {
	try {
		const mod = await import('@tauri-apps/api/core');
		return mod.invoke;
	} catch {
		return undefined;
	}
}

// How much of a file we read to answer "what language is this?". 4 KiB is
// enough for a shebang or a first-line hint without blocking on huge blobs.
const CONTENT_PEEK_BYTES = 4 * 1024;

export class LanguageDetectionService implements ILanguageDetectionService {
	declare readonly _serviceBrand: undefined;

	private readonly _invoke: Promise<TauriCore['invoke'] | undefined>;

	constructor(@IFileService private readonly fileService: IFileService) {
		this._invoke = loadInvoke();
	}

	isEnabledForLanguage(languageId: string): boolean {
		// VS Code disables detection for plaintext/jsonc/binary — match that.
		if (!languageId || languageId === 'plaintext') {
			return false;
		}
		return true;
	}

	async detectLanguage(resource: URI, supportedLangs?: string[]): Promise<string | undefined> {
		const invoke = await this._invoke;
		if (!invoke) {
			return undefined;
		}

		const filename = resource.path.length > 0 ? resource.path : undefined;
		const content = await this.peekContent(resource);

		try {
			const detected = await invoke<string | null>('syntax_detect_from_content', {
				filename,
				content,
				supported: supportedLangs
			});
			return detected ?? undefined;
		} catch {
			return undefined;
		}
	}

	private async peekContent(resource: URI): Promise<string | undefined> {
		try {
			const buffer = await this.fileService.readFile(resource, {
				position: 0,
				length: CONTENT_PEEK_BYTES
			});
			return buffer.value.toString();
		} catch {
			return undefined;
		}
	}
}

registerSingleton(ILanguageDetectionService, LanguageDetectionService, InstantiationType.Eager);
