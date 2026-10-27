//! Shared, local OCR for arbitrary image regions. Models are shared between adapters.
use image::{imageops::FilterType, DynamicImage};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use std::path::Path;

/// Optional host-provided batched line recognizer (see [`set_line_hook`]).
pub type LineHook = Box<dyn Fn(&[DynamicImage]) -> Option<Vec<String>>>;
thread_local! { static LINE_HOOK: std::cell::RefCell<Option<LineHook>> = const { std::cell::RefCell::new(None) }; }
/// Register (or clear) a parallel line recognizer for this thread. Every `read_lines` call
/// tries it first; results must be identical to local recognition (same model, same crops).
pub fn set_line_hook(hook: Option<LineHook>) { LINE_HOOK.with(|h| *h.borrow_mut() = hook); }

#[derive(Clone)]
pub struct Reader {
    engine: std::sync::Arc<OcrEngine>,
}

impl Reader {
    pub fn load(model_dir: &Path) -> Result<Reader, String> {
        let det = rten::Model::load_file(model_dir.join("text-detection.rten")).map_err(|e| e.to_string())?;
        let rec = rten::Model::load_file(model_dir.join("text-rec.rten")).map_err(|e| e.to_string())?;
        Self::from_models(det, rec)
    }

    /// Browser/embedded entry point: models are downloaded bytes, not file paths.
    pub fn from_bytes(detection: Vec<u8>, recognition: Vec<u8>) -> Result<Reader, String> {
        let det = rten::Model::load(detection).map_err(|e| e.to_string())?;
        let rec = rten::Model::load(recognition).map_err(|e| e.to_string())?;
        Self::from_models(det, rec)
    }
    /// Lightweight monitor: line recognition only, without a second detection model.
    pub fn recognition_only(recognition:Vec<u8>)->Result<Reader,String>{
        let rec=rten::Model::load(recognition).map_err(|e|e.to_string())?;
        let engine=OcrEngine::new(OcrEngineParams{recognition_model:Some(rec),..Default::default()}).map_err(|e|e.to_string())?;
        Ok(Reader{engine:std::sync::Arc::new(engine)})
    }

    fn from_models(det: rten::Model, rec: rten::Model) -> Result<Reader, String> {
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(det),
            recognition_model: Some(rec),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        Ok(Reader { engine: std::sync::Arc::new(engine) })
    }

    /// OCR an image region (upscaled 3x for small HUD text). Returns the text found.
    pub fn read(&self, img: &DynamicImage) -> Result<String, String> { self.read_scaled(img, 3) }

    /// Text plus approximate glyph area, for UI regions containing several font sizes.
    pub fn read_words(&self, img: &DynamicImage) -> Result<Vec<(String, u32)>, String> {
        use ocrs::TextItem;
        let big = img.resize(img.width() * 3, img.height() * 3, FilterType::CatmullRom).to_rgb8();
        let source = ImageSource::from_bytes(big.as_raw(), big.dimensions()).map_err(|e| e.to_string())?;
        let input = self.engine.prepare_input(source).map_err(|e| e.to_string())?;
        let words = self.engine.detect_words(&input).map_err(|e| e.to_string())?;
        let lines = self.engine.find_text_lines(&input, &words);
        let text = self.engine.recognize_text(&input, &lines).map_err(|e| e.to_string())?;
        Ok(text.into_iter().flatten().flat_map(|line| line.words().map(|w| {
            let bounds = w.bounding_rect(); (w.to_string(), (bounds.width() * bounds.height()).max(0) as u32)
        }).collect::<Vec<_>>()).collect())
    }
    pub fn read_word_regions(&self,img:&DynamicImage)->Result<Vec<crate::RegionObservation>,String>{
        use ocrs::TextItem;
        let rgb=img.resize_exact(img.width()*3,img.height()*3,FilterType::CatmullRom).to_rgb8();
        let input=self.engine.prepare_input(ImageSource::from_bytes(rgb.as_raw(),rgb.dimensions()).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let words=self.engine.detect_words(&input).map_err(|e|e.to_string())?;let lines=self.engine.find_text_lines(&input,&words);
        let text=self.engine.recognize_text(&input,&lines).map_err(|e|e.to_string())?;
        let mut result=vec![];
        for line in text.into_iter().flatten(){for word in line.words(){let b=word.bounding_rect();let x=(b.left().max(0) as u32/3).min(img.width()-1);let y=(b.top().max(0) as u32/3).min(img.height()-1);
            result.push(crate::RegionObservation{id:format!("word.{}",result.len()),label:word.to_string(),origin:crate::RegionOrigin::Detected,score:None,value_path:None,
                bounds:crate::Bounds{x,y,w:((b.width().max(1) as u32+2)/3).min(img.width()-x),h:((b.height().max(1) as u32+2)/3).min(img.height()-y)}});
        }}Ok(result)
    }
    pub fn read_spans(&self,img:&DynamicImage,scale:u32)->Result<Vec<crate::RegionObservation>,String>{
        use ocrs::TextItem;
        let rgb=img.resize_exact(img.width()*scale,img.height()*scale,FilterType::CatmullRom).to_rgb8();
        let source=ImageSource::from_bytes(rgb.as_raw(),rgb.dimensions()).map_err(|e|e.to_string())?;
        let input=self.engine.prepare_input(source).map_err(|e|e.to_string())?;
        let words=self.engine.detect_words(&input).map_err(|e|e.to_string())?;let lines=self.engine.find_text_lines(&input,&words);
        let text=self.engine.recognize_text(&input,&lines).map_err(|e|e.to_string())?;
        Ok(text.into_iter().flatten().enumerate().map(|(i,t)|{
            let b=t.bounding_rect();let x=(b.left().max(0) as u32/scale).min(img.width()-1);let y=(b.top().max(0) as u32/scale).min(img.height()-1);
            crate::RegionObservation{id:format!("text.{i}"),label:t.to_string(),origin:crate::RegionOrigin::Detected,score:None,value_path:None,
                bounds:crate::Bounds{x,y,w:((b.width().max(1) as u32+scale-1)/scale).min(img.width()-x),h:((b.height().max(1) as u32+scale-1)/scale).min(img.height()-y)}}
        }).collect())
    }

    /// Calibrated HUD fields already provide a line rectangle. Avoid re-running the
    /// fixed-size text detector for every tiny crop; recognition resizes the line itself.
    pub fn read_line(&self, img: &DynamicImage) -> Result<String, String> {
        use rten_imageproc::{Rect, RotatedRect};
        let rgb = img.to_rgb8();
        let source = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).map_err(|e| e.to_string())?;
        let input = self.engine.prepare_input(source).map_err(|e| e.to_string())?;
        let rect = RotatedRect::from_rect(Rect::from_tlhw(0.0, 0.0, rgb.height() as f32, rgb.width() as f32));
        let lines = self.engine.recognize_text(&input, &[vec![rect]]).map_err(|e| e.to_string())?;
        Ok(lines.into_iter().flatten().map(|line| line.to_string()).collect::<Vec<_>>().join("\n"))
    }

    /// Recognize many already-located single-line crops in ONE model call.
    /// Crops are stacked on one canvas, so text detection never runs and the
    /// recognizer batches lines internally. Output order matches input order.
    pub fn read_lines(&self, crops: &[DynamicImage]) -> Result<Vec<String>, String> {
        if crops.is_empty() { return Ok(vec![]); }
        // A host may register a parallel recognizer (e.g. a browser worker pool). It must
        // return exactly one line per crop, or `None` to fall back to local recognition.
        if let Some(lines) = LINE_HOOK.with(|h| h.borrow().as_ref().and_then(|f| f(crops))).filter(|l| l.len() == crops.len()) {
            return Ok(lines);
        }
        self.read_lines_local(crops)
    }

    /// Batched recognition on this thread (used directly by parallel recognizer workers).
    pub fn read_lines_local(&self, crops: &[DynamicImage]) -> Result<Vec<String>, String> {
        use rten_imageproc::{Rect, RotatedRect};
        if crops.is_empty() { return Ok(vec![]); }
        const GAP: u32 = 6;
        let scale = |c: &DynamicImage| (40 / c.height().max(1)).clamp(1, 4);
        let width = crops.iter().map(|c| c.width() * scale(c)).max().unwrap_or(1).max(1);
        let height: u32 = crops.iter().map(|c| c.height().max(1) * scale(c) + GAP).sum();
        let mut canvas = image::RgbImage::new(width, height.max(1));
        let mut rects = Vec::with_capacity(crops.len());
        let mut y = 0u32;
        for crop in crops {
            // Small HUD glyphs recognize far better after a smooth upscale to a usable height
            // (measured: a 45 px italic "1492" misread at 1x, correct at 3x). The recognizer
            // normalizes line height itself, so this does not grow inference cost.
            let k = (40 / crop.height().max(1)).clamp(1, 4);
            let rgb = if k > 1 { crop.resize_exact(crop.width() * k, crop.height() * k, FilterType::CatmullRom).to_rgb8() } else { crop.to_rgb8() };
            image::imageops::replace(&mut canvas, &rgb, 0, y as i64);
            rects.push(vec![RotatedRect::from_rect(Rect::from_tlhw(y as f32, 0.0, rgb.height().max(1) as f32, rgb.width().max(1) as f32))]);
            y += rgb.height().max(1) + GAP;
        }
        let source = ImageSource::from_bytes(canvas.as_raw(), canvas.dimensions()).map_err(|e| e.to_string())?;
        let input = self.engine.prepare_input(source).map_err(|e| e.to_string())?;
        let lines = self.engine.recognize_text(&input, &rects).map_err(|e| e.to_string())?;
        Ok(lines.into_iter().map(|l| l.map(|l| l.to_string()).unwrap_or_default()).collect())
    }

    /// Like `read` with an explicit upscale factor (tiny text benefits from more).
    pub fn read_scaled(&self, img: &DynamicImage, k: u32) -> Result<String, String> {
        let big = img.resize(img.width() * k, img.height() * k, FilterType::CatmullRom).to_rgb8();
        let src = ImageSource::from_bytes(big.as_raw(), big.dimensions()).map_err(|e| e.to_string())?;
        let input = self.engine.prepare_input(src).map_err(|e| e.to_string())?;
        self.engine.get_text(&input).map_err(|e| e.to_string())
    }
}
