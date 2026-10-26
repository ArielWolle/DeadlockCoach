use coach_perception::*;
use image::{DynamicImage, Rgb, RgbImage};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use base64::Engine as _;

fn profile(nodes: Value, outputs: Value) -> Profile {
    serde_json::from_value(json!({"schema_version":1,"id":"test","name":"Test game","nodes":nodes,"outputs":outputs})).unwrap()
}
fn image() -> DynamicImage { DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 32, Rgb([0,200,0]))) }

#[test]
fn graph_sorts_fanout_and_confirms_without_holding_stale_values() {
    let p = profile(json!([
        {"id":"stable","inputs":["percent"],"op":{"type":"confirm","frames":2,"tolerance":1,"max_age_ms":1000}},
        {"id":"percent","inputs":["green"],"op":{"type":"parse","parser":"number","scale":100}},
        {"id":"green","op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}}
    ]), json!({"health":"stable"}));
    let mut pipe = Pipeline::new(p, Resources::default()).unwrap();
    assert_eq!(pipe.run(&image(), None, 100).fields["health"].status, Status::Pending);
    assert_eq!(pipe.run(&image(), None, 200).fields["health"].value, Some(json!(100.0)));
    let report = pipe.run(&image(), None, 3000); assert_eq!(report.fields["health"].status, Status::Pending); assert!(report.fields["health"].value.is_none());
    assert_eq!(report.stages.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["green", "percent", "stable"]);
}

#[test]
fn rejects_cycles_unknown_inputs_and_bad_imports_atomically() {
    let good = profile(json!([{"id":"green","op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}}]), json!({"value":"green"}));
    let mut pipe = Pipeline::new(good.clone(), Resources::default()).unwrap();
    let mut bad = good.clone(); bad.nodes[0].inputs = vec!["green".into()]; assert!(pipe.set_profile(bad).unwrap_err().contains("cycle"));
    let mut bad = good; bad.nodes[0].inputs = vec!["absent".into()]; assert!(pipe.set_profile(bad).is_err());
    assert_eq!(pipe.run(&image(), None, 0).fields["value"].value, Some(json!(1.0)));
}

struct MockText { calls: Arc<AtomicUsize>, score: Option<f32> }
impl TextBackend for MockText {
    fn recognize(&self, _: &DynamicImage, _: OcrMode, _: u32) -> Result<TextResult, String> {
        self.calls.fetch_add(1, Ordering::SeqCst); Ok(TextResult { text: "HP: 42 / 100".into(), score: self.score,regions:vec![] })
    }
}

#[test]
fn ocr_backend_is_replaceable_and_shared_branches_do_not_repeat_inference() {
    let calls = Arc::new(AtomicUsize::new(0));
    let resources = Resources { ocr: Some(Arc::new(MockText { calls: calls.clone(), score: Some(0.91) })), ..Default::default() };
    let p = profile(json!([
        {"id":"a","op":{"type":"ocr","mode":"line","upscale":1}},
        {"id":"b","op":{"type":"ocr","mode":"line","upscale":1}},
        {"id":"hp","inputs":["a"],"op":{"type":"extract","pattern":"HP: (\\d+)","group":1}},
        {"id":"n","inputs":["hp"],"op":{"type":"parse","parser":"number","min":0,"max":100}},
        {"id":"check","inputs":["n"],"op":{"type":"gate","min_score":0.9}}
    ]), json!({"hp":"check","raw":"b"}));
    let mut pipe = Pipeline::new(p, resources).unwrap(); let report = pipe.run(&image(), None, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1); assert_eq!(report.fields["hp"].value, Some(json!(42.0)));
    assert_eq!(report.fields["hp"].score, Some(0.91));
}

#[test]
fn missing_scores_do_not_turn_into_fabricated_ocr_confidence() {
    let resources = Resources { ocr: Some(Arc::new(MockText { calls: Arc::new(AtomicUsize::new(0)), score: None })), ..Default::default() };
    let p = profile(json!([
        {"id":"text","op":{"type":"ocr","mode":"line","upscale":1}},
        {"id":"check","inputs":["text"],"op":{"type":"gate","min_score":0.8}}
    ]), json!({"value":"check"}));
    assert_eq!(Pipeline::new(p, resources).unwrap().run(&image(), None, 0).fields["value"].status, Status::Missing);
}

#[test]
fn parser_is_locale_explicit_and_clock_requires_a_separator() {
    assert_eq!(parse_value(&json!("$1,732 souls"), Parser::Number, false), Some(json!(1732.0)));
    assert_eq!(parse_value(&json!("1.732,5"), Parser::Number, true), Some(json!(1732.5)));
    assert_eq!(parse_value(&json!("2.5k"), Parser::Number, false), Some(json!(2500.0)));
    assert_eq!(parse_value(&json!("02:13"), Parser::Clock, false), Some(json!(133)));
    assert_eq!(parse_value(&json!("1:02:13"), Parser::Clock, false), Some(json!(3733)));
    assert_eq!(parse_value(&json!("0213"), Parser::Clock, false), None);
    assert_eq!(parse_value(&json!("1:02:99"), Parser::Clock, false), None);
}

#[test]
fn preprocessing_preserves_original_frame_coordinates_for_downstream_crops() {
    let p = profile(json!([
        {"id":"roi","op":{"type":"crop","region":{"x":0.25,"y":0.25,"w":0.5,"h":0.5}}},
        {"id":"zoom","inputs":["roi"],"op":{"type":"preprocess","scale":2}},
        {"id":"half","inputs":["zoom"],"op":{"type":"crop","region":{"x":0.5,"y":0,"w":0.5,"h":1}}},
        {"id":"green","inputs":["half"],"op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}}
    ]), json!({"value":"green"}));
    let report = Pipeline::new(p, Resources::default()).unwrap().run(&image(), None, 0);
    assert_eq!(report.fields["value"].bounds, Some(Bounds { x:32,y:8,w:16,h:16 }));
}

#[test]
fn template_search_locates_an_anchor_and_can_read_a_region_next_to_it() {
    let icon = RgbImage::from_fn(8,8,|x,y| { let v = ((x*37+y*71+x*y*13)%251) as u8; Rgb([v,v,v]) });
    let mut frame = RgbImage::from_pixel(64,48,Rgb([20,20,20])); image::imageops::overlay(&mut frame,&icon,17,9);
    for y in 9..17 { for x in 27..35 { frame.put_pixel(x,y,Rgb([0,200,0])); } }
    let mut png = std::io::Cursor::new(vec![]); DynamicImage::ImageRgb8(icon).write_to(&mut png,image::ImageFormat::Png).unwrap();
    let mut p = profile(json!([
        {"id":"anchor","op":{"type":"locate","template":"button","step":2,"scales":[1],"min_score":0.98}},
        {"id":"nearby","inputs":["$frame","anchor"],"op":{"type":"crop","anchor_input":true,"region":{"space":"pixels","x":10,"y":0,"w":8,"h":8}}},
        {"id":"green","inputs":["nearby"],"op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}}
    ]), json!({"anchor":"anchor","value":"green"}));
    p.templates.push(EmbeddedTemplate { id:"button".into(), png_base64:base64::engine::general_purpose::STANDARD.encode(png.into_inner()) });
    let report = Pipeline::new(p, Resources::default()).unwrap().run(&DynamicImage::ImageRgb8(frame),None,0);
    assert_eq!(report.fields["anchor"].bounds, Some(Bounds { x:17,y:9,w:8,h:8 }));
    assert_eq!(report.fields["value"].value,Some(json!(1.0)));
    assert_eq!(report.fields["value"].bounds,Some(Bounds { x:27,y:9,w:8,h:8 }));
}

#[test]
fn conditional_missing_stages_clear_temporal_history() {
    let p = profile(json!([
        {"id":"green","op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}},
        {"id":"check","inputs":["green"],"op":{"type":"gate","equals":1.0}},
        {"id":"stable","inputs":["check"],"op":{"type":"confirm","frames":2}}
    ]),json!({"value":"stable"}));
    let mut pipe = Pipeline::new(p,Resources::default()).unwrap();
    pipe.run(&image(),None,0); assert_eq!(pipe.run(&image(),None,100).fields["value"].status,Status::Ok);
    let dark = DynamicImage::ImageRgb8(RgbImage::new(64,32));
    assert_eq!(pipe.run(&dark,None,200).fields["value"].status,Status::Skipped);
    assert_eq!(pipe.run(&image(),None,300).fields["value"].status,Status::Pending);
}

#[test]
fn color_matching_separates_icons_with_the_same_grayscale_shape() {
    let red = DynamicImage::ImageRgb8(RgbImage::from_fn(16,16,|x,y| Rgb([((x*37+y*71+x*y*13)%251) as u8,0,0])));
    let green = DynamicImage::ImageRgb8(RgbImage::from_fn(16,16,|x,y| Rgb([0,((x*37+y*71+x*y*13)%251) as u8,0])));
    let mut p = profile(json!([
        {"id":"rgb","op":{"type":"match","templates":["red","green"],"min_score":0.9,"min_margin":0.1}},
        {"id":"gray","op":{"type":"match","templates":["red","green"],"min_score":0.9,"min_margin":0.1,"color":false}}
    ]),json!({"color":"rgb","shape":"gray"}));
    for (id,img) in [("red",&red),("green",&green)] {
        let mut bytes = std::io::Cursor::new(vec![]); img.write_to(&mut bytes,image::ImageFormat::Png).unwrap();
        p.templates.push(EmbeddedTemplate { id:id.into(), png_base64:base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()) });
    }
    let report = Pipeline::new(p,Resources::default()).unwrap().run(&red,None,0);
    assert_eq!(report.fields["color"].value,Some(json!("red")));
    assert_eq!(report.fields["shape"].status,Status::Missing);
}

struct SceneSequence { index: usize }
impl Stage for SceneSequence {
    fn run(&mut self, _: &[Packet], _: &FrameContext<'_>, _: &Value) -> Result<Packet,String> {
        let text = ["PLAY 42","PLAY 42","MENU 42","PLAY 42"][self.index.min(3)]; self.index += 1;
        Ok(Packet { value:Some(json!(text)), ..Default::default() })
    }
    fn reset(&mut self) { self.index = 0; }
}

#[test]
fn conditional_branch_dependencies_are_sorted_and_scene_changes_expire_readings() {
    let p = profile(json!([
        {"id":"stable","inputs":["number"],"op":{"type":"confirm","frames":2}},
        {"id":"number","inputs":["text"],"when":"playing","op":{"type":"parse","parser":"number"}},
        {"id":"playing","inputs":["text"],"op":{"type":"contains","text":"PLAY"}},
        {"id":"text","op":{"type":"custom","name":"test.scene"}}
    ]),json!({"value":"stable"}));
    let mut resources = Resources::default(); resources.stages.insert("test.scene".into(),Box::new(SceneSequence { index:0 }));
    let mut pipe = Pipeline::new(p,resources).unwrap();
    assert_eq!(pipe.run(&image(),None,0).fields["value"].status,Status::Pending);
    assert_eq!(pipe.run(&image(),None,100).fields["value"].value,Some(json!(42.0)));
    assert_eq!(pipe.run(&image(),None,200).fields["value"].status,Status::Skipped);
    assert_eq!(pipe.run(&image(),None,300).fields["value"].status,Status::Pending);
}

#[test]
fn out_of_bounds_detector_errors_can_feed_an_explicit_fallback() {
    let p = profile(json!([
        {"id":"bad","op":{"type":"crop","region":{"x":0.9,"y":0.9,"w":0.5,"h":0.5}}},
        {"id":"fallback","op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}},
        {"id":"value","inputs":["bad","fallback"],"op":{"type":"coalesce"}}
    ]),json!({"value":"value"}));
    let report = Pipeline::new(p,Resources::default()).unwrap().run(&image(),None,0);
    assert_eq!(report.stages[0].status,Status::Error); assert!(report.stages[0].error.is_some());
    assert_eq!(report.fields["value"].value,Some(json!(1.0)));
}

#[test]
fn reports_identify_the_exact_profile_revision() {
    let p = profile(json!([{"id":"green","op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}}]),json!({"value":"green"}));
    let mut pipe = Pipeline::new(p.clone(),Resources::default()).unwrap();
    let a = pipe.run(&image(),None,0).profile_revision; assert_eq!(a.len(),64);
    pipe.set_profile(p.clone()).unwrap(); assert_eq!(pipe.run(&image(),None,1).profile_revision,a);
    let mut changed = p; changed.outputs.insert("another_name".into(),"green".into()); pipe.set_profile(changed).unwrap();
    assert_ne!(pipe.run(&image(),None,2).profile_revision,a);
}

struct FallbackText { line_good: bool, detects: Arc<AtomicUsize> }
impl TextBackend for FallbackText {
    fn recognize(&self, _: &DynamicImage, mode: OcrMode, _: u32) -> Result<TextResult,String> {
        if matches!(mode,OcrMode::Detect) { self.detects.fetch_add(1,Ordering::SeqCst); }
        Ok(TextResult { text:if self.line_good || matches!(mode,OcrMode::Detect) { "5:13".into() } else { "unreadable".into() }, score:None,regions:vec![] })
    }
}

#[test]
fn status_gated_fallback_avoids_expensive_ocr_when_the_primary_strategy_succeeds() {
    let p = profile(json!([
        {"id":"line","op":{"type":"ocr","mode":"line","upscale":1}},
        {"id":"primary","inputs":["line"],"op":{"type":"parse","parser":"clock"}},
        {"id":"need_fallback","inputs":["primary"],"op":{"type":"status","one_of":["missing","error"]}},
        {"id":"detect","when":"need_fallback","op":{"type":"ocr","mode":"detect","upscale":1}},
        {"id":"secondary","inputs":["detect"],"op":{"type":"parse","parser":"clock"}},
        {"id":"clock","inputs":["primary","secondary"],"op":{"type":"coalesce"}}
    ]),json!({"clock":"clock"}));
    for good in [false,true] {
        let detects = Arc::new(AtomicUsize::new(0));
        let resources = Resources { ocr:Some(Arc::new(FallbackText { line_good:good, detects:detects.clone() })), ..Default::default() };
        let report = Pipeline::new(p.clone(),resources).unwrap().run(&image(),None,0);
        assert_eq!(report.fields["clock"].value,Some(json!(313.0)));
        assert_eq!(detects.load(Ordering::SeqCst),if good {0} else {1});
    }
}

#[test]
fn fractional_coordinate_rounding_does_not_extend_past_the_original_image() {
    let p = profile(json!([
        {"id":"zoom","op":{"type":"preprocess","scale":2}},
        {"id":"right","inputs":["zoom"],"op":{"type":"crop","region":{"x":0.5,"y":0,"w":0.5,"h":1}}},
        {"id":"read","inputs":["right"],"op":{"type":"color_ratio","rgb":[0,200,0],"tolerance":0}}
    ]),json!({"value":"read"}));
    let frame = DynamicImage::ImageRgb8(RgbImage::from_pixel(3,3,Rgb([0,200,0])));
    let report = Pipeline::new(p,Resources::default()).unwrap().run(&frame,None,0);
    assert_eq!(report.fields["value"].bounds,Some(Bounds { x:2,y:0,w:1,h:3 }));
}

struct BoxText;
impl TextBackend for BoxText{
    fn recognize(&self,_:&DynamicImage,_:OcrMode,_:u32)->Result<TextResult,String>{Ok(TextResult{text:"1732 SOULS".into(),score:None,
        regions:vec![RegionObservation{id:"currency".into(),label:"1732 SOULS".into(),bounds:Bounds{x:10,y:5,w:30,h:10},origin:RegionOrigin::Detected,score:None,value_path:None}]})}
}
#[test]
fn text_localization_produces_its_own_box_instead_of_relabelling_the_search_region(){
    let p=profile(json!([
        {"id":"ocr","op":{"type":"ocr","upscale":1}},
        {"id":"located","inputs":["ocr"],"op":{"type":"locate_text","pattern":"SOULS"}},
        {"id":"value","inputs":["located"],"op":{"type":"parse","parser":"number"}}
    ]),json!({"currency":"value"}));
    let r=Pipeline::new(p,Resources{ocr:Some(Arc::new(BoxText)),..Default::default()}).unwrap().run(&image(),None,0);
    assert_eq!(r.fields["currency"].bounds,Some(Bounds{x:10,y:5,w:30,h:10}));assert_eq!(r.fields["currency"].bounds_origin,RegionOrigin::Detected);
    assert_eq!(r.fields["currency"].value,Some(json!(1732.0)));
}

struct WidgetDetector;
impl Stage for WidgetDetector{fn run(&mut self,_:&[Packet],_:&FrameContext<'_>,_:&Value)->Result<Packet,String>{
    Ok(Packet{value:Some(json!({"balance":42})),regions:vec![RegionObservation{id:"currency".into(),label:"Currency".into(),bounds:Bounds{x:4,y:5,w:20,h:10},origin:RegionOrigin::Detected,score:Some(0.9),value_path:Some("/balance".into())}],..Default::default()})
}}
#[test]
fn a_new_hud_plugin_can_bind_detected_boxes_to_named_values_without_ui_code(){
    let p=profile(json!([{ "id":"gui","op":{"type":"custom","name":"another.game"}},{"id":"balance","inputs":["gui"],"op":{"type":"select","path":"/balance"}}]),json!({"wallet":"balance"}));
    let mut resources=Resources::default();resources.stages.insert("another.game".into(),Box::new(WidgetDetector));
    let r=Pipeline::new(p,resources).unwrap().run(&image(),None,0);assert_eq!(r.fields["wallet"].value,Some(json!(42)));
    assert_eq!(r.fields["wallet"].bounds,Some(Bounds{x:4,y:5,w:20,h:10}));assert_eq!(r.fields["wallet"].bounds_origin,RegionOrigin::Detected);
}
