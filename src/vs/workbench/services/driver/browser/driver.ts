/*---------------------------------------------------------------------------------------------
 *  SideX — Driver stub.
 *
 *  VS Code's driver is a Playwright-oriented automation harness used only by
 *  smoke tests (`--enable-smoke-test-driver`). SideX doesn't run those tests
 *  today, so this is a no-op that preserves the public signature so the
 *  workbench can keep calling `registerWindowDriver` unconditionally.
 *--------------------------------------------------------------------------------------------*/

import { IInstantiationService } from '../../../../platform/instantiation/common/instantiation.js';

export function registerWindowDriver(_instantiationService: IInstantiationService): void {
	// intentionally empty — smoke-test hook
}
