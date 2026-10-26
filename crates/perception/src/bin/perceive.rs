//! JSONL stdin -> JSONL observations. Logs/errors go to stderr, never mixed with reports.
use coach_perception::{Pipeline, Resources, profile_from_json};
use serde::Deserialize;
use base64::Engine as _;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Input { image: Option<String>, image_base64: Option<String>, timestamp_ms: u64 }

fn decode<R: std::io::BufRead + std::io::Seek>(mut reader: image::ImageReader<R>) -> Result<image::DynamicImage, String> {
    let mut limits = image::Limits::default(); limits.max_image_width = Some(8192); limits.max_image_height = Some(8192); limits.max_alloc = Some(128 * 1024 * 1024); reader.limits(limits);
    reader.decode().map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1); let mut profile = None; let mut models = None;
    while let Some(arg) = args.next() { match arg.as_str() {
        "--profile" => profile = args.next(), "--models" => models = args.next(),
        "--help" | "-h" => { eprintln!("coach-perceive --profile FILE [--models DIR]\nstdin: {{\"image\":\"frame.png\",\"timestamp_ms\":5000}} or image_base64 (PNG/JPEG) per line\nstdout: one perception report per frame; stderr: errors"); return Ok(()); }
        _ => return Err(format!("unknown argument {arg}")),
    }}
    let path = profile.ok_or("--profile is required")?;
    let profile = profile_from_json(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)?;
    #[allow(unused_mut)] let mut resources = Resources::default();
    if let Some(path) = models {
        #[cfg(feature = "ocrs-backend")]
        { resources.ocr = Some(std::sync::Arc::new(coach_perception::ocr::Reader::load(std::path::Path::new(&path))?)); }
        #[cfg(not(feature = "ocrs-backend"))]
        { let _ = path; return Err("this binary was built without the ocrs-backend feature".into()); }
    }
    let mut pipeline = Pipeline::new(profile, resources)?;
    let stdin = io::stdin(); let mut stdout = io::BufWriter::new(io::stdout().lock());
    for (index, line) in stdin.lock().lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?; if line.trim().is_empty() { continue; }
        if line.len() > 12 * 1024 * 1024 { return Err(format!("line {} exceeds input limit", index + 1)); }
        let input: Input = serde_json::from_str(&line).map_err(|e| format!("line {}: {e}", index + 1))?;
        let image = match (input.image, input.image_base64) {
            (Some(path), None) => decode(image::ImageReader::open(&path).map_err(|e| format!("{path}: {e}"))?)?,
            (None, Some(bytes)) => {
                let bytes = base64::engine::general_purpose::STANDARD.decode(bytes).map_err(|e| e.to_string())?;
                if bytes.len() > 8 * 1024 * 1024 { return Err("encoded image exceeds 8 MB".into()); }
                decode(image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?)?
            }
            _ => return Err(format!("line {} needs exactly one of image or image_base64", index + 1)),
        };
        let report = pipeline.run(&image, None, input.timestamp_ms);
        serde_json::to_writer(&mut stdout, &report).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?; stdout.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() { if let Err(error) = run() { eprintln!("{error}"); std::process::exit(2); } }
