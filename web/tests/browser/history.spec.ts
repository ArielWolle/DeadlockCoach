import {test,expect} from '@playwright/test';
test('inspector freezes acquisition, selects the last five frames and exports portable boxes',async({page})=>{
  await page.addInitScript(()=>{
    localStorage.setItem('coach-perception-profile-v1',JSON.stringify({schema_version:1,id:'history-hud',name:'History HUD',adapter:'generic',viewport:{aspect_ratio:null,max_width:1600,auto_crop:false},
      nodes:[{id:'area',op:{type:'crop',region:{x:.1,y:.1,w:.4,h:.3}}},{id:'color',inputs:['area'],op:{type:'color_ratio',rgb:[0,200,0],tolerance:255}}],outputs:{value:'color'}}));
    const state=window as unknown as {reports:number;producer:number;playback:HTMLMediaElement;shares:number};state.reports=0;state.producer=0;state.shares=0;
    window.addEventListener('coach:perception',()=>state.reports++);
    const play=HTMLMediaElement.prototype.play;HTMLMediaElement.prototype.play=function(this:HTMLMediaElement){if(this.srcObject)state.playback=this;return play.call(this);};
    Object.defineProperty(navigator.mediaDevices,'getDisplayMedia',{value:async()=>{
      state.shares++;const canvas=document.createElement('canvas');canvas.width=800;canvas.height=600;const ctx=canvas.getContext('2d')!;
      const draw=()=>{state.producer++;ctx.fillStyle=`rgb(0,${100+state.producer%100},0)`;ctx.fillRect(0,0,800,600);};draw();setInterval(draw,100);return canvas.captureStream(5);
    }});
  });
  await page.goto('./');await page.getByRole('button',{name:'Start coaching'}).click();
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {reports:number}).reports),{timeout:40000}).toBeGreaterThanOrEqual(6);
  await page.getByRole('button',{name:'Adjust viewport',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.locator('#crop-history button')).toHaveCount(5);
  const counts=await page.evaluate(()=>{const s=window as unknown as {reports:number;producer:number;playback:HTMLMediaElement};return{reports:s.reports,producer:s.producer,paused:s.playback.paused};});expect(counts.paused).toBe(true);
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {producer:number}).producer)).toBeGreaterThan(counts.producer+3);
  expect(await page.evaluate(()=>(window as unknown as {reports:number}).reports)).toBe(counts.reports);
  const latest=Number(await page.locator('#crop-source').getAttribute('data-source-frame-id'));
  await page.locator('#crop-history button').first().click();
  expect(Number(await page.locator('#crop-source').getAttribute('data-source-frame-id'))).toBeLessThan(latest);
  await expect(page.locator('#crop-region-status')).toContainText('configured search');
  const downloading=page.waitForEvent('download');await page.getByRole('button',{name:'Export inspection sample'}).click();const stream=await(await downloading).createReadStream();const chunks:Buffer[]=[];for await(const c of stream!)chunks.push(Buffer.from(c));
  const sample=JSON.parse(Buffer.concat(chunks).toString());expect(sample.annotation_state).toBe('unreviewed');expect(sample.image_base64).toBeTruthy();expect(sample.regions[0].origin).toBe('configured');
  await page.getByRole('button',{name:'Cancel',exact:true}).click();await expect(page.getByRole('dialog')).not.toBeVisible();
  expect(await page.evaluate(()=>(window as unknown as {playback:HTMLMediaElement}).playback.paused)).toBe(false);
  expect(await page.evaluate(()=>(window as unknown as {shares:number}).shares)).toBe(1);await page.getByRole('button',{name:'Stop',exact:true}).click();
});
