import {test,expect} from '@playwright/test';

test('real worker recovers stale shell timings and corrupt versioned cache without a reload loop',async({page})=>{
  await page.goto('./#replay');
  await page.evaluate(async()=>{
    await navigator.serviceWorker.ready;
  });
  await expect.poll(()=>page.evaluate(()=>!!navigator.serviceWorker.controller)).toBe(true);
  const expected=await page.evaluate(async()=>{
    const base=new URL('./',location.href);
    const manifest=await(await fetch(new URL('assets/manifest.json',base),{cache:'no-cache'})).json();
    const url=new URL('assets/timings.json',base);
    for(const name of await caches.keys())if(name.startsWith('deadlock-shell-')){
      await(await caches.open(name)).put(url.href,new Response('stale shell timings'));
    }
    url.searchParams.set('sha256',manifest.files['timings.json'].sha256);
    await(await caches.open(`deadlock-assets-${manifest.version}`)).put(url.href,new Response('corrupt verified-cache entry'));
    return {url:url.href,version:manifest.version,hash:manifest.files['timings.json'].sha256};
  });
  await page.getByRole('button',{name:'Load included match'}).click();
  await expect(page.locator('#replay-status')).toContainText('Included match replay complete',{timeout:110000});
  await expect(page.locator('#engine-state')).toHaveText('LOCAL ENGINE READY');
  const actual=await page.evaluate(async({url,version})=>{
    const response=await(await caches.open(`deadlock-assets-${version}`)).match(url);
    return [...new Uint8Array(await crypto.subtle.digest('SHA-256',await response!.arrayBuffer()))].map(b=>b.toString(16).padStart(2,'0')).join('');
  },expected);
  expect(actual).toBe(expected.hash);
});
