use std::sync::OnceLock;
use serde::Deserialize;
#[derive(Deserialize)]struct Rationale{class_name:String,summary:String}
#[derive(Deserialize)]struct Item{class_name:String,cost:u32,#[serde(default)]slot:String,#[serde(default)]desc:String,#[serde(default)]effect:String}
pub fn summary(class:&str)->Option<&'static str>{
    static DATA:OnceLock<Vec<Rationale>>=OnceLock::new();
    DATA.get_or_init(||serde_json::from_str(include_str!("../../../data/coaching/item-rationales.json")).expect("shipped item rationale"))
        .iter().find(|r|r.class_name==class).map(|r|r.summary.as_str())
}
fn items()->&'static [Item]{
    static ITEMS:OnceLock<Vec<Item>>=OnceLock::new();
    ITEMS.get_or_init(||serde_json::from_str(include_str!("../../../data/items.json").trim_start_matches('\u{feff}')).expect("shipped item costs"))
}
pub fn cost(class:&str)->Option<u32>{items().iter().find(|i|i.class_name==class).map(|i|i.cost)}
/// Shop category: "weapon", "spirit" or "vitality".
pub fn slot(class:&str)->Option<&'static str>{items().iter().find(|i|i.class_name==class).map(|i|i.slot.as_str())}
/// What the item does, from the game's own tooltip: the first sentences of its description plus
/// its headline values ("Deal additional Weapon Damage when beyond a minimum distance from your
/// target: +40% Weapon Damage beyond Range").
pub fn tooltip(class:&str)->Option<String>{
    let i=items().iter().find(|i|i.class_name==class)?;
    let mut d=String::new();
    for s in i.desc.split_inclusive(". ").take(2){if d.len()+s.len()>170&&!d.is_empty(){break;}d.push_str(s);}
    let d=d.trim().trim_end_matches('.').to_string();
    match (d.is_empty(),i.effect.is_empty()){
        (true,true)=>None,(true,false)=>Some(format!("It gives {}",i.effect)),
        (false,true)=>Some(d),(false,false)=>Some(format!("{d}: {}",i.effect)),
    }
}
/// The evidence behind a suggestion. counters.json lifts are observed win-rate differences, not
/// a mechanism, and are said as such. (No longer spoken: the live tip is <= 15 words.)
#[allow(dead_code)]
pub fn statistical_reason(reason:&str,who:&str)->String{
    if reason.starts_with("Wins ") {if let Some(lift)=reason.split_whitespace().nth(1).filter(|v|v.parse::<f32>().is_ok()){
        return format!("In recorded matches, players who bought it won {lift} percentage points more often against {who}.");
    }}reason.trim().to_string()
}
