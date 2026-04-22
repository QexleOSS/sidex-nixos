/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { URI } from '../../../base/common/uri.js';
import { IQuickPickItem } from '../../quickinput/common/quickInput.js';
import { IGalleryExtension } from '../../extensionManagement/common/extensionManagement.js';
import { createDecorator } from '../../instantiation/common/instantiation.js';

export function getLocale(extension: IGalleryExtension): string | undefined {
	return extension.tags.find(t => t.startsWith('lp-'))?.split('lp-')[1];
}

export const ILanguagePackService = createDecorator<ILanguagePackService>('languagePackService');

export interface ILanguagePackItem extends IQuickPickItem {
	readonly extensionId?: string;
	readonly galleryExtension?: IGalleryExtension;
}

export interface ILanguagePackService {
	readonly _serviceBrand: undefined;
	getAvailableLanguages(): Promise<Array<ILanguagePackItem>>;
	getInstalledLanguages(): Promise<Array<ILanguagePackItem>>;
	getBuiltInExtensionTranslationsUri(id: string, language: string): Promise<URI | undefined>;
}
