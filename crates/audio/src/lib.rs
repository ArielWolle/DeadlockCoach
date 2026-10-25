//! Audio: WAV playback on the Agent (default output device) and Piper TTS on the Brain.
use std::{io::Cursor, path::PathBuf, process::{Command, Stdio}, io::Write};

/// Names of all output devices.
pub fn list_devices() -> Vec<String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let host = rodio::cpal::default_host();
    let def = host.default_output_device().and_then(|d| d.name().ok()).unwrap_or_default();
    host.output_devices().map(|it| it.filter_map(|d| d.name().ok()).map(|n| if n == def { format!("{n} (default)") } else { n }).collect()).unwrap_or_default()
}

/// Play WAV bytes; device chosen by COACH_AUDIO_DEVICE (name substring) else default.
pub fn play_wav(wav: Vec<u8>) -> Result<(), String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let want = std::env::var("COACH_AUDIO_DEVICE").ok().map(|s| s.to_lowercase());
    let dev = want.and_then(|w| rodio::cpal::default_host().output_devices().ok()?.find(|d| d.name().map(|n| n.to_lowercase().contains(&w)).unwrap_or(false)));
    let (_stream, handle) = match dev {
        Some(d) => rodio::OutputStream::try_from_device(&d),
        None => rodio::OutputStream::try_default(),
    }.map_err(|e| e.to_string())?;
    let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
    let src = rodio::Decoder::new(Cursor::new(wav)).map_err(|e| e.to_string())?;
    sink.append(src);
    sink.sleep_until_end();
    Ok(())
}

pub struct Piper {
    exe: PathBuf,
    model: PathBuf,
}

impl Piper {
    /// Default location: ~/.deadlockcoach/piper/{piper/piper.exe, voice.onnx}
    /// (override with COACH_PIPER_DIR).
    pub fn from_env() -> Result<Piper, String> {
        let dir = std::env::var("COACH_PIPER_DIR").map(PathBuf::from).unwrap_or_else(|_| {
            PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(".deadlockcoach").join("piper")
        });
        let (exe, model) = (dir.join("piper").join("piper.exe"), dir.join("voice.onnx"));
        if !exe.exists() || !model.exists() {
            return Err(format!("Piper not found in {}", dir.display()));
        }
        Ok(Piper { exe, model })
    }

    /// Synthesize text to WAV bytes.
    pub fn synth(&self, text: &str) -> Result<Vec<u8>, String> {
        let out = std::env::temp_dir().join(format!("coach-tts-{}.wav", std::process::id()));
        let mut child = Command::new(&self.exe)
            .arg("-m").arg(&self.model).arg("-f").arg(&out)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().map_err(|e| e.to_string())?;
        child.stdin.take().ok_or("no stdin")?
            .write_all(format!("{}\n", text.replace('\n', " ")).as_bytes()).map_err(|e| e.to_string())?;
        if !child.wait().map_err(|e| e.to_string())?.success() {
            return Err("piper failed".into());
        }
        let wav = std::fs::read(&out).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&out);
        Ok(wav)
    }
}
