import { InstantiationType, registerSingleton } from '../../../../platform/instantiation/common/extensions.js';
import { IRemoteExtensionsScannerService } from '../../../../platform/remote/common/remoteExtensionsScanner.js';

class NullRemoteExtensionsScannerService implements IRemoteExtensionsScannerService {
	declare readonly _serviceBrand: undefined;

	async whenExtensionsReady() {
		return { local: { added: [], removed: [] }, remote: { added: [], removed: [] } } as any;
	}

	async scanExtensions() {
		return [];
	}
}

registerSingleton(IRemoteExtensionsScannerService, NullRemoteExtensionsScannerService, InstantiationType.Delayed);
