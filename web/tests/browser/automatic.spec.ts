import {test,expect} from '@playwright/test';
test('automatic HUD discovery accepts a video inside an arbitrary window without viewport setup',async({page})=>{
  // This source repeats a still image. Keep frame wall time fixed so temporal
  // validation doesn't reject its unchanged clock after slow WASM inference.
  // Timers/performance remain real; this does not shorten the processing work.
  await page.clock.setFixedTime(new Date('2026-10-26T12:00:00Z'));
  await page.addInitScript(()=>{
    (window as unknown as {report:unknown}).report=null;
    window.addEventListener('coach:perception',e=>(window as unknown as {report:unknown}).report=(e as CustomEvent).detail);
    Object.defineProperty(navigator.mediaDevices,'getDisplayMedia',{value:async()=>{
      const image=new Image();image.src='/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg';await image.decode();
      const canvas=document.createElement('canvas');canvas.width=1920;canvas.height=1200;const ctx=canvas.getContext('2d')!;
      const draw=()=>{ctx.fillStyle='#191919';ctx.fillRect(0,0,1920,1200);ctx.drawImage(image,160,150,1600,900);};draw();setInterval(draw,200);return canvas.captureStream(5);
    }});
  });
  await page.goto('./#settings');
  await expect(page.locator('#crop-w')).toBeDisabled();
  await expect(page.locator('#auto-crop')).toBeDisabled();
  await page.locator('[data-page="coach"]').click();
  await page.getByRole('button',{name:'Start coaching'}).click();
  await expect(page.locator('#metric-souls')).toHaveText('1,732',{timeout:110000});
  await expect(page.locator('#metric-clock')).toHaveText('5:13');await expect(page.getByRole('dialog')).not.toBeVisible();
  const report=await page.evaluate(()=>(window as unknown as {report:{profile_id:string;fields:Record<string,{bounds_origin:string;bounds:{x:number;y:number}}>}}).report);
  expect(report.profile_id).toBe('deadlock-auto');expect(report.fields['player.currency'].bounds_origin).toBe('detected');
  expect(report.fields['player.currency'].bounds.x).toBeGreaterThan(200);expect(report.fields['world.minimap'].bounds.x).toBeGreaterThan(1380);
  expect(await page.locator('#minimap').evaluate((canvas:HTMLCanvasElement)=>{
    const pixels=canvas.getContext('2d')!.getImageData(0,0,canvas.width,canvas.height).data;let nonBackground=0;
    for(let i=0;i<pixels.length;i+=4)if(pixels[i]>50||pixels[i+1]>50||pixels[i+2]>50)nonBackground++;
    return nonBackground/(canvas.width*canvas.height);
  })).toBeGreaterThan(.05);
  await expect(page.locator('#quick-time')).toContainText('ms fast monitor',{timeout:30000});
  await page.getByRole('button',{name:'Adjust viewport',exact:true}).click();
  await expect(page.locator('#crop-region-status')).toContainText('located boxes');
  await expect(page.locator('#crop-history-status')).toContainText('Sampling paused');
  await page.getByRole('button',{name:'Close inspection and resume'}).click();
  await page.getByRole('button',{name:'Stop',exact:true}).click();
});
