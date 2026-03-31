/*---------------------------------------------------------------------------------------------
 *  SideX - Tauri-based VSCode port
 *  This is the Tauri equivalent of VSCode's desktop.main.ts
 *  It bootstraps the real VSCode workbench through Tauri instead of Electron
 *--------------------------------------------------------------------------------------------*/

import product from '../../platform/product/common/product.js';
import { Workbench } from '../../workbench/browser/workbench.js';
import { domContentLoaded } from '../../base/browser/dom.js';
import { onUnexpectedError } from '../../base/common/errors.js';
import { Emitter } from '../../base/common/event.js';
import { ServiceCollection } from '../../platform/instantiation/common/serviceCollection.js';
import { ILogService, LogLevel } from '../../platform/log/common/log.js';
import { IWorkspaceContextService } from '../../platform/workspace/common/workspace.js';
import { IWorkbenchConfigurationService } from '../../workbench/services/configuration/common/configuration.js';
import { IStorageService } from '../../platform/storage/common/storage.js';
import { Disposable } from '../../base/common/lifecycle.js';
import { FileService } from '../../platform/files/common/fileService.js';
import { IFileService } from '../../platform/files/common/files.js';
import { ISignService } from '../../platform/sign/common/sign.js';
import { IProductService } from '../../platform/product/common/productService.js';
import { IUriIdentityService } from '../../platform/uriIdentity/common/uriIdentity.js';
import { UriIdentityService } from '../../platform/uriIdentity/common/uriIdentityService.js';
import { Schemas } from '../../base/common/network.js';
import { mainWindow } from '../../base/browser/window.js';
import { BrowserStorageService } from '../../workbench/services/storage/browser/storageService.js';
import { BrowserWorkbenchEnvironmentService } from '../../workbench/services/environment/browser/environmentService.js';
import { IWorkbenchEnvironmentService } from '../../workbench/services/environment/common/environmentService.js';
import { IRemoteAgentService } from '../../workbench/services/remote/common/remoteAgentService.js';
import { RemoteAgentService } from '../../workbench/services/remote/browser/remoteAgentService.js';
import { IRemoteAuthorityResolverService } from '../../platform/remote/common/remoteAuthorityResolver.js';
import { RemoteAuthorityResolverService } from '../../platform/remote/browser/remoteAuthorityResolverService.js';
import { IRemoteSocketFactoryService, RemoteSocketFactoryService } from '../../platform/remote/common/remoteSocketFactoryService.js';
import { BrowserSocketFactory } from '../../platform/remote/browser/browserSocketFactory.js';
import { ConfigurationCache } from '../../workbench/services/configuration/common/configurationCache.js';
import { WorkspaceService } from '../../workbench/services/configuration/browser/configurationService.js';
import { NullPolicyService } from '../../platform/policy/common/policy.js';

class ConsoleLogService extends Disposable implements ILogService {
	declare readonly _serviceBrand: undefined;
	private readonly _onDidChangeLogLevel = this._register(new Emitter<LogLevel>());
	readonly onDidChangeLogLevel = this._onDidChangeLogLevel.event;
	getLevel() { return LogLevel.Info; }
	setLevel(_level: LogLevel) { }
	trace(message: string, ...args: any[]) { console.trace(message, ...args); }
	debug(message: string, ...args: any[]) { console.debug(message, ...args); }
	info(message: string, ...args: any[]) { console.info(message, ...args); }
	warn(message: string, ...args: any[]) { console.warn(message, ...args); }
	error(message: string | Error, ...args: any[]) { console.error(message, ...args); }
	flush() { }
}

export class TauriMain extends Disposable {

	constructor() {
		super();
	}

	async open(): Promise<void> {
		const services = this.initServices();

		await domContentLoaded(mainWindow);

		const workbench = new Workbench(
			mainWindow.document.body,
			{ extraClasses: ['tauri'] },
			services.serviceCollection,
			services.logService,
		);

		this._register(workbench);

		workbench.startup();
	}

	private initServices(): { serviceCollection: ServiceCollection; logService: ILogService } {
		const serviceCollection = new ServiceCollection();

		const productService: IProductService = { _serviceBrand: undefined, ...product } as any;
		serviceCollection.set(IProductService, productService);

		const environmentService = new BrowserWorkbenchEnvironmentService(
			'SideX',
			productService,
			undefined,
			undefined
		);
		serviceCollection.set(IWorkbenchEnvironmentService, environmentService);

		const logService = this._register(new ConsoleLogService());
		serviceCollection.set(ILogService, logService);

		const signService: ISignService = {
			_serviceBrand: undefined,
			async sign(value: string) { return value; },
			async validate(_signedValue: string, _value: string) { return true; }
		} as any;
		serviceCollection.set(ISignService, signService);

		const remoteAuthorityResolverService = new RemoteAuthorityResolverService(
			false, undefined, undefined, productService, logService
		);
		serviceCollection.set(IRemoteAuthorityResolverService, remoteAuthorityResolverService);

		const remoteSocketFactoryService = new RemoteSocketFactoryService();
		remoteSocketFactoryService.register(0, new BrowserSocketFactory(null));
		serviceCollection.set(IRemoteSocketFactoryService, remoteSocketFactoryService);

		const remoteAgentService = this._register(new RemoteAgentService(
			remoteSocketFactoryService, environmentService, productService,
			remoteAuthorityResolverService, signService, logService
		));
		serviceCollection.set(IRemoteAgentService, remoteAgentService);

		const fileService = this._register(new FileService(logService));
		serviceCollection.set(IFileService, fileService);

		const uriIdentityService = new UriIdentityService(fileService);
		serviceCollection.set(IUriIdentityService, uriIdentityService);

		const configurationCache = new ConfigurationCache(
			[Schemas.file, Schemas.vscodeUserData, Schemas.tmp],
			environmentService, fileService
		);
		const workspaceService = new WorkspaceService(
			{ remoteAuthority: undefined, configurationCache },
			environmentService, fileService, remoteAgentService, uriIdentityService,
			logService, new NullPolicyService()
		);
		serviceCollection.set(IWorkbenchConfigurationService, workspaceService);
		serviceCollection.set(IWorkspaceContextService, workspaceService);

		const storageService = this._register(new BrowserStorageService(
			{ id: 'sidex-workspace' }, logService
		));
		serviceCollection.set(IStorageService, storageService);

		return { serviceCollection, logService };
	}
}

export async function main(): Promise<void> {
	const tauriMain = new TauriMain();
	try {
		await tauriMain.open();
	} catch (error) {
		onUnexpectedError(error);
		console.error('[SideX] Failed to boot workbench:', error);
	}
}
