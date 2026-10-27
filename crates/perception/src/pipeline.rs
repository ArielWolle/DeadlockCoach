use crate::matching::{ncc, ColorLibrary};
#[cfg(feature = "ocrs-backend")]
use crate::ocr::Reader;
use base64::Engine as _;
use image::{DynamicImage, imageops::FilterType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, HashMap, HashSet}, sync::Arc};

fn one() -> f32 { 1.0 }
fn default_width() -> u32 { 1600 }
fn default_height() -> u32 { 900 }
fn default_adapter() -> String { "generic".into() }
fn default_scale() -> u32 { 3 }
fn default_frames() -> u32 { 2 }
fn default_age() -> u64 { 15000 }
fn default_step() -> u32 { 8 }
fn default_score() -> f32 { 0.7 }
fn enabled() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default = "default_adapter")] pub adapter: String,
    #[serde(default)] pub viewport: Viewport,
    #[serde(default)] pub templates: Vec<EmbeddedTemplate>,
    pub nodes: Vec<Node>,
    pub outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Viewport { pub aspect_ratio: Option<f32>, pub max_width: u32, pub auto_crop:bool }
impl Default for Viewport { fn default() -> Self { Self { aspect_ratio: None, max_width: 1600, auto_crop:true } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddedTemplate { pub id: String, pub png_base64: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    #[serde(default)] pub inputs: Vec<String>,
    #[serde(default)] pub when: Option<String>,
    pub op: Operation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Space { #[default] Normalized, Pixels, Reference }
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Anchor { #[default] TopLeft, TopRight, BottomLeft, BottomRight, Center }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: f32, pub y: f32, pub w: f32, pub h: f32,
    #[serde(default)] pub space: Space,
    #[serde(default)] pub anchor: Anchor,
    #[serde(default = "default_width")] pub reference_width: u32,
    #[serde(default = "default_height")] pub reference_height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Bounds { pub x: u32, pub y: u32, pub w: u32, pub h: u32 }

#[derive(Debug,Clone,Copy,Serialize,Deserialize,Default,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub enum RegionOrigin { #[default] Configured, Detected }
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct RegionObservation {pub id:String,pub label:String,pub bounds:Bounds,pub origin:RegionOrigin,pub score:Option<f32>,#[serde(default)] pub value_path:Option<String>}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode { #[default] Detect, Line }
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parser { Text, Number, Clock }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Crop { region: Region, #[serde(default)] anchor_input: bool },
    Preprocess { #[serde(default)] grayscale: bool, #[serde(default)] invert: bool,
        #[serde(default)] threshold: Option<u8>, #[serde(default = "one")] scale: f32 },
    Ocr { #[serde(default)] mode: OcrMode, #[serde(default = "default_scale")] upscale: u32 },
    LocateText { pattern:String, #[serde(default)] index:usize },
    Extract { pattern: String, #[serde(default = "default_capture")] group: usize },
    Parse { parser: Parser, #[serde(default)] min: Option<f64>, #[serde(default)] max: Option<f64>,
        #[serde(default = "one")] scale: f32, #[serde(default)] decimal_comma: bool },
    Contains { text: String, #[serde(default)] case_sensitive: bool },
    Select { path: String },
    Gate { #[serde(default)] min_score: Option<f32>, #[serde(default)] equals: Option<Value> },
    Confirm { #[serde(default = "default_frames")] frames: u32, #[serde(default)] tolerance: f64,
        #[serde(default = "default_age")] max_age_ms: u64, #[serde(default)] max_delta: Option<f64> },
    Coalesce,
    Status { one_of: Vec<Status> },
    Classify { library: String, #[serde(default = "default_score")] min_score: f32, #[serde(default)] min_margin: f32 },
    Match { templates: Vec<String>, #[serde(default = "default_score")] min_score: f32, #[serde(default)] min_margin: f32,
        #[serde(default = "enabled")] color: bool },
    Locate { template: String, #[serde(default = "default_scales")] scales: Vec<f32>,
        #[serde(default = "default_step")] step: u32, #[serde(default = "default_score")] min_score: f32 },
    ColorRatio { rgb: [u8; 3], tolerance: u8 },
    Custom { name: String, #[serde(default)] config: Value },
}
fn default_capture() -> usize { 1 }
fn default_scales() -> Vec<f32> { vec![1.0] }

impl Operation {
    pub fn kind(&self) -> &'static str {
        match self { Self::Crop {..} => "crop", Self::Preprocess {..} => "preprocess", Self::Ocr {..} => "ocr",
            Self::Extract {..} => "extract", Self::LocateText {..}=>"locate_text", Self::Parse {..} => "parse", Self::Contains {..} => "contains",
            Self::Select {..} => "select", Self::Gate {..} => "gate", Self::Confirm {..} => "confirm",
            Self::Coalesce => "coalesce", Self::Status {..} => "status", Self::Classify {..} => "classify", Self::Match {..} => "match",
            Self::Locate {..} => "locate", Self::ColorRatio {..} => "color_ratio", Self::Custom {..} => "custom" }
    }
}

/// Internal stage data. Images stay in-process; reports never serialize raw pixels.
#[derive(Clone, Default)]
pub struct Packet {
    pub image: Option<Arc<DynamicImage>>,
    pub value: Option<Value>,
    /// Detector similarity score, not a calibrated probability. OCR without scores stays None.
    pub score: Option<f32>,
    pub bounds: Option<Bounds>,
    pub bounds_origin:RegionOrigin,
    pub regions:Vec<RegionObservation>,
}

pub struct FrameContext<'a> { pub frame: &'a DynamicImage, pub auxiliary: Option<&'a image::RgbImage>, pub timestamp_ms: u64, pub node_id: &'a str }
pub trait Stage {
    fn run(&mut self, inputs: &[Packet], frame: &FrameContext<'_>, config: &Value) -> Result<Packet, String>;
    fn reset(&mut self) {}
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status { Ok, Missing, Pending, Skipped, Error }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reading {
    pub status: Status,
    pub value: Option<Value>,
    pub score: Option<f32>,
    pub bounds: Option<Bounds>,
    pub source: String,
    #[serde(default)] pub bounds_origin:RegionOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub id: String, pub operation: String, pub inputs: Vec<String>, pub status: Status,
    pub elapsed_ms: f64, pub value: Option<Value>, pub score: Option<f32>, pub bounds: Option<Bounds>,
    pub image_size: Option<[u32; 2]>, pub error: Option<String>,
    #[serde(default)] pub bounds_origin:RegionOrigin,
    #[serde(default)] pub regions:Vec<RegionObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32, pub profile_id: String, pub frame_id: u64, pub timestamp_ms: u64,
    pub profile_revision: String,
    pub frame_size: [u32; 2],
    pub fields: BTreeMap<String, Reading>, pub stages: Vec<Trace>,
}

#[derive(Default)]
pub struct Resources {
    pub ocr: Option<Arc<dyn TextBackend>>,
    pub libraries: BTreeMap<String, Arc<ColorLibrary>>,
    pub stages: BTreeMap<String, Box<dyn Stage>>,
}

#[derive(Clone)]
pub struct TextResult { pub text: String, pub score: Option<f32>, pub regions:Vec<RegionObservation> }
/// Replace the OCR implementation without changing profile execution or game adapters.
pub trait TextBackend {
    fn recognize(&self, image: &DynamicImage, mode: OcrMode, upscale: u32) -> Result<TextResult, String>;
    /// Recognize several already-located single lines. Backends should batch these into
    /// one inference call; the default keeps correctness for simple backends.
    fn recognize_lines(&self, images: &[DynamicImage]) -> Result<Vec<TextResult>, String> {
        images.iter().map(|i| self.recognize(i, OcrMode::Line, 1)).collect()
    }
}
#[cfg(feature = "ocrs-backend")]
impl TextBackend for Reader {
    fn recognize(&self, image: &DynamicImage, mode: OcrMode, upscale: u32) -> Result<TextResult, String> {
        let mut regions=vec![];
        let text = match mode {
            OcrMode::Detect => {regions=self.read_spans(image,upscale)?;regions.iter().map(|r|r.label.clone()).collect::<Vec<_>>().join("\n")},
            OcrMode::Line if upscale == 1 => self.read_line(image)?,
            OcrMode::Line => self.read_line(&image.resize_exact(image.width() * upscale, image.height() * upscale, FilterType::CatmullRom))?,
        };
        Ok(TextResult { text, score: None,regions })
    }
    fn recognize_lines(&self, images: &[DynamicImage]) -> Result<Vec<TextResult>, String> {
        Ok(self.read_lines(images)?.into_iter().map(|text| TextResult { text, score: None, regions: vec![] }).collect())
    }
}
type OcrCache = HashMap<(usize, bool, u32), Result<TextResult, String>>;

struct Template { image: Arc<DynamicImage>, gray: image::GrayImage, color: Vec<f32> }
#[derive(Default)]
struct History { value: Option<Value>, timestamp: u64, count: u32 }
pub struct Pipeline {
    profile: Profile,
    order: Vec<usize>,
    pub resources: Resources,
    templates: BTreeMap<String, Template>,
    regexes: HashMap<String, Regex>,
    history: HashMap<String, History>,
    frame_id: u64,
    revision: String,
    /// Line-OCR results computed up front in one batched backend call, keyed by node id.
    prefetched: HashMap<String, TextResult>,
}

pub fn profile_from_json(text: &str) -> Result<Profile, String> {
    if text.len() > 4 * 1024 * 1024 { return Err("profile exceeds 4 MB".into()); }
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

impl Pipeline {
    pub fn new(profile: Profile, resources: Resources) -> Result<Self, String> {
        let mut pipeline = Self { profile: profile.clone(), order: vec![], resources, templates: BTreeMap::new(),
            regexes: HashMap::new(), history: HashMap::new(), frame_id: 0, revision: String::new(), prefetched: HashMap::new() };
        pipeline.set_profile(profile)?; Ok(pipeline)
    }
    pub fn profile(&self) -> &Profile { &self.profile }
    pub fn reset(&mut self) {
        self.history.clear(); self.frame_id = 0;
        for stage in self.resources.stages.values_mut() { stage.reset(); }
    }
    /// Compile atomically: a bad imported profile leaves the previous pipeline intact.
    pub fn set_profile(&mut self, profile: Profile) -> Result<(), String> {
        let order = validate(&profile, &self.resources)?;
        let mut templates = BTreeMap::new();
        for template in &profile.templates {
            let bytes = base64::engine::general_purpose::STANDARD.decode(&template.png_base64).map_err(|e| format!("{}: {e}", template.id))?;
            if bytes.len() > 512 * 1024 { return Err(format!("{}: template exceeds 512 KB", template.id)); }
            let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
            let mut limits = image::Limits::default(); limits.max_image_width = Some(1024); limits.max_image_height = Some(1024); limits.max_alloc = Some(16 * 1024 * 1024); reader.limits(limits);
            let image = reader.decode().map_err(|e| format!("{}: {e}", template.id))?;
            let gray = image::imageops::resize(&image.to_luma8(), 24, 24, FilterType::Triangle);
            let color = crate::matching::color_descriptor(&image);
            templates.insert(template.id.clone(), Template { image: Arc::new(image), gray, color });
        }
        let mut regexes = HashMap::new();
        for node in &profile.nodes { if let Operation::Extract { pattern, .. } | Operation::LocateText {pattern,..} = &node.op {
            regexes.insert(node.id.clone(), Regex::new(pattern).map_err(|e| e.to_string())?);
        }}
        fn canonical(v: Value) -> Value { match v {
            Value::Object(values) => Value::Object(values.into_iter().collect::<BTreeMap<_,_>>().into_iter().map(|(k,v)| (k,canonical(v))).collect()),
            Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()), other => other,
        }}
        let bytes = serde_json::to_vec(&canonical(serde_json::to_value(&profile).map_err(|e| e.to_string())?)).map_err(|e| e.to_string())?;
        self.revision = format!("{:x}", Sha256::digest(bytes));
        self.profile = profile; self.order = order; self.templates = templates; self.regexes = regexes; self.reset(); Ok(())
    }

    pub fn run(&mut self, frame: &DynamicImage, auxiliary: Option<&image::RgbImage>, timestamp_ms: u64) -> Report {
        self.frame_id += 1;
        let context = FrameContext { frame, auxiliary, timestamp_ms, node_id: "$frame" };
        let root = Packet { image: Some(Arc::new(frame.clone())), bounds: Some(Bounds { x: 0, y: 0, w: frame.width(), h: frame.height() }), ..Default::default() };
        let mut packets: HashMap<String, (Status, Packet)> = HashMap::from([("$frame".into(), (Status::Ok, root))]);
        let mut traces = vec![];
        let mut ocr_cache = OcrCache::new();
        let mut pixel_budget = 64_000_000u64;
        self.prefetch_lines(frame);
        for index in self.order.clone() {
            let node = self.profile.nodes[index].clone();
            let ids = if node.inputs.is_empty() { vec!["$frame".into()] } else { node.inputs.clone() };
            let inputs: Vec<_> = ids.iter().map(|id| packets[id].clone()).collect();
            let start = web_time::Instant::now();
            let conditional = node.when.as_ref().map(|id| packets[id].0 == Status::Ok && packets[id].1.value == Some(Value::Bool(true))).unwrap_or(true);
            let upstream_missing = !matches!(node.op, Operation::Coalesce) && inputs.iter().any(|(s, _)| *s != Status::Ok);
            let result = if !conditional { self.history.remove(&node.id); Ok((Status::Skipped, Packet::default())) }
                else if let Operation::Status { one_of } = &node.op { Ok((Status::Ok, Packet { value: Some(json!(one_of.contains(&inputs[0].0))), ..Default::default() })) }
                else if upstream_missing { self.history.remove(&node.id); Ok((Status::Skipped, Packet::default())) }
                else { self.execute(&node, &inputs.into_iter().map(|(_, p)| p).collect::<Vec<_>>(), &context, &mut ocr_cache, &mut pixel_budget) };
            let (status, packet, error) = match result { Ok((status, p)) => (status, p, None), Err(e) => { self.history.remove(&node.id); (Status::Error, Packet::default(), Some(e)) } };
            traces.push(Trace { id: node.id.clone(), operation: node.op.kind().into(), inputs: ids, status,
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.0, value: packet.value.clone(), score: packet.score,
                bounds: packet.bounds, image_size: packet.image.as_ref().map(|i| [i.width(), i.height()]), error,
                bounds_origin:packet.bounds_origin,regions:packet.regions.clone() });
            packets.insert(node.id, (status, packet));
        }
        let fields = self.profile.outputs.iter().map(|(field, source)| {
            let (status, p) = &packets[source];
            (field.clone(), Reading { status: *status, value: if *status == Status::Ok { p.value.clone() } else { None }, score: p.score, bounds: p.bounds, source: source.clone(), bounds_origin:p.bounds_origin })
        }).collect();
        Report { schema_version: 1, profile_id: self.profile.id.clone(), frame_id: self.frame_id, timestamp_ms,
            profile_revision: self.revision.clone(), frame_size: [frame.width(), frame.height()], fields, stages: traces }
    }

    /// Batch every unconditional `ocr(line)` whose input is a fixed crop of the frame into a
    /// single recognition call. Results are consumed by the normal OCR stage, so traces,
    /// parsing and confirmation behave exactly as without batching.
    fn prefetch_lines(&mut self, frame: &DynamicImage) {
        self.prefetched.clear();
        let Some(backend) = self.resources.ocr.clone() else { return };
        let ids: HashMap<&str, &Node> = self.profile.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut keys = vec![]; let mut crops = vec![];
        for node in &self.profile.nodes {
            let Operation::Ocr { mode: OcrMode::Line, upscale } = &node.op else { continue };
            if node.when.is_some() || node.inputs.len() != 1 { continue; }
            let Some(source) = ids.get(node.inputs[0].as_str()) else { continue };
            let Operation::Crop { region, anchor_input: false } = &source.op else { continue };
            if source.when.is_some() || !(source.inputs.is_empty() || source.inputs == ["$frame"]) { continue; }
            let Ok(b) = region_bounds(region, frame.width(), frame.height()) else { continue };
            if b.w as u64 * b.h as u64 * (*upscale as u64).pow(2) > 16_000_000 { continue; }
            let crop = frame.crop_imm(b.x, b.y, b.w, b.h);
            let crop = if *upscale > 1 { crop.resize_exact(b.w * upscale, b.h * upscale, FilterType::CatmullRom) } else { crop };
            keys.push(node.id.clone()); crops.push(crop);
        }
        if keys.len() < 2 { return; }
        if let Ok(results) = backend.recognize_lines(&crops) {
            if results.len() == keys.len() { self.prefetched = keys.into_iter().zip(results).collect(); }
        }
    }

    fn execute(&mut self, node: &Node, inputs: &[Packet], context: &FrameContext<'_>,
        cache: &mut OcrCache, budget: &mut u64) -> Result<(Status, Packet), String> {
        let input = &inputs[0];
        if input.image.as_ref().is_some_and(|i| i.width() == 0 || i.height() == 0) { return Err("empty input image".into()); }
        let mut out = input.clone();
        out.regions.clear();
        match &node.op {
            Operation::Crop { region, anchor_input } => {
                let image = input.image.as_ref().ok_or("crop needs an image")?;
                let base = input.bounds.unwrap_or(Bounds { x: 0, y: 0, w: image.width(), h: image.height() });
                let (relative, bounds) = if *anchor_input {
                    let anchor = inputs[1].bounds.ok_or("anchor has no bounds")?;
                    let (x, y, w, h) = region_coordinates(region, anchor.w, anchor.h);
                    let (x, y) = (anchor.x as f32 + x, anchor.y as f32 + y);
                    if x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite() || x + w > context.frame.width() as f32 || y + h > context.frame.height() as f32 {
                        return Ok((Status::Missing, Packet::default()));
                    }
                    let b = Bounds { x: x.round() as u32, y: y.round() as u32, w: w.round() as u32, h: h.round() as u32 };
                    if b.w == 0 || b.h == 0 { return Ok((Status::Missing, Packet::default())); } (b, b)
                } else {
                    let relative = region_bounds(region, image.width(), image.height())?;
                    (relative, map_bounds(relative, base, image.width(), image.height()))
                };
                // Anchored crops use original-frame coordinates; regular crops use their image.
                let crop = if *anchor_input {
                    if bounds.x + bounds.w > context.frame.width() || bounds.y + bounds.h > context.frame.height() { return Ok((Status::Missing, Packet::default())); }
                    context.frame.crop_imm(bounds.x, bounds.y, bounds.w, bounds.h)
                } else { image.crop_imm(relative.x, relative.y, relative.w, relative.h) };
                spend(budget, crop.width(), crop.height())?;
                out = Packet { image: Some(Arc::new(crop)), bounds: Some(bounds), ..Default::default() };
                out.bounds_origin=if *anchor_input {inputs[1].bounds_origin} else {input.bounds_origin};
                out.regions.push(RegionObservation{id:node.id.clone(),label:node.id.clone(),bounds,origin:out.bounds_origin,score:None,value_path:None});
            }
            Operation::Preprocess { grayscale, invert, threshold, scale } => {
                let mut image = (**input.image.as_ref().ok_or("preprocess needs an image")?).clone();
                let (w, h) = (((image.width() as f32 * scale).round() as u32).max(1), ((image.height() as f32 * scale).round() as u32).max(1));
                spend(budget, w, h)?;
                if *grayscale || threshold.is_some() { image = image.grayscale(); }
                if let Some(t) = threshold { let mut gray = image.to_luma8(); for p in gray.pixels_mut() { p[0] = if p[0] >= *t { 255 } else { 0 }; } image = DynamicImage::ImageLuma8(gray); }
                if *invert { image.invert(); }
                if *scale != 1.0 { image = image.resize_exact(w, h, FilterType::CatmullRom); }
                out.image = Some(Arc::new(image)); out.value = None;
            }
            Operation::Ocr { mode, upscale } => {
                let image = input.image.as_ref().ok_or("OCR needs an image")?;
                if image.width() as u64 * image.height() as u64 * (*upscale as u64).pow(2) > 16_000_000 { return Err("OCR input exceeds pixel budget; crop before recognition".into()); }
                let reader = self.resources.ocr.as_ref().ok_or("OCR backend is not loaded")?;
                let line = matches!(mode, OcrMode::Line);
                let key = (Arc::as_ptr(image) as usize, line, *upscale);
                let result = match self.prefetched.remove(&node.id) {
                    Some(batched) => batched,
                    None => cache.entry(key).or_insert_with(|| reader.recognize(image, *mode, *upscale)).clone()?,
                };
                let text: String = result.text.chars().take(8192).collect();
                if text.trim().is_empty() { return Ok((Status::Missing, Packet::default())); }
                out.value = Some(Value::String(text)); out.score = result.score.filter(|s| s.is_finite());
                let origin=input.bounds.unwrap_or(Bounds{x:0,y:0,w:image.width(),h:image.height()});
                out.regions=result.regions.into_iter().map(|mut r|{r.bounds=map_bounds(r.bounds,origin,image.width(),image.height());r}).collect();
            }
            Operation::LocateText { index,.. } => {
                let Some(region)=input.regions.iter().filter(|r|self.regexes[&node.id].is_match(&r.label)).nth(*index).cloned()else{return Ok((Status::Missing,Packet::default()));};
                let b=region.bounds;
                if b.w==0||b.h==0||b.x+b.w>context.frame.width()||b.y+b.h>context.frame.height(){return Err("text detector returned invalid bounds".into());}
                out.image=Some(Arc::new(context.frame.crop_imm(b.x,b.y,b.w,b.h)));out.bounds=Some(b);out.bounds_origin=region.origin;
                out.value=Some(json!(region.label));out.score=region.score;out.regions=vec![RegionObservation{id:node.id.clone(),..region}];
            }
            Operation::Extract { group, .. } => {
                let Some(text) = input.value.as_ref().and_then(Value::as_str) else { return Ok((Status::Missing, Packet::default())); };
                let value = self.regexes[&node.id].captures(text).and_then(|c| c.get(*group)).map(|s| s.as_str().to_string());
                let Some(value) = value else { return Ok((Status::Missing, Packet::default())); }; out.value = Some(Value::String(value));
            }
            Operation::Parse { parser, min, max, scale, decimal_comma } => {
                let Some(value) = &input.value else { return Ok((Status::Missing, Packet::default())); };
                let parsed = parse_value(value, *parser, *decimal_comma);
                let Some(mut parsed) = parsed else { return Ok((Status::Missing, Packet::default())); };
                if let Some(number) = parsed.as_f64() {
                    let n = number * *scale as f64;
                    if !n.is_finite() || min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) { return Ok((Status::Missing, Packet::default())); }
                    parsed = json!(n);
                }
                out.value = Some(parsed);
            }
            Operation::Contains { text, case_sensitive } => {
                let Some(value) = input.value.as_ref().and_then(Value::as_str) else { return Ok((Status::Missing, Packet::default())); };
                out.value = Some(Value::Bool(if *case_sensitive { value.contains(text) } else { value.to_lowercase().contains(&text.to_lowercase()) }));
            }
            Operation::Select { path } => {
                let value = input.value.as_ref().and_then(|v| v.pointer(path)).filter(|v| !v.is_null()).cloned();
                if value.is_none() { return Ok((Status::Missing, Packet::default())); } out.value = value;
                if let Some(region)=input.regions.iter().find(|r|r.value_path.as_deref()==Some(path)){
                    out.bounds=Some(region.bounds);out.bounds_origin=region.origin;out.score=region.score;
                }
            }
            Operation::Gate { min_score, equals } => {
                if min_score.is_some_and(|m| input.score.is_none_or(|s| s < m)) || equals.as_ref().is_some_and(|v| input.value.as_ref() != Some(v)) {
                    out.value = None; out.image = None; return Ok((Status::Missing, out));
                }
            }
            Operation::Confirm { frames, tolerance, max_age_ms, max_delta } => {
                let Some(value) = &input.value else { self.history.remove(&node.id); return Ok((Status::Missing, Packet::default())); };
                let state = self.history.entry(node.id.clone()).or_default();
                let fresh = context.timestamp_ms >= state.timestamp && context.timestamp_ms - state.timestamp <= *max_age_ms;
                let nearby = state.value.as_ref().is_some_and(|old| values_match(old, value, *tolerance)
                    && max_delta.is_none_or(|m| old.as_f64().zip(value.as_f64()).is_none_or(|(a,b)| (a-b).abs() <= m)));
                state.count = if fresh && nearby { state.count.saturating_add(1) } else { 1 };
                state.timestamp = context.timestamp_ms; state.value = Some(value.clone());
                if state.count < *frames { out.value = None; return Ok((Status::Pending, out)); }
            }
            Operation::Coalesce => {
                let Some(packet) = inputs.iter().find(|p| p.value.is_some()) else { return Ok((Status::Missing, Packet::default())); }; out = packet.clone();
            }
            Operation::Status { .. } => unreachable!("status probes are handled before missing-input propagation"),
            Operation::Classify { library, min_score, min_margin } => {
                let image = input.image.as_ref().ok_or("classify needs an image")?;
                let lib = &self.resources.libraries[library];
                let Some((name, score, margin)) = lib.identify(image) else { return Ok((Status::Missing, Packet::default())); };
                if score < *min_score || margin < *min_margin { out.value = None; out.image = None; out.score = Some(score); return Ok((Status::Missing, out)); }
                out.value = Some(json!(name)); out.score = Some(score);
            }
            Operation::Match { templates, min_score, min_margin, color } => {
                let image = input.image.as_ref().ok_or("match needs an image")?;
                let gray = image::imageops::resize(&image.to_luma8(), 24, 24, FilterType::Triangle);
                let rgb = if *color { crate::matching::color_descriptor(image) } else { vec![] };
                let mut matches: Vec<_> = templates.iter().map(|id| {
                    let template = &self.templates[id];
                    let score = if *color { rgb.iter().zip(&template.color).map(|(a,b)| a*b).sum() } else { ncc(&gray, &template.gray) };
                    (score, id)
                }).collect();
                matches.sort_by(|a,b| b.0.total_cmp(&a.0));
                let (score, id) = matches[0]; let second = matches.get(1).map(|v| v.0).unwrap_or(-1.0);
                if score < *min_score || score - second < *min_margin { out.value = None; out.image = None; out.score = Some(score); return Ok((Status::Missing, out)); }
                out.value = Some(json!(id)); out.score = Some(score);
            }
            Operation::Locate { template, scales, step, min_score } => {
                let image = input.image.as_ref().ok_or("locate needs an image")?;
                let source = image.to_luma8(); let target = &self.templates[template];
                let mut best: Option<(f32, Bounds)> = None; let mut candidates = 0;
                let mut peaks: Vec<(f32, Bounds)> = vec![];
                let exhaustive: u64 = scales.iter().map(|scale| {
                    let w = (target.image.width() as f32 * scale).round() as u32;
                    let h = (target.image.height() as f32 * scale).round() as u32;
                    if w == 0 || h == 0 || w > source.width() || h > source.height() { 0 }
                    else { (source.width()-w+1) as u64 * (source.height()-h+1) as u64 }
                }).sum();
                let stride = if exhaustive <= 20000 { 1 } else { *step as usize };
                for scale in scales {
                    let w = (target.image.width() as f32 * scale).round() as u32;
                    let h = (target.image.height() as f32 * scale).round() as u32;
                    if w == 0 || h == 0 || w > source.width() || h > source.height() { continue; }
                    for y in (0..=source.height()-h).step_by(stride) { for x in (0..=source.width()-w).step_by(stride) {
                        candidates += 1; if candidates > 25000 { return Err("template search exceeded 25,000 candidates; crop the search area or increase step".into()); }
                        let crop = image::imageops::crop_imm(&source, x, y, w, h).to_image();
                        let normalized = image::imageops::resize(&crop, 24, 24, FilterType::Triangle);
                        let score = ncc(&normalized, &target.gray);
                        if best.as_ref().is_none_or(|b| score > b.0) { best = Some((score, Bounds { x, y, w, h })); }
                        peaks.push((score, Bounds { x, y, w, h })); peaks.sort_by(|a,b| b.0.total_cmp(&a.0)); peaks.truncate(4);
                    }}
                }
                // Refine the strongest coarse candidates at pixel precision, so anchors
                // are not restricted to positions that happen to align with `step`.
                let mut visited = HashSet::new();
                for (_, peak) in peaks.into_iter().filter(|_| stride != 1) {
                    for y in peak.y.saturating_sub(*step)..=(peak.y + step).min(source.height()-peak.h) {
                        for x in peak.x.saturating_sub(*step)..=(peak.x + step).min(source.width()-peak.w) {
                            if !visited.insert((x,y,peak.w,peak.h)) { continue; }
                            candidates += 1; if candidates > 25000 { return Err("template refinement exceeded search budget; reduce search area/scales".into()); }
                            let crop = image::imageops::crop_imm(&source,x,y,peak.w,peak.h).to_image();
                            let gray = image::imageops::resize(&crop,24,24,FilterType::Triangle);
                            let score = ncc(&gray,&target.gray);
                            if best.as_ref().is_none_or(|b| score > b.0) { best = Some((score,Bounds { x,y,..peak })); }
                        }
                    }
                }
                let Some((score, local)) = best else { return Ok((Status::Missing, Packet::default())); };
                if score < *min_score { out.value = None; out.image = None; out.score = Some(score); return Ok((Status::Missing, out)); }
                let origin = input.bounds.unwrap_or(Bounds { x: 0, y: 0, w: image.width(), h: image.height() });
                out.image = Some(Arc::new(image.crop_imm(local.x, local.y, local.w, local.h)));
                out.bounds = Some(map_bounds(local, origin, image.width(), image.height())); out.value = Some(json!(template)); out.score = Some(score);
                out.bounds_origin=RegionOrigin::Detected;
                out.regions.push(RegionObservation{id:node.id.clone(),label:template.clone(),bounds:out.bounds.unwrap(),origin:RegionOrigin::Detected,score:Some(score),value_path:None});
            }
            Operation::ColorRatio { rgb, tolerance } => {
                let image = input.image.as_ref().ok_or("color ratio needs an image")?.to_rgb8();
                let count = image.pixels().filter(|p| p.0.iter().zip(rgb).all(|(a,b)| a.abs_diff(*b) <= *tolerance)).count();
                out.value = Some(json!(count as f64 / (image.width() as f64 * image.height() as f64)));
            }
            Operation::Custom { name, config } => {
                let stage_context = FrameContext { frame: context.frame, auxiliary: context.auxiliary, timestamp_ms: context.timestamp_ms, node_id: &node.id };
                out = self.resources.stages.get_mut(name).ok_or("custom stage missing")?.run(inputs, &stage_context, config)?;
            }
        }
        Ok((Status::Ok, out))
    }
}

fn spend(budget: &mut u64, w: u32, h: u32) -> Result<(), String> {
    let n = w as u64 * h as u64;
    if n == 0 || n > 16_000_000 || n > *budget { return Err("image processing pixel budget exceeded".into()); }
    *budget -= n; Ok(())
}
fn values_match(a: &Value, b: &Value, tolerance: f64) -> bool {
    a.as_f64().zip(b.as_f64()).map(|(a,b)| (a-b).abs() <= tolerance).unwrap_or(a == b)
}

fn map_bounds(local: Bounds, original: Bounds, width: u32, height: u32) -> Bounds {
    let x = ((local.x as f64 * original.w as f64 / width as f64).round() as u32).min(original.w.saturating_sub(1));
    let y = ((local.y as f64 * original.h as f64 / height as f64).round() as u32).min(original.h.saturating_sub(1));
    let right = ((local.x as f64 + local.w as f64) * original.w as f64 / width as f64).round() as u32;
    let bottom = ((local.y as f64 + local.h as f64) * original.h as f64 / height as f64).round() as u32;
    Bounds { x: original.x + x, y: original.y + y,
        w: right.max(x+1).min(original.w)-x, h: bottom.max(y+1).min(original.h)-y }
}

fn region_coordinates(r: &Region, width: u32, height: u32) -> (f32, f32, f32, f32) {
    let (sx, sy) = match r.space { Space::Normalized => (width as f32, height as f32), Space::Pixels => (1.0, 1.0),
        Space::Reference => (width as f32 / r.reference_width as f32, height as f32 / r.reference_height as f32) };
    let (w, h) = ((r.w * sx).round(), (r.h * sy).round());
    let (dx, dy) = (r.x * sx, r.y * sy);
    let x = match r.anchor { Anchor::TopRight | Anchor::BottomRight => width as f32 - w - dx,
        Anchor::Center => (width as f32 - w) / 2.0 + dx, _ => dx };
    let y = match r.anchor { Anchor::BottomLeft | Anchor::BottomRight => height as f32 - h - dy,
        Anchor::Center => (height as f32 - h) / 2.0 + dy, _ => dy };
    (x, y, w, h)
}

pub fn region_bounds(r: &Region, width: u32, height: u32) -> Result<Bounds, String> {
    let (x,y,w,h) = region_coordinates(r, width, height);
    if w < 1.0 || h < 1.0 || !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() || x < 0.0 || y < 0.0
        || x + w > width as f32 || y + h > height as f32 {
        return Err("region is outside its input image".into());
    }
    Ok(Bounds { x: x.round() as u32, y: y.round() as u32, w: w as u32, h: h as u32 })
}

pub fn parse_value(value: &Value, parser: Parser, decimal_comma: bool) -> Option<Value> {
    if matches!(parser, Parser::Text) { return value.as_str().map(|v| json!(v.trim())); }
    if let Some(n) = value.as_f64() { return n.is_finite().then(|| json!(n)); }
    let text = value.as_str()?;
    if matches!(parser, Parser::Clock) {
        let regex = Regex::new(r"\b(\d{1,3}):([0-5]\d)(?::([0-5]\d))?\b").unwrap();
        let c = regex.captures(text)?;
        let matched = c.get(0)?;
        if text[..matched.start()].ends_with(':') || text[matched.end()..].starts_with(':') { return None; }
        let a = c[1].parse::<u32>().ok()?; let b = c[2].parse::<u32>().ok()?;
        return Some(json!(if let Some(s) = c.get(3) { a * 3600 + b * 60 + s.as_str().parse::<u32>().ok()? } else { a * 60 + b }));
    }
    let regex = Regex::new(r"[+-]?\d+(?:[.,]\d+)*(?:[kKmM])?").unwrap();
    let raw = regex.find(text)?.as_str();
    let multiplier = if raw.ends_with(['k','K']) { 1000.0 } else if raw.ends_with(['m','M']) { 1_000_000.0 } else { 1.0 };
    let raw = raw.trim_end_matches(['k','K','m','M']);
    let normalized = if decimal_comma { raw.replace('.', "").replace(',', ".") } else { raw.replace(',', "") };
    let number = normalized.parse::<f64>().ok()? * multiplier;
    number.is_finite().then(|| json!(number))
}

fn validate(profile: &Profile, resources: &Resources) -> Result<Vec<usize>, String> {
    if profile.schema_version != 1 { return Err("unsupported profile schema_version (expected 1)".into()); }
    if profile.id.is_empty() || profile.id.len() > 80 || profile.name.is_empty() || profile.name.len() > 120 { return Err("profile id/name is missing or too long".into()); }
    if profile.nodes.is_empty() || profile.nodes.len() > 128 || profile.outputs.is_empty() || profile.outputs.len() > 64 || profile.templates.len() > 64 { return Err("profile requires 1–128 nodes, 1–64 outputs and at most 64 templates".into()); }
    if !(320..=4096).contains(&profile.viewport.max_width) || profile.viewport.aspect_ratio.is_some_and(|r| !r.is_finite() || !(0.2..=8.0).contains(&r)) { return Err("invalid viewport constraints".into()); }
    let mut ids = HashMap::new();
    for (i, n) in profile.nodes.iter().enumerate() { if n.id.is_empty() || n.id.len() > 80 || n.id.starts_with('$') || ids.insert(n.id.as_str(), i).is_some() { return Err(format!("duplicate or invalid node id: {}", n.id)); } }
    let mut template_ids = HashSet::new();
    for t in &profile.templates { if t.id.is_empty() || !template_ids.insert(t.id.as_str()) { return Err("template IDs must be unique and nonempty".into()); } }
    for n in &profile.nodes {
        for input in n.inputs.iter().chain(n.when.iter()) { if input != "$frame" && !ids.contains_key(input.as_str()) { return Err(format!("{}: missing input {input}", n.id)); } }
        if n.inputs.len() > 8 { return Err(format!("{}: at most 8 inputs", n.id)); }
        match &n.op {
            Operation::Crop { region: r, anchor_input } => {
                if ![r.x,r.y,r.w,r.h].iter().all(|v| v.is_finite()) || r.w <= 0.0 || r.h <= 0.0 || r.reference_width == 0 || r.reference_height == 0 { return Err(format!("{}: invalid crop region", n.id)); }
                if *anchor_input && n.inputs.len() != 2 { return Err(format!("{}: anchored crop requires image and anchor inputs", n.id)); }
            }
            Operation::Preprocess { scale, .. } if !scale.is_finite() || !(0.1..=8.0).contains(scale) => return Err(format!("{}: preprocess scale must be 0.1–8", n.id)),
            Operation::Ocr { upscale, .. } => { if resources.ocr.is_none() || !(1..=6).contains(upscale) { return Err(format!("{}: OCR needs a loaded backend and upscale 1–6", n.id)); } }
            Operation::Extract { pattern, group } => {
                if pattern.len() > 1024 { return Err(format!("{}: extraction pattern too long", n.id)); }
                let r = Regex::new(pattern).map_err(|e| format!("{}: {e}", n.id))?;
                if *group >= r.captures_len() { return Err(format!("{}: missing regex capture group", n.id)); }
            }
            Operation::LocateText {pattern,..} => {if pattern.len()>1024{return Err("text-locator pattern too long".into());}Regex::new(pattern).map_err(|e|e.to_string())?;}
            Operation::Confirm { frames, tolerance, max_age_ms, max_delta } if *frames == 0 || *frames > 60 || !tolerance.is_finite() || *tolerance < 0.0 || *max_age_ms == 0 || max_delta.is_some_and(|v| !v.is_finite() || v < 0.0) => return Err(format!("{}: invalid confirmation settings", n.id)),
            Operation::Parse { min, max, scale, .. } if !scale.is_finite() || min.is_some_and(|v| !v.is_finite()) || max.is_some_and(|v| !v.is_finite()) || min.zip(*max).is_some_and(|(a,b)| a > b) => return Err(format!("{}: invalid parser bounds", n.id)),
            Operation::Select { path } if !path.is_empty() && !path.starts_with('/') => return Err(format!("{}: select path must be a JSON pointer", n.id)),
            Operation::Classify { library, .. } if !resources.libraries.contains_key(library) => return Err(format!("{}: unknown library {library}", n.id)),
            Operation::Match { templates, .. } if templates.is_empty() || templates.iter().any(|t| !template_ids.contains(t.as_str())) => return Err(format!("{}: unknown/empty template set", n.id)),
            Operation::Locate { template, scales, step, .. } if !template_ids.contains(template.as_str()) || scales.is_empty() || scales.len() > 8 || scales.iter().any(|v| !v.is_finite() || !(0.1..=8.0).contains(v)) || !(2..=128).contains(step) => return Err(format!("{}: invalid template search", n.id)),
            Operation::Custom { name, .. } if !resources.stages.contains_key(name) => return Err(format!("{}: unknown custom stage {name}", n.id)),
            Operation::Coalesce if n.inputs.len() < 2 => return Err(format!("{}: coalesce needs at least two inputs", n.id)),
            Operation::Status { one_of } if one_of.is_empty() || one_of.len() > 5 => return Err(format!("{}: status probe needs 1–5 statuses", n.id)),
            _ => {}
        }
        let scores: Vec<f32> = match &n.op {
            Operation::Gate { min_score, .. } => min_score.iter().copied().collect(),
            Operation::Classify { min_score, min_margin, .. } | Operation::Match { min_score, min_margin, .. } => vec![*min_score, *min_margin],
            Operation::Locate { min_score, .. } => vec![*min_score], _ => vec![],
        };
        if scores.iter().any(|s| !s.is_finite() || !(-1.0..=2.0).contains(s)) { return Err(format!("{}: invalid score threshold", n.id)); }
        if matches!(n.op, Operation::Extract {..} | Operation::LocateText {..} | Operation::Parse {..} | Operation::Contains {..} | Operation::Select {..} | Operation::Gate {..} | Operation::Confirm {..} | Operation::Status {..}) && n.inputs.len() != 1 {
            return Err(format!("{}: value operation requires exactly one input", n.id));
        }
    }
    for (field, source) in &profile.outputs { if field.is_empty() || field.len() > 120 || ["__proto__", "prototype", "constructor"].contains(&field.as_str()) || !ids.contains_key(source.as_str()) { return Err(format!("invalid output {field}: {source}")); } }
    // Topological sort includes conditional dependencies; JSON declaration order is irrelevant.
    fn visit(i: usize, profile: &Profile, ids: &HashMap<&str,usize>, marks: &mut [u8], order: &mut Vec<usize>) -> Result<(), String> {
        if marks[i] == 2 { return Ok(()); } if marks[i] == 1 { return Err(format!("cycle at {}", profile.nodes[i].id)); }
        marks[i] = 1;
        for input in profile.nodes[i].inputs.iter().chain(profile.nodes[i].when.iter()) { if input != "$frame" { visit(ids[input.as_str()], profile, ids, marks, order)?; } }
        marks[i] = 2; order.push(i); Ok(())
    }
    let mut marks = vec![0; profile.nodes.len()]; let mut order = vec![];
    for i in 0..profile.nodes.len() { visit(i, profile, &ids, &mut marks, &mut order)?; }
    Ok(order)
}
