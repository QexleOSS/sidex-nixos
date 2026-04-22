/**
 * Cheap, collision-resistant ETag for response bodies.
 *
 * Uses FNV-1a 64-bit instead of SHA because ETags don't need to be
 * cryptographic — they just need to change when the body changes.
 * Running SHA-256 on every search response would cost us a few ms per
 * request at scale; FNV-1a is ~10 ns per KB.
 */
export function etagFor(body: string): string {
	let hash = 0xcbf29ce484222325n;
	const prime = 0x100000001b3n;
	for (let i = 0; i < body.length; i++) {
		hash ^= BigInt(body.charCodeAt(i));
		hash = (hash * prime) & 0xffffffffffffffffn;
	}
	return `W/"${hash.toString(36)}"`;
}

/**
 * Checks If-None-Match / If-Modified-Since against the given etag.
 * Returns a 304 response if the client already has the latest copy,
 * or `null` if the full body should be sent.
 */
export function notModifiedResponse(request: Request, etag: string): Response | null {
	const ifNoneMatch = request.headers.get('if-none-match');
	if (ifNoneMatch && matchesEtag(ifNoneMatch, etag)) {
		return new Response(null, {
			status: 304,
			headers: {
				etag,
				'cache-control': 'public, max-age=300, stale-while-revalidate=1800'
			}
		});
	}
	return null;
}

function matchesEtag(ifNoneMatch: string, etag: string): boolean {
	// Handle "*" wildcard and multiple etag values (comma-separated).
	if (ifNoneMatch.trim() === '*') {
		return true;
	}
	const incoming = ifNoneMatch.split(',').map(s => s.trim());
	return incoming.some(tag => tag === etag || tag === etag.replace(/^W\//, ''));
}
