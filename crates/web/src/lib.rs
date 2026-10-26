//! Browser adapter. All recognition and coaching executes locally in a worker.
use coach_core::GameSnapshot;
use coach_engine::{Engine, Rule, RulesConfig};
use coach_vision::{ocr::Reader, scan::Scanner, ColorLibrary};
use coach_perception::{Pipeline, Resources, Profile, Packet, FrameContext, Stage, Report, profile_from_json};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Clone, Deserialize)]
#[serde(default)]
struct Options { timers: bool, economy: bool, map: bool, counters: bool, deaths: bool, laning:bool, focus_lane:Option<coach_core::Lane> }
impl Default for Options {
    fn default() -> Self { Self { timers: true, economy: true, map: true, counters: true, deaths: true, laning:true, focus_lane:None } }
}

#[derive(Serialize)]
struct Analysis { snapshot: GameSnapshot, cues: Vec<coach_engine::Cue>, perception: Option<Report>, tactics:Option<coach_engine::laning::Decision> }

struct DeadlockStage { scanner: Scanner }
struct AutomaticStage {scanner:coach_vision::auto::AutoScanner}
impl Stage for AutomaticStage {
    fn run(&mut self,_inputs:&[Packet],frame:&FrameContext<'_>,_:&serde_json::Value)->Result<Packet,String>{
        let value=self.scanner.scan(frame.frame,frame.timestamp_ms);
        Ok(Packet{value:Some(serde_json::to_value(value).map_err(|e|e.to_string())?),regions:self.scanner.regions(),..Default::default()})
    }
    fn reset(&mut self){self.scanner.reset();}
}
impl Stage for DeadlockStage {
    fn run(&mut self, _inputs: &[Packet], frame: &FrameContext<'_>, _config: &serde_json::Value) -> Result<Packet, String> {
        let snapshot = self.scanner.scan_image(frame.frame, frame.auxiliary, frame.timestamp_ms);
        let mut regions=vec![];
        regions.extend(self.scanner.regions());
        for (id,label,r) in [("clock_search","Clock search",coach_vision::hud::CLOCK),("souls_search","Souls search",coach_vision::hud::SOULS),
            ("map_search","Minimap search",coach_vision::Region{x:coach_vision::minimap::FRAC_X,y:coach_vision::minimap::FRAC_Y,w:coach_vision::minimap::FRAC_W,h:coach_vision::minimap::FRAC_H})]{
            regions.push(coach_perception::RegionObservation{id:id.into(),label:label.into(),origin:coach_perception::RegionOrigin::Configured,score:None,value_path:None,
                bounds:coach_perception::Bounds{x:(r.x*frame.frame.width() as f32) as u32,y:(r.y*frame.frame.height() as f32) as u32,w:(r.w*frame.frame.width() as f32) as u32,h:(r.h*frame.frame.height() as f32) as u32}});
        }
        Ok(Packet { value: Some(serde_json::to_value(snapshot).map_err(|e| e.to_string())?),regions, ..Default::default() })
    }
    fn reset(&mut self) { self.scanner.reset(); }
}

type Images = Vec<(String, Vec<u8>)>;

/// Explicit binary asset pack: DCAS1 | count:u32LE | [name_len:u16LE | png_len:u32LE | name | PNG].
fn unpack(bytes: &[u8]) -> Result<[Images; 3], String> {
    if bytes.get(..5) != Some(b"DCAS1") { return Err("invalid asset pack magic".into()); }
    let mut pos = 5usize;
    fn take<'a>(bytes: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8], String> {
        let end = pos.checked_add(n).ok_or("asset pack overflow")?;
        let slice = bytes.get(*pos..end).ok_or("truncated asset pack")?; *pos = end; Ok(slice)
    }
    let count = u32::from_le_bytes(take(bytes, &mut pos, 4)?.try_into().unwrap());
    if count > 2000 { return Err("asset pack too large".into()); }
    let mut sets: [Images; 3] = Default::default();
    for _ in 0..count {
        let name_len = u16::from_le_bytes(take(bytes, &mut pos, 2)?.try_into().unwrap()) as usize;
        let image_len = u32::from_le_bytes(take(bytes, &mut pos, 4)?.try_into().unwrap()) as usize;
        let name = std::str::from_utf8(take(bytes, &mut pos, name_len)?).map_err(|e| e.to_string())?;
        let (kind, class) = name.split_once('/').ok_or("asset has no kind")?;
        let index = match kind { "shop" => 0, "h_card" => 1, "h_crit" => 2, _ => return Err("unknown asset kind".into()) };
        let png = take(bytes, &mut pos, image_len)?.to_vec();
        sets[index].push((class.to_owned(), png));
    }
    if pos != bytes.len() { return Err("trailing asset pack data".into()); }
    Ok(sets)
}

#[wasm_bindgen]
pub struct Coach {
    pipeline: Pipeline,
    engine: Engine,
    timings: String,
    counters: String,
    rules: RulesConfig,
    options: Options,
    match_id: Option<u64>,
    last_clock: Option<u32>,
    lane_coach:coach_engine::laning::LaneCoach,
}

fn js_error(s: impl ToString) -> JsValue { JsValue::from_str(&s.to_string()) }
fn to_js(value: &impl Serialize) -> Result<JsValue, JsValue> {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(js_error)
}
fn json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, JsValue> {
    serde_json::from_str(s.trim_start_matches('\u{feff}')).map_err(js_error)
}

impl Coach {
    fn engine(&self) -> Result<Engine, JsValue> {
        if self.pipeline.profile().adapter != "deadlock" { return Ok(Engine::new(vec![])); }
        let mut rules: Vec<Box<dyn Rule>> = vec![];
        if self.options.timers { rules.extend(coach_engine::rules_from_timings(&self.timings).map_err(js_error)?); }
        if self.options.laning {rules.push(Box::new(self.lane_coach.clone()));}
        if self.options.map { rules.push(Box::new(coach_engine::map_rules::MapRule)); }
        if self.options.economy {
            rules.push(Box::new(coach_engine::UnspentSoulsRule(self.rules.clone())));
            rules.push(Box::new(coach_engine::NetworthLeadRule::new(self.rules.clone())));
        }
        if self.options.deaths { rules.push(Box::new(coach_engine::DeathCauseRule)); }
        if self.options.counters { rules.push(coach_engine::counter_rule_from_json(&self.counters).map_err(js_error)?); }
        Ok(Engine::new(rules))
    }
    fn step(&mut self, snap: &GameSnapshot) -> Result<Vec<coach_engine::Cue>, JsValue> {
        let new_match = matches!((self.match_id, snap.match_id), (Some(a), Some(b)) if a != b);
        let reset_clock = matches!((self.last_clock, snap.game_clock_secs), (Some(a), Some(b)) if a > 120 && b < 60);
        if new_match || reset_clock { self.engine = self.engine()?; }
        if snap.match_id.is_some() { self.match_id = snap.match_id; }
        if snap.game_clock_secs.is_some() { self.last_clock = snap.game_clock_secs; }
        Ok(self.engine.step(snap))
    }
}

#[wasm_bindgen]
impl Coach {
    #[wasm_bindgen(constructor)]
    pub fn new(detection: Vec<u8>, recognition: Vec<u8>, assets: Vec<u8>, heroes_json: &str,
        timings: &str, counters: &str, rules: &str) -> Result<Coach, JsValue> {
        Self::create(include_str!("../../../data/profiles/deadlock.json"), detection, recognition, assets, heroes_json, timings, counters, rules)
    }

    /// Generic profiles can omit models and all Deadlock assets when their stages do not need them.
    pub fn create(profile_json: &str, detection: Vec<u8>, recognition: Vec<u8>, assets: Vec<u8>, heroes_json: &str,
        timings: &str, counters: &str, rules: &str) -> Result<Coach, JsValue> {
        console_error_panic_hook::set_once();
        let profile = profile_from_json(profile_json).map_err(js_error)?;
        check_adapter(&profile)?;
        let mut resources = Resources::default();
        let reader = if !detection.is_empty() && !recognition.is_empty() { Some(Reader::from_bytes(detection, recognition).map_err(js_error)?) } else { None };
        if let Some(reader) = &reader { resources.ocr = Some(std::sync::Arc::new(reader.clone())); }
        if !assets.is_empty() {
            let [items, heroes, critical] = unpack(&assets).map_err(js_error)?;
            let items = std::sync::Arc::new(ColorLibrary::from_images(items, true, false).map_err(js_error)?);
            let heroes = std::sync::Arc::new(ColorLibrary::from_images(heroes, false, true).map_err(js_error)?);
            let critical = std::sync::Arc::new(ColorLibrary::from_images(critical, false, true).map_err(js_error)?);
            resources.libraries.insert("deadlock.items".into(), items.clone());
            resources.libraries.insert("deadlock.heroes".into(), heroes.clone());
            resources.libraries.insert("deadlock.critical_heroes".into(), critical.clone());
            #[derive(Deserialize)] struct Hero { class_name: String, name: String }
            let names: Vec<Hero> = json(heroes_json)?;
            if let Some(reader) = reader {
                let names:Vec<_>=names.into_iter().map(|h|(h.class_name,h.name)).collect();
                let automatic=Scanner::from_shared(reader.clone(),items.clone(),heroes.clone(),critical.clone(),names.clone());
                resources.stages.insert("deadlock.auto".into(),Box::new(AutomaticStage{scanner:coach_vision::auto::AutoScanner::new(automatic,reader.clone())}));
                let scanner = Scanner::from_shared(reader, items, heroes, critical, names);
                resources.stages.insert("deadlock.scan".into(), Box::new(DeadlockStage { scanner }));
            }
        }
        let pipeline = Pipeline::new(profile, resources).map_err(js_error)?;
        let mut coach = Coach { pipeline, engine: Engine::new(vec![]), timings: timings.into(),
            counters: counters.trim_start_matches('\u{feff}').into(), rules: if rules.is_empty() { RulesConfig::default() } else { json(rules)? },
            options: Options::default(), match_id: None, last_clock: None, lane_coach:coach_engine::laning::LaneCoach::default() };
        coach.engine = coach.engine()?;
        Ok(coach)
    }

    pub fn configure(&mut self, options_json: &str) -> Result<(), JsValue> {
        self.options = json(options_json)?;
        self.engine = self.engine()?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), JsValue> {
        self.pipeline.reset(); self.match_id = None; self.last_clock = None;
        self.engine = self.engine()?; Ok(())
    }

    /// RGBA transfer avoids full-frame JPEG encoding, preserving HUD edges.
    pub fn analyze(&mut self, rgba: Vec<u8>, width: u32, height: u32, map: Vec<u8>,
        map_width: u32, map_height: u32, now_ms: f64) -> Result<JsValue, JsValue> {
        if width == 0 || height == 0 || width > 4096 || height > 2304 || !now_ms.is_finite() || now_ms < 0.0 {
            return Err(js_error("invalid frame dimensions or timestamp"));
        }
        let image = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| js_error("invalid RGBA buffer length"))?;
        let image = image::DynamicImage::ImageRgba8(image);
        let map = if map_width >= 100 && map_width <= 1024 && map_height >= 100 && map_height <= 1024
            && (map_width as f32 / map_height as f32 - 335.0 / 350.0).abs() < 0.05 {
            image::RgbaImage::from_raw(map_width, map_height, map).map(|m| image::DynamicImage::ImageRgba8(m).to_rgb8())
        } else { None };
        let perception = self.pipeline.run(&image, map.as_ref(), now_ms as u64);
        let mut snapshot:GameSnapshot = if self.pipeline.profile().adapter == "deadlock" {
            let id = self.pipeline.profile().nodes.iter().find(|n| matches!(&n.op, coach_perception::Operation::Custom { name, .. } if name == "deadlock.scan"||name=="deadlock.auto")).map(|n| n.id.as_str());
            perception.stages.iter().find(|s| Some(s.id.as_str()) == id).and_then(|s| s.value.clone())
                .and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
        } else { GameSnapshot::default() };
        snapshot.coaching_context.focus_lane=self.options.focus_lane;
        let tactics=if self.options.laning && self.pipeline.profile().adapter=="deadlock" {self.lane_coach.decide(&snapshot)} else {None};
        let cues = self.step(&snapshot)?;
        to_js(&Analysis { snapshot, cues, perception: Some(perception), tactics })
    }

    /// Replay imports traverse the very same rule engine as live frames.
    pub fn replay(&mut self, snapshot_json: &str) -> Result<JsValue, JsValue> {
        let mut snapshot: GameSnapshot = json(snapshot_json)?;
        snapshot.coaching_context.focus_lane=self.options.focus_lane.or(snapshot.coaching_context.focus_lane);
        let tactics=if self.options.laning && self.pipeline.profile().adapter=="deadlock" {self.lane_coach.decide(&snapshot)} else {None};
        let cues = self.step(&snapshot)?;
        to_js(&Analysis { snapshot, cues, perception: None, tactics })
    }
}

fn check_adapter(profile: &Profile) -> Result<(), JsValue> {
    if profile.adapter != "generic" && profile.adapter != "deadlock" { return Err(js_error("unknown browser game adapter")); }
    if profile.adapter == "deadlock" && profile.nodes.iter().filter(|n| matches!(&n.op, coach_perception::Operation::Custom { name, .. } if name == "deadlock.scan"||name=="deadlock.auto")).count() != 1 {
        return Err(js_error("Deadlock coaching requires exactly one deadlock.scan or deadlock.auto stage"));
    }
    if profile.adapter == "deadlock" && profile.nodes.iter().any(|n|matches!(&n.op,coach_perception::Operation::Custom{name,..}if name=="deadlock.scan")) && (profile.viewport.aspect_ratio.is_none_or(|r| (r - 16.0 / 9.0).abs() > 0.01) || profile.viewport.max_width != 1600) {
        return Err(js_error("the legacy Deadlock adapter requires its calibrated 16:9 / 1600-wide input; use a generic profile for other layouts"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_pack_checks_truncation_and_kind() {
        assert!(unpack(b"bogus").is_err());
        assert!(unpack(b"DCAS1\x01\x00\x00\x00").is_err());
        assert!(unpack(b"DCAS1\x00\x00\x00\x00extra").is_err());
        assert!(unpack(b"DCAS1\x00\x00\x00\x00").is_ok());
    }
}
