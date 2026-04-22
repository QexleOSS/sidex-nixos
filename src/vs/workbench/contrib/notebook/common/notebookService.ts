/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { createDecorator } from '../../../../platform/instantiation/common/instantiation.js';
import { Event } from '../../../../base/common/event.js';
import { URI } from '../../../../base/common/uri.js';

export const INotebookService = createDecorator<INotebookService>('notebookService');

export interface INotebookContentProvider {
	readonly options: any;
}

export interface NotebookTypeDescriptor {
	readonly id: string;
	readonly displayName: string;
}

export interface INotebookService {
	readonly _serviceBrand: undefined;

	readonly onAddViewType: Event<string>;
	readonly onWillRemoveViewType: Event<string>;
	readonly onDidChangeOutputRenderers: Event<void>;

	canResolve(viewType: string): Promise<boolean>;
	getContributedNotebookTypes(resource?: URI): readonly NotebookTypeDescriptor[];
	withNotebookDataProvider(viewType: string): Promise<{ serializer: any }>;
	getNotebookTextModel(uri: URI): any | undefined;
	getMimeTypeInfo(textModel: any, kernelProvides: readonly string[] | undefined, output: any): readonly any[];
}
