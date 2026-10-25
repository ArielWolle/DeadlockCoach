//! Screen capture for the Agent. Screenshots only: never touches game memory.
use coach_core::FrameMeta;
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage};
use std::time::{SystemTime, UNIX_EPOCH};
use xcap::{Monitor, Window};

pub struct Frame {
    pub meta: FrameMeta,
    pub jpeg: Vec<u8>,
    /// "window" if the Deadlock window was captured, "monitor" for fallback.
    pub source: &'static str,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn find_deadlock_window() -> Option<Window> {
    Window::all().ok()?.into_iter().find(|w| {
        !w.is_minimized().unwrap_or(true)
            && (w.title().unwrap_or_default().eq_ignore_ascii_case("Deadlock")
                || w.app_name().unwrap_or_default().to_ascii_lowercase().contains("deadlock"))
    })
}

/// Capture one frame. `max_width` downsizes (keeps aspect) to limit bandwidth;
/// meta reports the ORIGINAL size so the Brain can auto-detect resolution.
pub fn capture_frame(max_width: u32, quality: u8) -> Result<Frame, String> {
    let (rgba, source, scale) = if let Some(w) = find_deadlock_window() {
        let scale = w.current_monitor().ok().and_then(|m| m.scale_factor().ok()).unwrap_or(1.0);
        (w.capture_image().map_err(|e| e.to_string())?, "window", scale)
    } else {
        let m = Monitor::all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .ok_or("no monitor found")?;
        let scale = m.scale_factor().unwrap_or(1.0);
        (m.capture_image().map_err(|e| e.to_string())?, "monitor", scale)
    };
    let (w, h) = (rgba.width(), rgba.height());
    let mut img = DynamicImage::ImageRgba8(rgba);
    if w > max_width {
        img = img.resize(max_width, u32::MAX, FilterType::Triangle);
    }
    let rgb = img.to_rgb8();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, quality)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(Frame {
        meta: FrameMeta { width: w, height: h, dpi_scale: scale, timestamp_ms: now_ms() },
        jpeg,
        source,
    })
}
