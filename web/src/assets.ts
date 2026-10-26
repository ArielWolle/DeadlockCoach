interface AssetEntry { bytes: number; sha256: string }
type AssetCache = Pick<Cache, 'match' | 'put' | 'delete'>;

/** Content-addressed requests also bypass older, unversioned service-worker caches. */
export async function loadVerifiedAsset(base: string, name: string, expected: AssetEntry,
  cache?: AssetCache, fetchAsset: typeof fetch = fetch): Promise<ArrayBuffer> {
  const url = new URL(`assets/${name}`, base);
  url.searchParams.set('sha256', expected.sha256);
  let cached: Response | undefined;
  try { cached = await cache?.match(url.href, { ignoreVary: true }); } catch { /* storage is optional */ }
  for (let attempt = 0; attempt < 2; attempt++) {
    const response = cached || await fetchAsset(url.href, { cache: attempt ? 'reload' : 'no-cache' });
    if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
    const buffer = await response.arrayBuffer();
    const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', buffer))]
      .map(b => b.toString(16).padStart(2, '0')).join('');
    if (hash === expected.sha256 && buffer.byteLength === expected.bytes) {
      try { await cache?.put(url.href, new Response(buffer.slice(0))); } catch { /* quota is not fatal */ }
      return buffer;
    }
    try { await cache?.delete(url.href); } catch { /* retry without persistent storage */ }
    cached = undefined;
  }
  throw new Error(`Asset integrity check failed for ${name} after refetching. The published files may be from different builds; retry after the update completes.`);
}
