//! Browser adapter. All recognition and coaching executes locally in a worker.
use coach_core::GameSnapshot;
use coach_engine::{Engine, Rule, RulesConfig};
use coach_vision::{ocr::Reader, scan::Scanner, ColorLibrary};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Clone, Deserialize)]
#[serde(default)]
struct Options { timers: bool, economy: bool, map: bool, counters: bool, deaths: bool }
impl Default for Options {
    fn default() -> Self { Self { timers: true, economy: true, map: true, counters: true, deaths: true } }
}

#[derive(Serialize)]
struct Analysis { snapshot: GameSnapshot, cues: Vec<coach_engine::Cue> }

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
    scanner: Scanner,
    engine: Engine,
    timings: String,
    counters: String,
    rules: RulesConfig,
    options: Options,
    match_id: Option<u64>,
    last_clock: Option<u32>,
}

fn js_error(s: impl ToString) -> JsValue { JsValue::from_str(&s.to_string()) }
fn json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, JsValue> {
    serde_json::from_str(s.trim_start_matches('\u{feff}')).map_err(js_error)
}

impl Coach {
    fn engine(&self) -> Result<Engine, JsValue> {
        let mut rules: Vec<Box<dyn Rule>> = vec![];
        if self.options.timers { rules.extend(coach_engine::rules_from_timings(&self.timings).map_err(js_error)?); }
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
        console_error_panic_hook::set_once();
        let [items, heroes, critical] = unpack(&assets).map_err(js_error)?;
        #[derive(Deserialize)] struct Hero { class_name: String, name: String }
        let names: Vec<Hero> = json(heroes_json)?;
        let scanner = Scanner::from_resources(
            Reader::from_bytes(detection, recognition).map_err(js_error)?,
            ColorLibrary::from_images(items, true, false).map_err(js_error)?,
            ColorLibrary::from_images(heroes, false, true).map_err(js_error)?,
            ColorLibrary::from_images(critical, false, true).map_err(js_error)?,
            names.into_iter().map(|h| (h.class_name, h.name)).collect(),
        );
        let mut coach = Coach { scanner, engine: Engine::new(vec![]), timings: timings.into(),
            counters: counters.trim_start_matches('\u{feff}').into(), rules: json(rules)?,
            options: Options::default(), match_id: None, last_clock: None };
        coach.engine = coach.engine()?;
        Ok(coach)
    }

    pub fn configure(&mut self, options_json: &str) -> Result<(), JsValue> {
        self.options = json(options_json)?;
        self.engine = self.engine()?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), JsValue> {
        self.scanner.reset(); self.match_id = None; self.last_clock = None;
        self.engine = self.engine()?; Ok(())
    }

    /// RGBA transfer avoids full-frame JPEG encoding, preserving HUD edges.
    pub fn analyze(&mut self, rgba: Vec<u8>, width: u32, height: u32, map: Vec<u8>,
        map_width: u32, map_height: u32, now_ms: f64) -> Result<JsValue, JsValue> {
        if width < 320 || height < 180 || width > 4096 || height > 2304 || !now_ms.is_finite() || now_ms < 0.0 {
            return Err(js_error("invalid frame dimensions or timestamp"));
        }
        let image = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| js_error("invalid RGBA buffer length"))?;
        let image = image::DynamicImage::ImageRgba8(image);
        let map = if map_width >= 100 && map_width <= 1024 && map_height >= 100 && map_height <= 1024
            && (map_width as f32 / map_height as f32 - 335.0 / 350.0).abs() < 0.05 {
            image::RgbaImage::from_raw(map_width, map_height, map).map(|m| image::DynamicImage::ImageRgba8(m).to_rgb8())
        } else { None };
        let snapshot = self.scanner.scan_image(&image, map.as_ref(), now_ms as u64);
        let cues = self.step(&snapshot)?;
        serde_wasm_bindgen::to_value(&Analysis { snapshot, cues }).map_err(js_error)
    }

    /// Replay imports traverse the very same rule engine as live frames.
    pub fn replay(&mut self, snapshot_json: &str) -> Result<JsValue, JsValue> {
        let snapshot: GameSnapshot = json(snapshot_json)?;
        let cues = self.step(&snapshot)?;
        serde_wasm_bindgen::to_value(&Analysis { snapshot, cues }).map_err(js_error)
    }
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
