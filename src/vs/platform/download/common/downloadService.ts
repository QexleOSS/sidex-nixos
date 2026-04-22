/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { URI } from '../../../base/common/uri.js';
import { IDownloadService } from './download.js';

export class DownloadService implements IDownloadService {
	declare readonly _serviceBrand: undefined;

	async download(_resource: URI, _target: URI, _callSite: string): Promise<void> {
		throw new Error('Download service is handled by Rust runtime');
	}
}
