use coach_vision::{scan::Scanner,ocr::Reader,quick::{Context,Monitor}};
use coach_perception::Bounds;
fn root()->std::path::PathBuf{std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")}
fn fixture(name:&str)->image::DynamicImage{image::open(root().join("samples/frames").join(name)).unwrap()}
/// Seed the fast monitor from full scans. Team banners rotate (3 of 12 per frame) and each
/// needs a repeat to count as observed, so eight frames cover every slot twice.
fn seeded()->(Monitor,Context,image::DynamicImage){
    let frame=fixture("00059_1792966931596.jpg");let mut scanner=Scanner::load(&root()).unwrap();
    let mut snapshot=scanner.scan_image(&frame,None,0);
    for t in 1..8u64{snapshot=scanner.scan_image(&frame,None,t*100);}
    let hp=scanner.regions().into_iter().find(|r|r.id=="hp").unwrap().bounds;
    let reader=Reader::recognition_only(std::fs::read(root().join("data/models/text-rec.rten")).unwrap()).unwrap();
    let ts=700;snapshot.game_clock_secs=Some(313);
    (Monitor::new(reader),Context{snapshot,timestamp_ms:ts,frame_size:[1600,900],game:Bounds{x:0,y:0,w:1600,h:900},hp,map:Bounds{x:1255,y:525,w:335,h:350}},frame)
}
#[test]
fn recognition_only_monitor_reads_health_and_twelve_slots_without_full_detection(){
    let(mut monitor,context,frame)=seeded();let start=std::time::Instant::now();
    let first=monitor.scan(&frame,&context,700).expect("fresh gameplay should be readable");
    println!("quick native {:?}",start.elapsed());
    assert_eq!(first.hp,Some(779));assert_eq!(first.max_hp,Some(964));assert_eq!(first.hero_status.len(),12);
    // Team banners come from the full scan's round-robin and pass through unchanged.
    assert_eq!(first.hero_status[6].critical,Some(true));assert_eq!(first.hero_status[9].critical,Some(true));
    assert!(first.hero_status[6].observations>=2);
    // Health bars are refreshed by the fast monitor itself: ally 1 is damaged, the two CRITICAL
    // enemies are near zero, enemy 2 has no bar (not seen recently).
    let hp=|i:usize|first.hero_status[i].health_ratio.unwrap_or(-1.0);
    assert!((hp(1)-0.69).abs()<0.06&&hp(0)>0.95,"{:?}",first.hero_status.iter().map(|s|s.health_ratio).collect::<Vec<_>>());
    assert!(hp(6)<0.05&&hp(9)<0.05);
    assert_eq!(first.hero_status[8].bar_visible,Some(false));assert!(first.hero_status[8].health_ratio.is_none());
    assert_eq!(context.snapshot.hero_status[0].networth,Some(3500));
    let second=monitor.scan(&frame,&context,700).unwrap();assert!(second.health_observations>=2);
}
#[test]
fn monitor_rejects_death_menu_stale_context_and_resized_sources(){
    let(mut monitor,context,frame)=seeded();
    assert!(monitor.scan(&frame,&context,30701).is_none());
    assert!(monitor.scan(&frame.resize_exact(800,450,image::imageops::FilterType::Triangle),&context,700).is_none());
    assert!(monitor.scan(&fixture("00216_dead-respawn.jpg"),&context,700).is_none());
    assert!(monitor.scan(&fixture("00167_scoreboard-mid.jpg"),&context,700).is_none());
}
#[test]
fn recovery_context_retains_only_current_health_and_clock_after_thirty_seconds(){
    let(mut monitor,mut context,frame)=seeded();context.snapshot.game_clock_secs=Some(283);
    let snapshot=monitor.scan(&frame,&context,30701).expect("fresh clock and health can support conservative monitoring during recovery");
    assert_eq!(snapshot.hp,Some(779));assert_eq!(snapshot.game_clock_secs,Some(313));
    assert!(snapshot.minimap.is_none());assert!(snapshot.hero_status.is_empty());assert!(snapshot.souls.is_none());assert!(snapshot.own_networth.is_none());
    assert!(monitor.scan(&frame,&context,90701).is_none());
}
