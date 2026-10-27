import {test,expect} from '@playwright/test';
test('one-second acquisition and the real quick worker continue during full OCR and pause for inspection',async({page},testInfo)=>{
  await page.clock.setFixedTime(new Date('2026-10-26T12:00:00Z'));
  await page.addInitScript(()=>{
    const state=window as unknown as {captures:number;full:number;quick:Array<{elapsed:number;snapshot:{hp:number;hero_status:Array<{critical:boolean}>}}>};state.captures=0;state.full=0;state.quick=[];
    window.addEventListener('coach:capture',()=>state.captures++);
    window.addEventListener('coach:perception',()=>state.full++);
    window.addEventListener('coach:quick',e=>state.quick.push((e as CustomEvent).detail));
    Object.defineProperty(navigator.mediaDevices,'getDisplayMedia',{value:async()=>{
      const image=new Image();image.src='/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg';await image.decode();
      const canvas=document.createElement('canvas');canvas.width=1600;canvas.height=900;const ctx=canvas.getContext('2d')!;
      const draw=()=>ctx.drawImage(image,0,0);draw();setInterval(draw,100);return canvas.captureStream(10);
    }});
  });
  await page.goto('./');await page.getByRole('button',{name:'Start coaching'}).click();
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {quick:unknown[]}).quick.length),{timeout:110000}).toBeGreaterThanOrEqual(2);
  const state=await page.evaluate(()=>{const s=window as unknown as {captures:number;full:number;quick:Array<{elapsed:number;snapshot:{hp:number;hero_status:Array<{critical:boolean}>}}>};return{captures:s.captures,full:s.full,quick:s.quick.at(-1)!};});
  expect(state.quick.snapshot.hp).toBe(779);expect(state.quick.snapshot.hero_status).toHaveLength(12);// Banners are seeded from the full scan's round-robin: unobserved is unknown, never safe.
  expect([true,null]).toContain(state.quick.snapshot.hero_status[6].critical ?? null);
  expect(state.captures).toBeGreaterThan(state.full+4);
  await expect(page.locator('#fast-analysis-time')).toContainText('Fast HP/map:');
  await expect(page.locator('#analysis-time')).toContainText('Full scene:');
  console.info(`Quick monitor: ${Math.round(state.quick.elapsed)} ms; ${state.captures} captures / ${state.full} full scans`);
  await testInfo.attach('quick-monitor-timing',{body:JSON.stringify(state),contentType:'application/json'});
  await page.getByRole('button',{name:'Adjust viewport',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();
  const count=await page.evaluate(()=>(window as unknown as {captures:number}).captures);
  await page.waitForTimeout(2200);expect(await page.evaluate(()=>(window as unknown as {captures:number}).captures)).toBe(count);
  await expect(page.locator('#crop-history button')).toHaveCount(5);await expect(page.locator('#crop-region-status')).toContainText('configured search');
  await page.getByRole('button',{name:'Close inspection and resume'}).click();
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {captures:number}).captures)).toBeGreaterThan(count);
  await page.getByRole('button',{name:'Stop',exact:true}).click();
});
