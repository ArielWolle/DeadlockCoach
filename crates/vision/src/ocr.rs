//! Local OCR (ocrs) for HUD text regions.
use image::{imageops::FilterType, DynamicImage};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use std::path::Path;

pub struct Reader {
    engine: OcrEngine,
}

impl Reader {
    pub fn load(model_dir: &Path) -> Result<Reader, String> {
        let det = rten::Model::load_file(model_dir.join("text-detection.rten")).map_err(|e| e.to_string())?;
        let rec = rten::Model::load_file(model_dir.join("text-rec.rten")).map_err(|e| e.to_string())?;
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(det),
            recognition_model: Some(rec),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        Ok(Reader { engine })
    }

    /// OCR an image region (upscaled 3x for small HUD text). Returns the text found.
    pub fn read(&self, img: &DynamicImage) -> Result<String, String> {
        let big = img.resize(img.width() * 3, img.height() * 3, FilterType::CatmullRom).to_rgb8();
        let src = ImageSource::from_bytes(big.as_raw(), big.dimensions()).map_err(|e| e.to_string())?;
        let input = self.engine.prepare_input(src).map_err(|e| e.to_string())?;
        self.engine.get_text(&input).map_err(|e| e.to_string())
    }
}
