/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { URI } from '../../../base/common/uri.js';
import { ILanguagePackItem, ILanguagePackService } from '../common/languagePacks.js';

export class WebLanguagePacksService implements ILanguagePackService {
	declare readonly _serviceBrand: undefined;

	async getBuiltInExtensionTranslationsUri(_id: string, _language: string): Promise<URI | undefined> {
		return undefined;
	}

	async getAvailableLanguages(): Promise<ILanguagePackItem[]> {
		return [];
	}

	async getInstalledLanguages(): Promise<ILanguagePackItem[]> {
		return [];
	}
}
