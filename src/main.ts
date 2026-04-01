/*---------------------------------------------------------------------------------------------
 *  SideX — Tauri-based VSCode port
 *  Entry point. Globals set by inline script in index.html.
 *--------------------------------------------------------------------------------------------*/

async function boot() {
	const stages = [
		['common',       () => import('./vs/workbench/workbench.common.main.js')],
		['web.main',     () => import('./vs/workbench/browser/web.main.js')],
		['web-dialog',   () => import('./vs/workbench/browser/parts/dialogs/dialog.web.contribution.js')],
		['web-services', () => import('./vs/workbench/workbench.web.main.js')],
	] as const;

	for (const [label, loader] of stages) {
		try {
			await loader();
		} catch (e) {
			console.warn(`[SideX] Barrel stage "${label}" failed (non-fatal):`, e);
		}
	}

	const { create } = await import('./vs/workbench/browser/web.factory.js');
	const { URI } = await import('./vs/base/common/uri.js');

	if (document.readyState === 'loading') {
		await new Promise<void>(r => window.addEventListener('DOMContentLoaded', () => r()));
	}

	const urlParams = new URLSearchParams(window.location.search);
	const folderParam = urlParams.get('folder');

	// Build the workspace provider — this is how VSCode web knows what folder to open
	let workspace: any = undefined;
	if (folderParam) {
		workspace = { folderUri: URI.parse(folderParam) };
	}

	const options: any = {
		// The workspace provider tells VSCode what folder/workspace to open
		workspaceProvider: {
			workspace,
			trusted: true,
			open: async (_workspace: any, _options: any) => {
				// When VSCode asks to open a new workspace, reload with the folder param
				if (_workspace && 'folderUri' in _workspace) {
					const url = new URL(window.location.href);
					url.searchParams.set('folder', _workspace.folderUri.toString());
					window.location.href = url.toString();
				}
				return true;
			},
		},
		windowIndicator: {
			label: folderParam ? decodeURIComponent(folderParam.split('/').pop() || 'SideX') : 'SideX',
			tooltip: 'SideX — Tauri Code Editor',
			command: undefined,
		},
		productConfiguration: {
			nameShort: 'SideX',
			nameLong: 'SideX',
			applicationName: 'sidex',
			dataFolderName: '.sidex',
			version: '0.1.0',
		},
		settingsSyncOptions: {
			enabled: false,
		},
		additionalBuiltinExtensions: [],
		defaultLayout: {
			editors: [],
			layout: { editors: {} },
		},
		configurationDefaults: {
			'workbench.startupEditor': 'none',
			'workbench.enableExperiments': false,
			'window.menuBarVisibility': 'classic',
			'window.titleBarStyle': 'custom',
			'telemetry.telemetryLevel': 'off',
			'update.mode': 'none',
			'extensions.autoUpdate': false,
			'extensions.autoCheckUpdates': false,
			'workbench.settings.enableNaturalLanguageSearch': false,
		},
	};

	create(document.body, options);

	console.log('[SideX] Workbench created' + (folderParam ? ` (folder: ${folderParam})` : ' (no folder)'), 'workspace:', workspace);
}

boot().catch((err) => {
	console.error('[SideX] Fatal:', err);
	document.body.innerHTML = `<div style="padding:40px;color:#ccc;font-family:system-ui">
		<h2>SideX failed to start</h2>
		<pre style="color:#f88;white-space:pre-wrap">${(err as Error)?.stack || err}</pre>
	</div>`;
});
