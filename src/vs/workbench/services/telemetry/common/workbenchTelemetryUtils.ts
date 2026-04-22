/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { IConfigurationService } from '../../../../platform/configuration/common/configuration.js';
import { IProductService } from '../../../../platform/product/common/productService.js';
import { IWorkbenchEnvironmentService } from '../../environment/common/environmentService.js';

// Stub: telemetry experiments now handled by Rust telemetry crate
export function experimentsEnabled(
	_configurationService: IConfigurationService,
	_productService: IProductService,
	_environmentService: IWorkbenchEnvironmentService
): boolean {
	return false;
}
