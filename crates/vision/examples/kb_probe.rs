//! Killing-blow tag pixels per death-panel row of a 1920x1080 frame. Usage: kb_probe FRAME...
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for p in std::env::args().skip(1) {
        let src = image::open(&p).unwrap();
        let canon = src.resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        let ctx = coach_perception::Bounds { x: 0, y: 0, w: src.width(), h: src.height() };
        let view = coach_vision::hudtext::View { canon: &canon, source: Some((&src, ctx)) };
        let size = coach_vision::death::panel_size(&view);
        let panel = view.region(coach_vision::death::PANEL_CANONICAL, size, size);
        let (rows, crops) = coach_vision::death::panel_crops(&panel);
        let lines = reader.read_lines(&crops).unwrap_or_default();
        println!("{p}: {} rows", rows.len());
        for k in 0..rows.len() {
            println!("  row {k}: px={} name={:?} wide={:?} hits={:?} dmg={:?}", coach_vision::death::killing_blow_px(&panel, k),
                lines.get(4 * k), lines.get(4 * k + 1), lines.get(4 * k + 2), lines.get(4 * k + 3));
        }
        let r = coach_vision::death::parse_panel(&panel, &rows, &lines, &[]);
        println!("  total line {:?}", lines.get(4 * rows.len()));
        println!("  parsed {:?} total {:?} tags {} complete {}", r.rows.iter().map(|r| (r.index, r.source.clone(), r.damage, r.killing_blow)).collect::<Vec<_>>(), r.total, r.tags, r.complete());
        println!("  tab {}", coach_vision::death::panel_tab(&canon));
    }
}
