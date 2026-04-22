/*---------------------------------------------------------------------------------------------
 *  SideX
 *  Minimal stubs for accessibility commands. The real VS Code implementations
 *  are `MultiCommand` instances exposing `addImplementation`, which is called
 *  by terminal / editor accessibility contributions during startup. Since our
 *  accessible-view flows are handled by the workbench itself, these only need
 *  to accept implementations without registering anything.
 *--------------------------------------------------------------------------------------------*/

import { IDisposable } from '../../../../base/common/lifecycle.js';

type AccessibleViewImplementation = (...args: unknown[]) => boolean | void | Promise<void>;

class NoopMultiCommand {
	private readonly implementations: Array<[number, string, AccessibleViewImplementation]> = [];

	addImplementation(
		priority: number,
		name: string,
		implementation: AccessibleViewImplementation,
		_when?: unknown
	): IDisposable {
		this.implementations.push([priority, name, implementation]);
		return {
			dispose: () => {
				const idx = this.implementations.findIndex(([, , fn]) => fn === implementation);
				if (idx !== -1) {
					this.implementations.splice(idx, 1);
				}
			}
		};
	}
}

export const AccessibilityHelpAction = new NoopMultiCommand();
export const AccessibleViewAction = new NoopMultiCommand();
