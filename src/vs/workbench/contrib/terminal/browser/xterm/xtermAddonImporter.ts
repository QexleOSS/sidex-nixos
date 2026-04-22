/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import type { ClipboardAddon as ClipboardAddonType } from '@xterm/addon-clipboard';
import type { ImageAddon as ImageAddonType } from '@xterm/addon-image';
import type { LigaturesAddon as LigaturesAddonType } from '@xterm/addon-ligatures';
import type { ProgressAddon as ProgressAddonType } from '@xterm/addon-progress';
import type { SearchAddon as SearchAddonType } from '@xterm/addon-search';
import type { SerializeAddon as SerializeAddonType } from '@xterm/addon-serialize';
import type { Unicode11Addon as Unicode11AddonType } from '@xterm/addon-unicode11';
import type { WebglAddon as WebglAddonType } from '@xterm/addon-webgl';

export interface IXtermAddonNameToCtor {
	clipboard: typeof ClipboardAddonType;
	image: typeof ImageAddonType;
	ligatures: typeof LigaturesAddonType;
	progress: typeof ProgressAddonType;
	search: typeof SearchAddonType;
	serialize: typeof SerializeAddonType;
	unicode11: typeof Unicode11AddonType;
	webgl: typeof WebglAddonType;
}

// This interface lets a maps key and value be linked with generics
interface IImportedXtermAddonMap extends Map<
	keyof IXtermAddonNameToCtor,
	IXtermAddonNameToCtor[keyof IXtermAddonNameToCtor]
> {
	get<K extends keyof IXtermAddonNameToCtor>(name: K): IXtermAddonNameToCtor[K] | undefined;
	set<K extends keyof IXtermAddonNameToCtor>(name: K, value: IXtermAddonNameToCtor[K]): this;
}

const importedAddons: IImportedXtermAddonMap = new Map();

/**
 * Exposes a simple interface to consumers, encapsulating the messy import xterm
 * addon import and caching logic.
 */
export class XtermAddonImporter {
	async importAddon<T extends keyof IXtermAddonNameToCtor>(name: T): Promise<IXtermAddonNameToCtor[T]> {
		let addon = importedAddons.get(name);
		if (!addon) {
			try {
				switch (name) {
					case 'clipboard':
						addon = (await import('@xterm/addon-clipboard')).ClipboardAddon as IXtermAddonNameToCtor[T];
						break;
					case 'image':
						addon = (await import('@xterm/addon-image')).ImageAddon as IXtermAddonNameToCtor[T];
						break;
					case 'search':
						addon = (await import('@xterm/addon-search')).SearchAddon as IXtermAddonNameToCtor[T];
						break;
					case 'serialize':
						addon = (await import('@xterm/addon-serialize')).SerializeAddon as IXtermAddonNameToCtor[T];
						break;
					case 'unicode11':
						addon = (await import('@xterm/addon-unicode11')).Unicode11Addon as IXtermAddonNameToCtor[T];
						break;
					case 'webgl':
						addon = (await import('@xterm/addon-webgl')).WebglAddon as IXtermAddonNameToCtor[T];
						break;
					case 'progress':
						addon = (await import('@xterm/addon-progress')).ProgressAddon as IXtermAddonNameToCtor[T];
						break;
					case 'ligatures':
						addon = (await import('@xterm/addon-ligatures')).LigaturesAddon as IXtermAddonNameToCtor[T];
						break;
				}
			} catch (e) {
				console.warn(`[SideX] Failed to load xterm addon '${name}':`, e);
				throw new Error(`Could not load addon ${name}`);
			}
			if (!addon) {
				throw new Error(`Could not load addon ${name}`);
			}
			importedAddons.set(name, addon);
		}
		return addon as IXtermAddonNameToCtor[T];
	}
}
