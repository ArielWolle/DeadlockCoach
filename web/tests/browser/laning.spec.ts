import {test,expect,type Page} from '@playwright/test';

function snapshot(clock=400) {
  const lane=()=>({push:.55,ally_troopers:4,enemy_troopers:2,ally_progress:.6,enemy_progress:.5});
  return {game_clock_secs:clock,screen:'Gameplay',hp:800,max_hp:1000,health_observations:3,
    minimap:{observations:3,you:{x:.5,y:.55},your_lane:'Blue',your_progress:.45,green:lane(),blue:lane(),yellow:lane(),
      allies:[],enemies:[] as Array<{x:number;y:number}>,enemy_count:0,midboss_up:false}};
}
async function replay(page:Page,frames:unknown[]) {
  await page.goto('./#replay');
  await page.locator('#replay-files').setInputFiles({name:'lane-scenario.jsonl',mimeType:'application/x-ndjson',buffer:Buffer.from(frames.map(f=>JSON.stringify(f)).join('\n'))});
  await page.getByRole('button',{name:'Analyze selected files'}).click();
  await expect(page.locator('#replay-status')).toContainText('Replay complete.',{timeout:110000});
  await page.getByRole('link',{name:'Coach',exact:false}).first().click();
}

test('crashed wave produces sourced camp advice and the source guide can be opened',async({page})=>{
  const s=snapshot();s.minimap.blue.push=.8;s.minimap.blue.ally_progress=.8;s.minimap.blue.enemy_troopers=0;
  await replay(page,[s]);
  await expect(page.locator('#lane-action')).toHaveText('One-camp window');
  await expect(page.locator('#lane-call')).toContainText('if it is up');
  await page.getByText('Strategy sources',{exact:true}).click();
  await page.locator('#lane-sources button').first().click();
  await expect(page.locator('#knowledge-body')).toContainText('Push first, then camp');
});

test('healthy wave cover allows poke but falling health preempts it with the critical call',async({page})=>{
  const poke=snapshot(300);poke.minimap.enemies=[{x:.54,y:.54}];
  await replay(page,[poke]);await expect(page.locator('#lane-action')).toHaveText('Short poke window');
  const low={...poke,game_clock_secs:303,hp:200,health_delta_ratio:-.6};
  await replay(page,[poke,low]);
  await expect(page.locator('#lane-action')).toHaveText('Back off');
  // 20% and falling fast: the critical retreat, not the trade warning.
  await expect(page.locator('#current-advice')).toContainText('Health critical');
  await expect(page.locator('#player-readings')).toContainText('200/1000 HP');
});

test('lane focus and active coaching preference persist without affecting generic profiles',async({page})=>{
  await page.goto('./');await page.getByLabel('Lane focus',{exact:true}).selectOption('Green');
  await page.getByRole('link',{name:'Settings',exact:false}).click();
  await page.getByLabel('Active lane coaching',{exact:false}).uncheck();
  await page.getByRole('button',{name:'Save settings'}).click();await page.reload();
  await expect(page.getByLabel('Active lane coaching',{exact:false})).not.toBeChecked();
  await page.getByRole('link',{name:'Coach',exact:false}).first().click();
  await expect(page.getByLabel('Lane focus',{exact:true})).toHaveValue('Green');
  await expect(page.locator('#lane-call')).toContainText('disabled');
});
