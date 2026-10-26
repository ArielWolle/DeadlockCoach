use coach_vision::{scan::Scanner,ocr::Reader,quick::{Context,Monitor}};
use coach_perception::Bounds;
fn root()->std::path::PathBuf{std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")}
fn fixture(name:&str)->image::DynamicImage{image::open(root().join("samples/frames").join(name)).unwrap()}
fn seeded()->(Monitor,Context,image::DynamicImage){
    let frame=fixture("00059_1792966931596.jpg");let mut scanner=Scanner::load(&root()).unwrap();scanner.scan_image(&frame,None,0);
    let snapshot=scanner.scan_image(&frame,None,5000);
    let hp=scanner.regions().into_iter().find(|r|r.id=="hp").unwrap().bounds;
    let reader=Reader::recognition_only(std::fs::read(root().join("data/models/text-rec.rten")).unwrap()).unwrap();
    (Monitor::new(reader),Context{snapshot,timestamp_ms:5000,frame_size:[1600,900],game:Bounds{x:0,y:0,w:1600,h:900},hp,map:Bounds{x:1255,y:525,w:335,h:350}},frame)
}
#[test]
fn recognition_only_monitor_reads_health_and_twelve_slots_without_full_detection(){
    let(mut monitor,context,frame)=seeded();let start=std::time::Instant::now();
    let first=monitor.scan(&frame,&context,5000).expect("fresh gameplay should be readable");
    println!("quick native {:?}",start.elapsed());
    assert_eq!(first.hp,Some(779));assert_eq!(first.max_hp,Some(964));assert_eq!(first.hero_status.len(),12);
    assert_eq!(first.hero_status[6].critical,Some(true));assert_eq!(first.hero_status[9].critical,Some(true));
    assert!(first.hero_status.iter().all(|p|p.health_ratio.is_none()));
    assert_eq!(context.snapshot.hero_status[0].networth,Some(3500));
    let second=monitor.scan(&frame,&context,5000).unwrap();assert!(second.health_observations>=2);assert!(second.hero_status[6].observations>=2);
}
#[test]
fn monitor_rejects_death_menu_stale_context_and_resized_sources(){
    let(mut monitor,context,frame)=seeded();
    assert!(monitor.scan(&frame,&context,35001).is_none());
    assert!(monitor.scan(&frame.resize_exact(800,450,image::imageops::FilterType::Triangle),&context,5000).is_none());
    assert!(monitor.scan(&fixture("00216_dead-respawn.jpg"),&context,5000).is_none());
    assert!(monitor.scan(&fixture("00167_scoreboard-mid.jpg"),&context,5000).is_none());
}
