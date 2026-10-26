import {test,expect} from '@playwright/test';
test('real replay gives one explained Rem item and spaces other enemy suggestions',async({page})=>{
  await page.addInitScript(()=>localStorage.setItem('deadlock-coach-settings-v1',JSON.stringify({options:{timers:false,economy:false,map:false,counters:true,deaths:false,laning:false}})));
  await page.goto('./#replay');
  const frames=[600,610,780].map(game_clock_secs=>({game_clock_secs,screen:'Gameplay',souls:3500,hp:900,max_hp:1000,enemy_heroes:['hero_familiar','hero_inferno']}));
  await page.locator('#replay-files').setInputFiles({name:'counter-scenario.jsonl',mimeType:'application/x-ndjson',buffer:Buffer.from(frames.map(f=>JSON.stringify(f)).join('\n'))});
  await page.getByRole('button',{name:'Analyze selected files'}).click();
  await expect(page.locator('#replay-status')).toContainText('Replay complete.',{timeout:110000});
  await page.locator('[data-page="coach"]').click();
  await expect(page.locator('#cue-list li')).toHaveCount(2);
  await expect(page.locator('#cue-list')).toContainText('Healing Nova. It heals you and nearby allies');
  await expect(page.locator('#cue-list')).not.toContainText('Good items against them');
});
