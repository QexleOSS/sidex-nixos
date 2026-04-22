/*---------------------------------------------------------------------------------------------
 *  SideX: Stub for removed custom editor service.
 *--------------------------------------------------------------------------------------------*/

import { createDecorator } from '../../../../platform/instantiation/common/instantiation.js';

export interface ICustomEditorModel {
	readonly viewType: string;
}

export const ICustomEditorService = createDecorator<ICustomEditorService>('customEditorService');
export interface ICustomEditorService {
	readonly _serviceBrand: undefined;
}
