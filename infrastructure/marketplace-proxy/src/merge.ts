import type { NormalizedExtension } from './types';

/**
 * Dedupe + merge results from MS Marketplace and Open VSX.
 *
 * Policy:
 *   - An extension is keyed by `publisher.name` (lowercased).
 *   - If both backends return the same id, prefer the one with the
 *     higher install count. This lets popular MS-only extensions win
 *     while still allowing Open VSX exclusives (e.g. `redhat.vscode-*`
 *     mirrors) to surface when they're the only source.
 *   - The merged list is stable-sorted by install count, then rating,
 *     then display name so the client sees a deterministic order.
 */
export function mergeResults(ms: NormalizedExtension[], openvsx: NormalizedExtension[]): NormalizedExtension[] {
	const byId = new Map<string, NormalizedExtension>();

	const insert = (ext: NormalizedExtension): void => {
		const key = ext.id.toLowerCase();
		const existing = byId.get(key);
		if (!existing) {
			byId.set(key, ext);
			return;
		}
		if (ext.installCount > existing.installCount) {
			byId.set(key, ext);
		}
	};

	for (const ext of ms) {
		insert(ext);
	}
	for (const ext of openvsx) {
		insert(ext);
	}

	return Array.from(byId.values()).sort((a, b) => {
		if (b.installCount !== a.installCount) {
			return b.installCount - a.installCount;
		}
		if (b.rating !== a.rating) {
			return b.rating - a.rating;
		}
		return a.displayName.localeCompare(b.displayName);
	});
}
