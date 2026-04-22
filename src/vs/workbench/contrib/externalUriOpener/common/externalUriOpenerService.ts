/*---------------------------------------------------------------------------------------------
 *  SideX: Stub for removed external URI opener service.
 *--------------------------------------------------------------------------------------------*/

import { CancellationToken } from '../../../../base/common/cancellation.js';
import { URI } from '../../../../base/common/uri.js';
import { createDecorator } from '../../../../platform/instantiation/common/instantiation.js';

export interface IExternalUriOpener {
	canOpen(uri: URI, token: CancellationToken): Promise<unknown>;
	openExternalUri(uri: URI, ctx: unknown, token: CancellationToken): Promise<boolean>;
}

export interface IExternalOpenerProvider {
	getOpeners(uri: URI): AsyncIterable<IExternalUriOpener>;
}

export const IExternalUriOpenerService = createDecorator<IExternalUriOpenerService>('externalUriOpenerService');
export interface IExternalUriOpenerService {
	readonly _serviceBrand: undefined;
	registerExternalOpenerProvider(provider: IExternalOpenerProvider): { dispose(): void };
}
