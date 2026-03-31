/*---------------------------------------------------------------------------------------------
 *  SideX — Tauri-based VSCode port
 *  Entry point. Globals set by inline script in index.html.
 *--------------------------------------------------------------------------------------------*/

async function boot() {
	// Import the web workbench barrel — catch errors but continue
	try {
		await import('./vs/workbench/workbench.web.main.js');
	} catch (e) {
		console.warn('[SideX] Some workbench modules failed to load (non-fatal):', e);
	}

	const { create } = await import('./vs/workbench/browser/web.factory.js');

	if (document.readyState === 'loading') {
		await new Promise<void>(r => window.addEventListener('DOMContentLoaded', () => r()));
	}

	// Register a file system provider for file:// that uses our Tauri backend
	const { InMemoryFileSystemProvider } = await import('./vs/platform/files/common/inMemoryFilesystemProvider.js');

	create(document.body, {
		windowIndicator: {
			label: 'SideX',
			tooltip: 'SideX — Tauri Code Editor',
			command: undefined as any,
		},
		productConfiguration: {
			nameShort: 'SideX',
			nameLong: 'SideX',
			applicationName: 'sidex',
			dataFolderName: '.sidex',
			version: '0.1.0',
		} as any,
		settingsSyncOptions: {
			enabled: false,
		},
		additionalBuiltinExtensions: [],
		defaultLayout: {
			editors: [],
		},
	});

	console.log('[SideX] Workbench created successfully');
}

boot().catch((err) => {
	console.error('[SideX] Fatal:', err);
	document.body.innerHTML = `<div style="padding:40px;color:#ccc;font-family:system-ui">
		<h2>SideX failed to start</h2>
		<pre style="color:#f88;white-space:pre-wrap">${(err as Error)?.stack || err}</pre>
	</div>`;
});
