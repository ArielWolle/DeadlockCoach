use std::{io::Write, process::{Command,Stdio}};
use base64::Engine as _;

#[test]
fn jsonl_cli_reads_frames_and_emits_one_observation_per_line_without_models() {
    let dir = std::env::temp_dir().join(format!("coach-perception-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("frame.png"); image::RgbImage::from_pixel(64,32,image::Rgb([0,200,0])).save(&image).unwrap();
    let profile = dir.join("profile.json");
    std::fs::write(&profile, r#"{"schema_version":1,"id":"test","name":"CLI test","nodes":[{"id":"green","op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}}],"outputs":{"fill":"green"}}"#).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_coach-perceive")).args(["--profile",profile.to_str().unwrap()]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    writeln!(child.stdin.as_mut().unwrap(),"{}",serde_json::json!({"image":image,"timestamp_ms":0})).unwrap();
    let bytes = base64::engine::general_purpose::STANDARD.encode(std::fs::read(&image).unwrap());
    writeln!(child.stdin.as_mut().unwrap(),"{}",serde_json::json!({"image_base64":bytes,"timestamp_ms":5000})).unwrap();
    drop(child.stdin.take()); let output = child.wait_with_output().unwrap();
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap(); let lines:Vec<_> = text.lines().collect(); assert_eq!(lines.len(),2);
    for line in lines { let r:coach_perception::Report = serde_json::from_str(line).unwrap(); assert_eq!(r.fields["fill"].value,Some(serde_json::json!(1.0))); }
    std::fs::remove_dir_all(dir).unwrap();
}
