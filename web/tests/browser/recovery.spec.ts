import {test,expect} from '@playwright/test';
test('a hung full scan is rebuilt automatically while screenshots, fast monitoring and the share continue',async({page})=>{
  await page.clock.setFixedTime(new Date('2026-10-26T12:00:00Z'));
  await page.addInitScript(()=>{
    const state=window as unknown as {shares:number;captures:number;quick:number;recoveries:number;dropped:boolean;fullFrames:number};Object.assign(state,{shares:0,captures:0,quick:0,recoveries:0,dropped:false,fullFrames:0});
    window.addEventListener('coach:capture',()=>state.captures++);window.addEventListener('coach:quick',()=>state.quick++);window.addEventListener('coach:recovery',()=>state.recoveries++);
    const NativeWorker=window.Worker;
    window.Worker=class extends NativeWorker{
      constructor(url:string|URL,options?:WorkerOptions){super(url,options);
        if(String(url).includes('coach.worker')){
          const post=this.postMessage.bind(this);
          this.postMessage=((message:{type?:string},transfer:Transferable[])=>{
            if(message.type==='frame'&&++state.fullFrames===3&&!state.dropped){state.dropped=true;return;}
            post(message,transfer);
          }) as Worker['postMessage'];
        }
      }
    };
    Object.defineProperty(navigator.mediaDevices,'getDisplayMedia',{value:async()=>{
      state.shares++;const image=new Image();image.src='/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg';await image.decode();
      const canvas=document.createElement('canvas');canvas.width=1600;canvas.height=900;const ctx=canvas.getContext('2d')!;const draw=()=>ctx.drawImage(image,0,0);draw();setInterval(draw,100);return canvas.captureStream(10);
    }});
  });
  await page.goto('./');await page.getByRole('button',{name:'Start coaching'}).click();
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {quick:number}).quick),{timeout:90000}).toBeGreaterThan(1);
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {recoveries:number}).recoveries),{timeout:50000}).toBe(1);
  const before=await page.evaluate(()=>{const s=window as unknown as {captures:number;quick:number};return {captures:s.captures,quick:s.quick};});
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {captures:number}).captures)).toBeGreaterThan(before.captures);
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {quick:number}).quick)).toBeGreaterThan(before.quick);
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {fullFrames:number}).fullFrames),{timeout:30000}).toBeGreaterThan(3);
  expect(await page.evaluate(()=>(window as unknown as {shares:number}).shares)).toBe(1);
  await expect(page.getByRole('button',{name:'Pause',exact:true})).toBeEnabled();
  await page.getByRole('button',{name:'Stop',exact:true}).click();
});
