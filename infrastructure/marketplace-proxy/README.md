# SideX Marketplace Proxy

A Cloudflare Worker that gives SideX a Cursor-style extension
marketplace: one endpoint, two backends (Microsoft Marketplace + Open
VSX), merged and cached at the edge.

## Why a Worker (vs self-hosting)

- No origin to operate. Runs globally on Cloudflare's edge network.
- Sub-50ms cache hits from the closest POP.
- Two-tier cache: Cloudflare Cache API (per-POP) + KV (global).
- `Promise.allSettled` against both backends → if one is down, the
  other still serves results.

## Endpoints

### `GET /api/search?q=<query>&pageSize=<n>`

Searches both Microsoft Marketplace and Open VSX in parallel and
returns a merged, deduped list.

```json
{
  "results": [
    {
      "id": "ms-vscode.cpptools",
      "name": "cpptools",
      "displayName": "C/C++",
      "description": "...",
      "version": "1.19.0",
      "publisher": "ms-vscode",
      "installCount": 70000000,
      "rating": 4.3,
      "iconUrl": "https://...",
      "downloadUrl": "https://...vsix",
      "source": "microsoft"
    }
  ],
  "totalCount": 1,
  "sources": {
    "microsoft": { "ok": true, "count": 50 },
    "openvsx": { "ok": true, "count": 42 }
  }
}
```

- Cached 5 min at the edge and in KV.
- Response header `x-sidex-cache` is `edge | kv | miss`.

### `GET /api/download/<source>/<base64url-url>`

Proxies a VSIX download. `source` is `microsoft` or `openvsx`. The
upstream URL is base64url-encoded and validated against an allowlist
of hosts before it's fetched. Responses are cached 24h at the edge.

### `GET /healthz`

Liveness check. Returns `ok`.

## Deploy

```bash
cd infrastructure/marketplace-proxy
npm install
npx wrangler kv namespace create MARKETPLACE_CACHE
npx wrangler kv namespace create MARKETPLACE_CACHE --preview
# Paste the returned ids into wrangler.toml
npx wrangler deploy
```

> Wrangler v4+ uses `kv namespace` (space), not `kv:namespace` (colon) —
> the older colon form was removed.

## Point SideX at the worker

After deploy, grab your worker URL (e.g.
`https://sidex-marketplace-proxy.<account>.workers.dev`) and set:

- `crates/sidex-extensions/src/marketplace.rs` →
  `DEFAULT_BASE_URL = "https://<worker>"`
- `src/vs/platform/product/common/product.ts` →
  `extensionsGallery.serviceUrl = "https://<worker>/api/search"`

## Local dev

```bash
npm install
npx wrangler dev
# Worker is served at http://localhost:8787
curl 'http://localhost:8787/api/search?q=python&pageSize=10'
```
