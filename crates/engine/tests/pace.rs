use coach_core::{GameSnapshot,HeroStatus};
use coach_engine::{Rule,economy::PaceRule};
fn state(own:u32)->GameSnapshot{
    let mut s=GameSnapshot{screen:Some("Gameplay".into()),game_clock_secs:Some(360),hero:Some("me".into()),own_networth:Some(own),..Default::default()};
    s.hero_status=(0..12).map(|i|HeroStatus{ally:i<6,slot:i%6,hero:Some(if i==0{"me".into()}else{format!("hero{i}")}),networth:Some(if i==0{own}else{5000}),networth_observations:2,..Default::default()}).collect();s
}
fn band(s:&GameSnapshot)->Option<String>{PaceRule::default().assess(s).map(|(b,_)|b.to_string())}
#[test]fn personal_pace_uses_earned_souls_and_rewards_on_track_farm(){
    assert!(PaceRule::default().assess(&state(3000)).unwrap().1.text.contains("behind"));
    assert!(PaceRule::default().assess(&state(5000)).unwrap().1.text.contains("Good farming"));
    assert!(PaceRule::default().assess(&state(6500)).unwrap().1.text.contains("ahead"));
    let mut s=state(3000);s.own_networth=None;s.souls=Some(6000);assert!(band(&s).is_none());
    s=state(5000);s.hero_status[0].networth_observations=1;assert!(band(&s).is_none());
    s=state(5000);s.hero=None;assert!(band(&s).is_none());
}
#[test]fn a_small_gap_or_a_digit_misread_is_not_a_band(){
    // 20% under a 2,400 lobby at 4:00 is ~500 souls: reading noise, no call (rec12 4:00).
    let mut s=state(1900);for p in s.hero_status.iter_mut().skip(1){p.networth=Some(2400);}s.game_clock_secs=Some(240);
    assert!(band(&s).is_none());
    // rec12 2:55-3:13 read 8,000 for a 2,400 slot: not your pace, and not part of the lobby.
    let mut s=state(8000);for p in s.hero_status.iter_mut().skip(1){p.networth=Some(2400);}
    assert!(band(&s).is_none());
    let mut s=state(2400);for p in s.hero_status.iter_mut().skip(1){p.networth=Some(2400);}s.hero_status[3].networth=Some(8000);
    assert_eq!(band(&s).as_deref(),Some("on-track"));
}
#[test]fn behind_never_contradicts_a_team_that_is_ahead(){
    // Your allies carry the lead; you are level with the enemy heroes: no "behind".
    let mut s=state(3000);for p in s.hero_status.iter_mut().skip(1){p.networth=Some(if p.ally||p.slot>=4{6000}else{3200});}
    assert_eq!(PaceRule::default().assess(&{let mut x=s.clone();x.ally_networth=None;x}).map(|b|b.0),Some("behind"),"without team totals");
    s.ally_networth=Some(33000);s.enemy_networth=Some(19000);
    assert!(band(&s).is_none());
    // Behind the enemy heroes too: still said.
    for p in s.hero_status.iter_mut().filter(|p|!p.ally){p.networth=Some(5000);}
    s.ally_networth=Some(33000);s.enemy_networth=Some(30000);
    assert_eq!(band(&s).as_deref(),Some("behind"));
}
#[test]fn a_band_is_spoken_once_it_held_for_fifteen_seconds(){
    let r=PaceRule::default();let at=|t:u32,w:u32|{let mut s=state(w);s.game_clock_secs=Some(t);s};
    // rec12 said "ahead" at 3:32 and "on track" at 3:40: one-read bands are not spoken.
    assert!(r.evaluate(&at(600,6500)).is_empty());assert!(r.evaluate(&at(608,5000)).is_empty());
    assert!(r.evaluate(&at(612,5000)).is_empty());assert!(r.evaluate(&at(618,5000)).is_empty(),"3 reads but only 10 s");
    assert_eq!(r.evaluate(&at(624,5000)).len(),1);
}
#[test]fn the_same_pace_band_is_not_repeated_and_only_behind_is_reminded(){
    let r=PaceRule::default();let at=|t:u32,w:u32|{let mut s=state(w);s.game_clock_secs=Some(t);s};
    let said=|from:u32,to:u32,w:u32|(from..=to).step_by(5).map(|t|r.evaluate(&at(t,w)).len()).sum::<usize>();
    assert_eq!(said(600,700,5000),1);assert_eq!(said(705,1000,5000),0,"\"Good farming\" once, not every 5 minutes (rec6: 16 times)");
    // Behind: earned souls only grow, so "behind" here is the lobby pulling away (others 9,000).
    let late=|t:u32|{let mut s=at(t,5600);for p in s.hero_status.iter_mut().skip(1){p.networth=Some(9000);}s};
    let n:usize=(1010..=1040).step_by(5).map(|t|r.evaluate(&late(t)).len()).sum();assert_eq!(n,1,"a change of band is news");
    let n:usize=(1045..=1300).step_by(5).map(|t|r.evaluate(&late(t)).len()).sum();assert_eq!(n,0);
    let n:usize=(1305..=1400).step_by(5).map(|t|r.evaluate(&late(t)).len()).sum();assert_eq!(n,1,"reminder after 5 minutes");
}
#[test]fn a_drop_in_your_own_earned_souls_is_a_misread(){
    // rec11 read 7,100 from 14:42 after 11,000 at 14:39 and said "behind".
    let r=PaceRule::default();let at=|t:u32,w:u32|{let mut s=state(w);s.game_clock_secs=Some(t);for p in s.hero_status.iter_mut().skip(1){p.networth=Some(11000);}s};
    let n:usize=(860..=880).step_by(5).map(|t|r.evaluate(&at(t,11000)).len()).sum();assert_eq!(n,1);
    let n:usize=(882..=960).step_by(3).map(|t|r.evaluate(&at(t,7100)).len()).sum();assert_eq!(n,0);
}
