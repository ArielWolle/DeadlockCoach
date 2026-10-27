use std::sync::OnceLock;
use serde::Deserialize;
#[derive(Deserialize)]struct Rationale{class_name:String,summary:String}
#[derive(Deserialize)]struct Item{class_name:String,cost:u32,#[serde(default)]slot:String}
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
pub fn statistical_reason(reason:&str)->String{
    if reason.starts_with("Wins ") {if let Some(lift)=reason.split_whitespace().nth(1).filter(|v|v.parse::<f32>().is_ok()){
        return format!("Recorded win rate is {lift} percentage points higher in this matchup sample.");
    }}reason.trim().to_string()
}
