use coach_core::{GameSnapshot,HeroStatus};
use coach_engine::{Rule,economy::PaceRule};
fn state(own:u32)->GameSnapshot{
    let mut s=GameSnapshot{screen:Some("Gameplay".into()),game_clock_secs:Some(360),hero:Some("me".into()),own_networth:Some(own),..Default::default()};
    s.hero_status=(0..12).map(|i|HeroStatus{ally:i<6,slot:i%6,hero:Some(if i==0{"me".into()}else{format!("hero{i}")}),networth:Some(if i==0{own}else{5000}),networth_observations:2,..Default::default()}).collect();s
}
#[test]fn personal_pace_uses_earned_souls_and_rewards_on_track_farm(){
    assert!(PaceRule.evaluate(&state(3000))[0].text.contains("behind"));
    assert!(PaceRule.evaluate(&state(5000))[0].text.contains("Good farming"));
    assert!(PaceRule.evaluate(&state(6500))[0].text.contains("ahead"));
    let mut s=state(3000);s.own_networth=None;s.souls=Some(6000);assert!(PaceRule.evaluate(&s).is_empty());
    s=state(5000);s.hero_status[0].networth_observations=1;assert!(PaceRule.evaluate(&s).is_empty());
    s=state(5000);s.hero=None;assert!(PaceRule.evaluate(&s).is_empty());
}
