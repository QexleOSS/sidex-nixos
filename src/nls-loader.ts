type Translations = Record<string, string>;

export async function loadNlsMessages(): Promise<void> {
	const locale = localStorage.getItem('vscode.nls.locale');
	if (!locale || locale.toLowerCase().startsWith('en')) {
		return;
	}

	const extensionId = localStorage.getItem('vscode.nls.languagePackExtensionId');
	if (!extensionId) {
		return;
	}

	try {
		const indexRes = await fetch('/nls.messages.json');
		if (!indexRes.ok) {
			return;
		}
		const nlsEntries: Array<{ key: string; msg: string }> = await indexRes.json();

		const translations = (await loadFromDisk(extensionId)) ?? (await loadFromGallery(extensionId));

		if (!translations) {
			return;
		}

		(globalThis as any)._VSCODE_NLS_MESSAGES = nlsEntries.map(({ key, msg }) => translations[key] ?? msg);
		(globalThis as any)._VSCODE_NLS_LANGUAGE = locale;
	} catch {
		// Non-fatal — UI stays in English
	}
}

async function loadFromDisk(extensionId: string): Promise<Translations | null> {
	try {
		const { invoke } = await import('@tauri-apps/api/core');
		const homedir = await invoke<string>('get_env', { key: 'HOME' });
		if (!homedir) {
			return null;
		}
		const raw = await invoke<string>('read_file', {
			path: `${homedir}/.sidex/extensions/${extensionId}/translations/main.i18n.json`
		});
		return parseBundle(raw);
	} catch {
		return null;
	}
}

async function loadFromGallery(extensionId: string): Promise<Translations | null> {
	const [publisher, name] = extensionId.split('.');
	if (!publisher || !name) {
		return null;
	}
	try {
		const meta = await fetch(`https://open-vsx.org/api/${publisher}/${name}/latest`).then(r =>
			r.ok ? r.json() : null
		);
		if (!meta?.version) {
			return null;
		}
		const res = await fetch(
			`https://open-vsx.org/vscode/unpkg/${publisher}/${name}/${meta.version}/extension/translations/main.i18n.json`
		);
		return res.ok ? parseBundle(await res.text()) : null;
	} catch {
		return null;
	}
}

function parseBundle(raw: string): Translations | null {
	try {
		const bundles = JSON.parse(raw)?.contents;
		if (!bundles) {
			return null;
		}
		const messages: Translations = {};
		for (const bundle of Object.values(bundles)) {
			Object.assign(messages, bundle);
		}
		return messages;
	} catch {
		return null;
	}
}
