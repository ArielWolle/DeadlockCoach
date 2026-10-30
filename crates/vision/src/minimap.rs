//! Minimap reader. Works on the cropped minimap image at any resolution: all sizes are
//! expressed at the canonical 335x350 crop (1600x900 frame) and multiplied by `scale`.
use coach_core::{MinimapState, Pt, UrnState};
use image::{DynamicImage, RgbImage};

/// Minimap rectangle as fractions of the whole frame (measured on a 1600x900 frame).
pub const FRAC_X: f32 = 1255.0 / 1600.0;
pub const FRAC_Y: f32 = 525.0 / 900.0;
pub const FRAC_W: f32 = 335.0 / 1600.0;
pub const FRAC_H: f32 = 350.0 / 900.0;
/// Canonical crop size.
pub const CW: f32 = 335.0;
pub const CH: f32 = 350.0;

/// Crop the minimap out of a full frame.
pub fn crop_from_frame(img: &DynamicImage) -> RgbImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    img.crop_imm((FRAC_X * w) as u32, (FRAC_Y * h) as u32, (FRAC_W * w) as u32, (FRAC_H * h) as u32).to_rgb8()
}

pub fn hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d == 0.0 { 0.0 }
        else if mx == r { 60.0 * (((g - b) / d) % 6.0) }
        else if mx == g { 60.0 * ((b - r) / d + 2.0) }
        else { 60.0 * ((r - g) / d + 4.0) };
    (if h < 0.0 { h + 360.0 } else { h }, if mx == 0.0 { 0.0 } else { d / mx }, mx)
}

#[derive(Debug, Clone, Copy)]
pub struct Blob {
    pub area: u32,
    pub min_x: u32,
    pub max_x: u32,
    pub min_y: u32,
    pub max_y: u32,
    pub cx: f32,
    pub cy: f32,
}

impl Blob {
    pub fn w(&self) -> u32 { self.max_x - self.min_x + 1 }
    pub fn h(&self) -> u32 { self.max_y - self.min_y + 1 }
    pub fn fill(&self) -> f32 { self.area as f32 / (self.w() * self.h()) as f32 }
}

/// 8-connected components of a boolean mask.
pub fn components(mask: &[bool], w: u32, h: u32) -> Vec<Blob> {
    let mut seen = vec![false; mask.len()];
    let mut out = vec![];
    let mut stack = vec![];
    for sy in 0..h {
        for sx in 0..w {
            let si = (sy * w + sx) as usize;
            if !mask[si] || seen[si] { continue; }
            seen[si] = true;
            stack.push((sx, sy));
            let (mut area, mut sxs, mut sys) = (0u32, 0f64, 0f64);
            let (mut minx, mut maxx, mut miny, mut maxy) = (sx, sx, sy, sy);
            while let Some((x, y)) = stack.pop() {
                area += 1;
                sxs += x as f64;
                sys += y as f64;
                minx = minx.min(x); maxx = maxx.max(x); miny = miny.min(y); maxy = maxy.max(y);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 { continue; }
                        let ni = (ny as u32 * w + nx as u32) as usize;
                        if mask[ni] && !seen[ni] { seen[ni] = true; stack.push((nx as u32, ny as u32)); }
                    }
                }
            }
            out.push(Blob { area, min_x: minx, max_x: maxx, min_y: miny, max_y: maxy, cx: (sxs / area as f64) as f32, cy: (sys / area as f64) as f32 });
        }
    }
    out
}

/// Colour classes used on the minimap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Px { White, Red, Gold, Cyan, Green, Other }

pub fn classify_px(r: u8, g: u8, b: u8) -> Px {
    let mn = r.min(g).min(b);
    let mx = r.max(g).max(b);
    if mn >= 205 && mx - mn <= 45 { return Px::White; }
    let (h, s, v) = hsv(r, g, b);
    if s > 0.62 && v > 0.78 && (h < 16.0 || h > 348.0) { return Px::Red; }
    if s > 0.6 && v > 0.78 && (38.0..60.0).contains(&h) { return Px::Gold; }
    if s > 0.55 && v > 0.75 && (170.0..197.0).contains(&h) { return Px::Cyan; }
    if s > 0.6 && v > 0.6 && (100.0..150.0).contains(&h) { return Px::Green; }
    Px::Other
}

pub struct Masks {
    pub w: u32,
    pub h: u32,
    pub white: Vec<bool>,
    pub red: Vec<bool>,
    pub gold: Vec<bool>,
    pub cyan: Vec<bool>,
    pub green: Vec<bool>,
}

pub fn masks(img: &RgbImage) -> Masks {
    let (w, h) = img.dimensions();
    let n = (w * h) as usize;
    let mut m = Masks { w, h, white: vec![false; n], red: vec![false; n], gold: vec![false; n], cyan: vec![false; n], green: vec![false; n] };
    for (x, y, p) in img.enumerate_pixels() {
        let i = (y * w + x) as usize;
        match classify_px(p[0], p[1], p[2]) {
            Px::White => m.white[i] = true,
            Px::Red => m.red[i] = true,
            Px::Gold => m.gold[i] = true,
            Px::Cyan => m.cyan[i] = true,
            Px::Green => m.green[i] = true,
            Px::Other => {}
        }
    }
    m
}

// Geometry traced on a 3x enlargement of the canonical crop (divide by 3 for canonical px).
// Own base at the bottom, enemy base at the top.
const GREEN_PX: &[(f32, f32)] = &[(432.,930.),(415.,880.),(395.,850.),(360.,832.),(330.,825.),(270.,790.),(225.,765.),(205.,742.),(170.,700.),(150.,665.),(130.,630.),(125.,580.),(135.,545.),(150.,520.),(160.,490.),(175.,450.),(180.,410.),(200.,380.),(225.,345.),(240.,315.),(280.,310.),(330.,290.),(370.,262.),(410.,255.),(440.,245.),(480.,230.),(515.,205.),(520.,170.),(540.,140.)];
const BLUE_PX: &[(f32, f32)] = &[(432.,930.),(428.,900.),(428.,860.),(428.,770.),(428.,730.),(430.,700.),(445.,680.),(470.,660.),(485.,630.),(488.,590.),(487.,540.),(487.,490.),(490.,455.),(515.,410.),(540.,380.),(540.,330.),(540.,293.),(540.,230.),(540.,150.)];
const YELLOW_PX: &[(f32, f32)] = &[(440.,940.),(445.,900.),(450.,870.),(470.,850.),(500.,835.),(528.,823.),(580.,822.),(620.,820.),(660.,800.),(690.,775.),(722.,755.),(750.,710.),(770.,675.),(793.,623.),(810.,575.),(822.,520.),(832.,480.),(845.,455.),(830.,420.),(800.,395.),(780.,370.),(760.,325.),(720.,305.),(680.,280.),(640.,245.),(600.,225.),(565.,205.),(558.,170.),(545.,140.)];

/// Structure diamonds and base icons (3x px). Blobs centred on these are map furniture,
/// not troopers, heroes or the urn.
const STRUCT_PX: &[(f32, f32)] = &[
    (540., 100.), (470., 185.), (612., 185.), (440., 245.), (640., 245.), (540., 293.), (540., 380.),
    (758., 325.), (237., 313.), (178., 445.), (845., 455.),
    (205., 742.), (330., 825.), (528., 823.), (722., 755.), (793., 623.), (428., 770.), (428., 685.),
    (360., 880.), (495., 880.), (430., 935.),
];

fn div3(v: &[(f32, f32)]) -> Vec<(f32, f32)> { v.iter().map(|(x, y)| (x / 3.0, y / 3.0)).collect() }

/// Lane centre lines in canonical px, ordered own base -> enemy base. Index 0 = Green, 1 = Blue, 2 = Yellow.
pub fn lanes() -> [Vec<(f32, f32)>; 3] { [div3(GREEN_PX), div3(BLUE_PX), div3(YELLOW_PX)] }
pub fn structures() -> Vec<(f32, f32)> { div3(STRUCT_PX) }
pub fn norm(cx: f32, cy: f32, w: u32, h: u32) -> Pt { Pt { x: cx / w as f32, y: cy / h as f32 } }

fn dilate(mask: &[bool], w: u32, h: u32, radius: i32) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    for y in 0..h as i32 { for x in 0..w as i32 {
        if !mask[(y as u32 * w + x as u32) as usize] { continue; }
        for dy in -radius..=radius { for dx in -radius..=radius {
            let (xx, yy) = (x + dx, y + dy);
            if xx >= 0 && yy >= 0 && xx < w as i32 && yy < h as i32 {
                out[(yy as u32 * w + xx as u32) as usize] = true;
            }
        }}
    }}
    out
}

/// Circular support, rather than a component's rectangle, distinguishes portrait rings
/// from diamonds and fragmented lines. Nearby maxima are suppressed; never invent heroes
/// to split a merged component.
/// Team-ring colour masks for hero portraits (measured on saved frames): allied rings are
/// cream-white (~242,231,213), enemy rings red-orange (~224,68,28).
pub fn ring_masks(img: &RgbImage) -> (Vec<bool>, Vec<bool>) {
    let n = (img.width() * img.height()) as usize;
    let (mut ally, mut enemy) = (vec![false; n], vec![false; n]);
    for (i, p) in img.pixels().enumerate() {
        let (h, s, v) = hsv(p[0], p[1], p[2]);
        if v >= 0.74 && s <= 0.24 && p[0] >= 175 { ally[i] = true; }
        else if (h < 26.0 || h > 340.0) && s >= 0.6 && v >= 0.55 { enemy[i] = true; }
    }
    (ally, enemy)
}

/// A detected hero portrait ring in canonical minimap px, with its ring coverage (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring { pub x: f32, pub y: f32, pub coverage: f32 }

/// Find hero portrait rings in a team-colour mask: "large circles" of the measured radius.
///
/// For each candidate centre, 36 rays sample a +-1 px band at each radius; coverage is the
/// fraction of rays that hit. Occlusion by structures or overlapping portraits is tolerated
/// (coverage >= `min_coverage`, spread over at least three quadrants). Two checks reject map
/// furniture that a naive circle test accepts: a filled interior (solid diamonds / blobs), and
/// missing diagonal support just outside the ring (hollow diamonds reach the radius only on
/// their axes). Heroes standing on towers are therefore kept instead of position-filtered.
pub fn hero_rings(mask: &[bool], w: u32, h: u32, s: f32, min_coverage: f32) -> Vec<Ring> {
    rings_scaled(mask, w, h, s, min_coverage, 1.0)
}
/// `hero_rings` for a ring of `scale` x the portrait size (e.g. the smaller player marker).
pub fn rings_scaled(mask: &[bool], w: u32, h: u32, s0: f32, min_coverage: f32, scale: f32) -> Vec<Ring> {
    const RAYS: usize = 36;
    let radii = [9.0f32, 10.0, 11.0];
    // Work in units of the portrait ring: every length below is multiplied by `scale`.
    let s = s0 * scale;
    let at = |x: i32, y: i32| x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h && mask[(y as u32 * w + x as u32) as usize];
    // Integral image: skip centres whose neighbourhood cannot hold a ring (most of the map).
    let mut integral = vec![0u32; ((w + 1) * (h + 1)) as usize];
    for y in 0..h { let mut row = 0u32; for x in 0..w {
        row += mask[(y * w + x) as usize] as u32;
        integral[((y + 1) * (w + 1) + x + 1) as usize] = integral[(y * (w + 1) + x + 1) as usize] + row;
    }}
    let boxed = |x0: i32, y0: i32, x1: i32, y1: i32| {
        let (x0, y0, x1, y1) = (x0.clamp(0, w as i32) as u32, y0.clamp(0, h as i32) as u32, x1.clamp(0, w as i32) as u32, y1.clamp(0, h as i32) as u32);
        let g = |x: u32, y: u32| integral[(y * (w + 1) + x) as usize] as i64;
        (g(x1, y1) - g(x0, y1) - g(x1, y0) + g(x0, y0)).max(0) as u32
    };
    let rays: Vec<(f32, f32)> = (0..RAYS).map(|i| { let a = i as f32 * std::f32::consts::TAU / RAYS as f32; (a.cos(), a.sin()) }).collect();
    let outer = (12.5 * s).ceil() as i32;
    let need = (0.28 * min_coverage * std::f32::consts::TAU * 10.0 * 2.5 * s * s) as u32;
    let step = (s * 0.375).floor().max(1.0) as usize;
    let mut found: Vec<(Ring, f32)> = vec![];
    for y in (0..h as i32).step_by(step) { for x in (0..w as i32).step_by(step) {
        if boxed(x - outer, y - outer, x + outer + 1, y + outer + 1) < need { continue; }
        // Interior: a portrait, not team colour.
        let inner = boxed(x - (4.0 * s) as i32, y - (4.0 * s) as i32, x + (4.0 * s) as i32 + 1, y + (4.0 * s) as i32 + 1);
        let inner_area = ((8.0 * s) as u32 + 1).pow(2);
        if inner as f32 > 0.45 * inner_area as f32 { continue; }
        let mut best = (0usize, 0f32, [0u8; 4]);
        for r in radii {
            let mut hits = 0usize; let mut quadrants = [0u8; 4];
            for (i, (c, sn)) in rays.iter().enumerate() {
                let hit = [-1.0f32, 0.0, 1.0].iter().any(|d| at(x + ((r + d) * s * c).round() as i32, y + ((r + d) * s * sn).round() as i32));
                if hit { hits += 1; quadrants[i * 4 / RAYS] += 1; }
                if hits + (RAYS - 1 - i) < (min_coverage * RAYS as f32) as usize { break; }
            }
            if hits > best.0 { best = (hits, r, quadrants); }
        }
        let coverage = best.0 as f32 / RAYS as f32;
        if coverage < min_coverage || best.2.iter().filter(|q| **q >= 3).count() < 3 { continue; }
        // Roundness: a circle also has support on the diagonals at its outer edge.
        let r = best.1 + 1.0;
        let diagonal = (0..8).filter(|k| {
            let a = (45.0 + 90.0 * (*k / 2) as f32 + if k % 2 == 0 { -8.0 } else { 8.0 }).to_radians();
            [-1.0f32, 0.0].iter().any(|d| at(x + ((r + d) * s * a.cos()).round() as i32, y + ((r + d) * s * a.sin()).round() as i32))
        }).count();
        if diagonal < 4 { continue; }
        found.push((Ring { x: x as f32 / s0, y: y as f32 / s0, coverage }, best.1));
    }}
    // Greedy explain-away: accept the strongest ring, claim its band pixels, then re-score the
    // rest on unclaimed pixels only. Arcs of two overlapping portraits no longer combine into
    // a phantom third ring, while a genuinely overlapped portrait keeps its own visible arc.
    found.sort_by(|a, b| b.0.coverage.total_cmp(&a.0.coverage));
    let mut claimed = vec![false; mask.len()];
    let mut out: Vec<Ring> = vec![];
    for (ring, r) in found {
        if out.iter().any(|o| dist((o.x, o.y), (ring.x, ring.y)) < 7.5 * scale) { continue; }
        let (cx, cy) = ((ring.x * s0) as i32, (ring.y * s0) as i32);
        let free = |x: i32, y: i32| at(x, y) && !claimed[(y as u32 * w + x as u32) as usize];
        let hits = rays.iter().filter(|(c, sn)| [-1.0f32, 0.0, 1.0].iter()
            .any(|d| free(cx + ((r + d) * s * c).round() as i32, cy + ((r + d) * s * sn).round() as i32))).count();
        let coverage = hits as f32 / RAYS as f32;
        if coverage < min_coverage { continue; }
        let band = ((r + 2.0) * s).ceil() as i32;
        for dy in -band..=band { for dx in -band..=band {
            let d = ((dx * dx + dy * dy) as f32).sqrt() / s;
            let (x, y) = (cx + dx, cy + dy);
            if (r - 2.5..=r + 2.5).contains(&d) && x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h { claimed[(y as u32 * w + x as u32) as usize] = true; }
        }}
        out.push(Ring { coverage, ..ring });
    }
    out
}

/// The playable map is roughly an ellipse inside the square crop; outside it the game world
/// shows through and can contain unrelated circles.
/// The map panel is a disc (x 15-319, y 37-333) plus the bump (y 15-60) that holds the enemy base icon;
/// the corners above the side lanes are game world (rec5 read scenery at (100,29), (232,35)
/// and (290,87) as enemies on many frames under the looser 150x165 ellipse).
pub fn on_map(p: (f32, f32)) -> bool {
    ((p.0 - 167.0) / 152.0).powi(2) + ((p.1 - 185.0) / 148.0).powi(2) < 1.0 || ((150.0..=212.0).contains(&p.0) && (15.0..=60.0).contains(&p.1))
}

/// Elliptic radius of a canonical point on the map disc (0 = centre, 1 = `on_map` edge).
fn rim_value(p: (f32, f32)) -> f32 { ((p.0 - 167.0) / 152.0).powi(2) + ((p.1 - 185.0) / 148.0).powi(2) }

/// A ring near the rim must be surrounded by the dark map panel, not by lit game world.
fn on_dark_panel(img: &RgbImage, p: (f32, f32), s: f32) -> bool {
    let (mut n, mut dark) = (0u32, 0u32);
    let (r0, r1) = ((13.0 * s) as i32, (17.0 * s) as i32);
    for dy in -r1..=r1 { for dx in -r1..=r1 {
        let d2 = dx * dx + dy * dy;
        if d2 < r0 * r0 || d2 > r1 * r1 { continue; }
        let (x, y) = ((p.0 * s) as i32 + dx, (p.1 * s) as i32 + dy);
        if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() { continue; }
        let q = img.get_pixel(x as u32, y as u32);
        n += 1;
        if q.0.iter().copied().max().unwrap() < 90 { dark += 1; }
    }}
    n > 0 && dark as f32 >= 0.35 * n as f32
}

/// Erase team-ping circles from an enemy-colour mask: large thin rings (canonical diameter
/// 36-120 px, a portrait ring is ~22) whose pixels lie on one circle around most of the way.
pub fn erase_ping_circles(mask: &mut [bool], w: u32, h: u32, s: f32) {
    for b in components(mask, w, h) {
        let (bw, bh) = (b.w() as f32 / s, b.h() as f32 / s);
        // No aspect limit: a ping merges with lane lines and portraits it touches (rec5 frame 547: 77x44).
        if !(34.0..=120.0).contains(&bw) || !(34.0..=120.0).contains(&bh) || b.fill() > 0.65 { continue; }
        // The ping's edge is one continuous circle; a cluster of portraits only touches any
        // circle here and there. Search centre and radius (a portrait can be merged into the
        // component, so the bounding box is not centred on the ping).
        let at = |x: f32, y: f32| { let (x, y) = (x.round() as i32, y.round() as i32);
            x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h && mask[(y as u32 * w + x as u32) as usize] };
        let (bx, by) = ((b.min_x + b.max_x) as f32 / 2.0, (b.min_y + b.max_y) as f32 / 2.0);
        let step = s.max(1.0);
        let mut best = (0usize, bx, by, 0f32);
        for oy in -6..=6 { for ox in -6..=6 {
            let (cx, cy) = (bx + ox as f32 * step, by + oy as f32 * step);
            let mut r = 16.0 * s;
            while r <= 0.5 * (b.w().min(b.h()) as f32) + 2.0 * s {
                let hits = (0..72).filter(|i| { let a = *i as f32 * std::f32::consts::TAU / 72.0;
                    [-1.0f32, 0.0, 1.0].iter().any(|d| at(cx + (r + d * s) * a.cos(), cy + (r + d * s) * a.sin())) }).count();
                if hits > best.0 { best = (hits, cx, cy, r); }
                r += step;
            }
        }}
        let (hits, cx, cy, r) = best;
        if hits < 60 { continue; }
        // A ping's band is thick (~4-8 px while it shrinks); touching portrait rings are ~2 px
        // lines, so they cannot fill an annulus just inside the circle. Thin large pings
        // (sparse components) pass on the circle alone.
        let (mut ann, mut ann_n) = (0u32, 0u32);
        for y in b.min_y..=b.max_y { for x in b.min_x..=b.max_x {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let d = (dx * dx + dy * dy).sqrt();
            if d >= r - 3.5 * s && d <= r - 0.5 * s { ann_n += 1; ann += mask[(y * w + x) as usize] as u32; }
        }}
        let thick = ann_n > 0 && ann as f32 >= 0.5 * ann_n as f32;
        if !thick && (b.fill() > 0.45 || hits < 66) { continue; }
        let band = 1.5 * s;
        let (mut inner, mut inner_n) = (0u32, 0u32);
        for y in b.min_y..=b.max_y { for x in b.min_x..=b.max_x {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            if (dx * dx + dy * dy).sqrt() < 0.6 * r { inner_n += 1; inner += mask[(y * w + x) as usize] as u32; }
        }}
        // A filled (danger) ping tints its whole disc: erase it all; a hollow one only its band,
        // so portraits inside it stay.
        let filled = inner_n > 0 && inner as f32 > 0.2 * inner_n as f32;
        for y in b.min_y..=b.max_y { for x in b.min_x..=b.max_x {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let d = (dx * dx + dy * dy).sqrt();
            if (filled && d <= r + 2.0 * band) || (d - r).abs() <= 2.0 * band + 1.0 { mask[(y * w + x) as usize] = false; }
        }}
    }
}

/// Base icons are large team-coloured arcs that partially match a ring.
const BASES: [(f32, f32); 2] = [(180.0, 33.0), (143.0, 312.0)];

/// Mean brightness and spread of the ring interior. Portraits are lit faces (bright, varied);
/// a hollow structure diamond has a uniform dark hole.
fn interior_is_portrait(img: &RgbImage, p: (f32, f32), s: f32) -> bool {
    // A structure interior is only structure colour (red / team hue) or its dark hole.
    let (mut n, mut furniture) = (0f32, 0f32);
    let r = (5.5 * s) as i32;
    for dy in -r..=r { for dx in -r..=r {
        if dx * dx + dy * dy > r * r { continue; }
        let (x, y) = ((p.0 * s) as i32 + dx, (p.1 * s) as i32 + dy);
        if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() { continue; }
        let q = img.get_pixel(x as u32, y as u32); let (h, sat, v) = hsv(q[0], q[1], q[2]);
        n += 1.0;
        if v < 0.28 || (sat > 0.45 && (h < 26.0 || h > 340.0 || (38.0..62.0).contains(&h) || (85.0..218.0).contains(&h))) { furniture += 1.0; }
    }}
    n > 0.0 && furniture / n < 0.6
}

/// Fraction of the ring interior that is dark map (value < 0.28).
fn interior_dark(img: &RgbImage, p: (f32, f32), s: f32) -> f32 {
    let (mut n, mut dark) = (0f32, 0f32);
    let r = (5.5 * s) as i32;
    for dy in -r..=r { for dx in -r..=r {
        if dx * dx + dy * dy > r * r { continue; }
        let (x, y) = ((p.0 * s) as i32 + dx, (p.1 * s) as i32 + dy);
        if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() { continue; }
        let q = img.get_pixel(x as u32, y as u32);
        n += 1.0;
        if hsv(q[0], q[1], q[2]).2 < 0.28 { dark += 1.0; }
    }}
    if n > 0.0 { dark / n } else { 0.0 }
}

const MIDBOSS_POS: (f32, f32) = (162.0, 175.0);
/// Side-lane bridges (canonical px): urn spawn points and bridge power-up spawns.
const BRIDGES: [(f32, f32); 2] = [(68.0, 175.0), (259.0, 175.0)];
/// Mint shop icons (canonical px) that pass the player-marker colour test in every match.
const SHOP_ICONS: [(f32, f32); 5] = [(23.0, 207.0), (104.0, 163.0), (243.0, 207.0), (217.0, 196.0), (177.0, 197.0)];

/// Lane for a polyline index (0 = left, 1 = middle, 2 = right) under the detected layout.
pub fn lane_of(k: usize, mirrored: bool) -> coach_core::Lane {
    use coach_core::Lane::*;
    match (k, mirrored) { (1, _) => Blue, (0, false) | (2, true) => Green, _ => Yellow }
}
fn lane_state(st: &mut MinimapState, k: usize, mirrored: bool) -> &mut coach_core::LaneState {
    match lane_of(k, mirrored) { coach_core::Lane::Green => &mut st.green, coach_core::Lane::Blue => &mut st.blue, coach_core::Lane::Yellow => &mut st.yellow }
}

pub struct Analyzer {
    lanes: [Vec<(f32, f32)>; 3],
    cum: [Vec<f32>; 3],
    structs: Vec<(f32, f32)>,
}

impl Default for Analyzer { fn default() -> Self { Self::new() } }

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 { (a.0 - b.0).hypot(a.1 - b.1) }

impl Analyzer {
    pub fn new() -> Analyzer {
        let lanes = lanes();
        let cum = [0, 1, 2].map(|k| {
            let mut c = vec![0.0f32];
            for w in lanes[k].windows(2) { c.push(c.last().unwrap() + dist(w[0], w[1])); }
            c
        });
        Analyzer { lanes, cum, structs: structures() }
    }

    /// Nearest lane to a canonical point: (lane index, progress 0..1, distance px).
    pub fn project(&self, p: (f32, f32)) -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f32)> = None;
        for k in 0..3 {
            let l = &self.lanes[k];
            let total = *self.cum[k].last().unwrap();
            for i in 0..l.len() - 1 {
                let (a, b) = (l[i], l[i + 1]);
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len2 = dx * dx + dy * dy;
                let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
                let q = (a.0 + dx * t, a.1 + dy * t);
                let d = dist(p, q);
                if best.map(|b| d < b.2).unwrap_or(true) {
                    best = Some((k, (self.cum[k][i] + t * len2.sqrt()) / total, d));
                }
            }
        }
        best
    }

    fn near_structure(&self, p: (f32, f32), r: f32) -> bool { self.structs.iter().any(|s| dist(*s, p) < r) }

    /// Fraction of the lane drawn in lane colour vs red, sampling the line pixels.
    fn push_fraction(&self, img: &RgbImage, k: usize, s: f32, mirrored: bool) -> Option<f32> {
        let (w, h) = img.dimensions();
        let (green, blue, yellow) = ((85.0, 150.0), (190.0, 218.0), (38.0, 62.0));
        let (lo, hi) = match (k, mirrored) { (1, _) => blue, (0, false) | (2, true) => green, _ => yellow };
        let r = (1.6 * s).round().max(1.0) as i32;
        let total = *self.cum[k].last().unwrap();
        // Each sample along the lane: Some(true) = your lane colour, Some(false) = red, None = neither.
        let mut samples: Vec<(f32, Option<bool>)> = vec![];
        let mut d = 0.0;
        while d < total {
            let t = d / total;
            d += 1.0;
            if !(0.09..=0.93).contains(&t) { continue; }
            // point at arc length d on the polyline
            let i = self.cum[k].partition_point(|c| *c <= d - 1.0).saturating_sub(1).min(self.lanes[k].len() - 2);
            let (a, b) = (self.lanes[k][i], self.lanes[k][i + 1]);
            let seg = (self.cum[k][i + 1] - self.cum[k][i]).max(1e-3);
            let f = ((d - 1.0 - self.cum[k][i]) / seg).clamp(0.0, 1.0);
            let (px, py) = ((a.0 + (b.0 - a.0) * f) * s, (a.1 + (b.1 - a.1) * f) * s);
            let (mut has_lane, mut has_red) = (false, false);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (px as i32 + dx, py as i32 + dy);
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { continue; }
                    let p = img.get_pixel(x as u32, y as u32);
                    let (hh, ss, vv) = hsv(p[0], p[1], p[2]);
                    if ss < 0.42 || vv < 0.42 { continue; }
                    if (hh < 24.0 || hh > 345.0) && vv > 0.5 { has_red = true; }
                    else if hh >= lo && hh <= hi { has_lane = true; }
                }
            }
            samples.push((t, if has_lane { Some(true) } else if has_red { Some(false) } else { None }));
        }
        let coloured = samples.iter().filter(|(_, c)| c.is_some()).count();
        if coloured < 40 { return None; }
        // The front is where your colour stops being the majority, walking from your base. A share
        // of coloured pixels (the old measure) is pulled toward the middle by red troopers, rings and
        // structures on your side, so a lane owned up to their guardian read ~0.47 in a recording.
        // Pick the single split that best explains the lane as "yours, then theirs": maximise
        // (your colour before) - (red before), i.e. the peak of a running +1/-1 sum. A local
        // occluder (an enemy portrait ring, a red trooper dot) only dents the sum, so it cannot
        // end the walk early the way a sliding window did.
        let (mut run, mut best, mut first, mut last) = (0i32, 0i32, 0usize, 0usize);
        for (j, (_, c)) in samples.iter().enumerate() {
            run += match c { Some(true) => 1, Some(false) => -1, None => 0 };
            if run > best { best = run; first = j + 1; last = j + 1; } else if run == best && best > 0 { last = j + 1; }
        }
        if best == 0 { return Some(samples[0].0); }
        let at = ((first + last) / 2).min(samples.len() - 1);
        Some(samples[at].0)
    }

    /// Lane colours depend on which team you are: the map is drawn from your base, so the
    /// side lanes swap between Green and Yellow. Count green- vs yellow-hued pixels along the
    /// left and right lane polylines (red, enemy-owned stretches ignored).
    /// Returns (left green, left yellow, right green, right yellow).
    pub fn side_lane_hues(&self, img: &RgbImage) -> (u32, u32, u32, u32) {
        let (w, h) = img.dimensions(); let s = w as f32 / CW;
        let mut out = [0u32; 4];
        for (slot, k) in [(0usize, 0usize), (2, 2)] {
            let total = *self.cum[k].last().unwrap();
            let mut d = 0.0;
            while d < total {
                let i = self.cum[k].partition_point(|c| *c <= d).saturating_sub(1).min(self.lanes[k].len() - 2);
                let (a, b) = (self.lanes[k][i], self.lanes[k][i + 1]);
                let f = ((d - self.cum[k][i]) / (self.cum[k][i + 1] - self.cum[k][i]).max(1e-3)).clamp(0.0, 1.0);
                let (px, py) = ((a.0 + (b.0 - a.0) * f) * s, (a.1 + (b.1 - a.1) * f) * s);
                d += 1.0;
                let r = (1.6 * s).round().max(1.0) as i32;
                for dy in -r..=r { for dx in -r..=r {
                    let (x, y) = (px as i32 + dx, py as i32 + dy);
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { continue; }
                    let p = img.get_pixel(x as u32, y as u32); let (hh, ss, vv) = hsv(p[0], p[1], p[2]);
                    if ss < 0.42 || vv < 0.42 { continue; }
                    if (85.0..150.0).contains(&hh) { out[slot] += 1; } else if (38.0..62.0).contains(&hh) { out[slot + 1] += 1; }
                }}
            }
        }
        (out[0], out[1], out[2], out[3])
    }

    pub fn analyze(&self, img: &RgbImage) -> MinimapState { self.run(img, &mut vec![]) }

    /// Analyze and also return debug markers: (x, y in canonical px, kind).
    /// Kinds: a/e = ally/enemy trooper, E = enemy ring, M = midboss, U = urn.
    pub fn run(&self, img: &RgbImage, mk: &mut Vec<(f32, f32, char)>) -> MinimapState { self.run_layout(img, mk, None) }

    /// Distance (canonical px) and progress of a point projected onto lane polyline `k`.
    pub fn project_lane(&self, k: usize, p: (f32, f32)) -> (f32, f32) {
        let l = &self.lanes[k];
        let total = *self.cum[k].last().unwrap();
        let mut best = (f32::INFINITY, 0.0);
        for i in 0..l.len() - 1 {
            let (a, b) = (l[i], l[i + 1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
            let d = dist(p, (a.0 + dx * t, a.1 + dy * t));
            if d < best.0 { best = (d, (self.cum[k][i] + t * len2.sqrt()) / total); }
        }
        best
    }

    /// `run` with the lane layout already established for this match (`Tracker::layout`).
    /// The per-frame colour evidence flipped Green/Yellow on 22 frames of a recorded match
    /// (every side-lane field and your lane with it); a latched layout keeps them consistent.
    /// This frame's own evidence is still reported in `lanes_mirrored_frame` for the vote.
    pub fn run_layout(&self, img: &RgbImage, mk: &mut Vec<(f32, f32, char)>, layout: Option<bool>) -> MinimapState {
        let (w, h) = img.dimensions();
        let s = w as f32 / CW;
        let m = masks(img);
        let mut st = MinimapState::default();
        let mut sums = [[(0u32, 0f32); 2]; 3]; // lane -> [ally, enemy] -> (count, sum progress)
        // Heroes: mask each team colour, then look for portrait-sized circles (6 per team max;
        // the own hero is drawn in green, so at most 5 cream-white allied rings).
        let (ally_mask, mut enemy_mask) = ring_masks(img);
        // Red ping circles (a team ping: a thick ring ~2.5x a portrait) put several "enemy rings"
        // on their band (rec5 frame 122 read six enemies). Erase them before the ring search.
        erase_ping_circles(&mut enemy_mask, w, h, s);
        let plausible = |p: &(f32, f32)| on_map(*p) && dist(BASES[0], *p) > 32.0 && dist(BASES[1], *p) > 22.0
            && (!self.near_structure(*p, 6.0) || interior_is_portrait(img, *p, s))
            // A ring just below a small base diamond is the diamond plus lane line and trooper
            // dots (rec5: two "enemies" under the enemy base towers on 1/5 of frames); its
            // interior is the dark map, a portrait's is lit.
            && (!self.near_structure(*p, 12.0) || interior_dark(img, *p, s) < 0.6)
            // Near the map rim the game world shows through: a ring there must sit on the dark
            // map panel (rec5 frame 560 read lit scenery past the rim as an enemy).
            && (rim_value(*p) < 0.72 || on_dark_panel(img, *p, s));
        let ring_pts: Vec<(f32, f32)> = hero_rings(&enemy_mask, w, h, s, 0.5).into_iter()
            .map(|r| (r.x, r.y)).filter(plausible).take(6).collect();
        let ally_pts: Vec<(f32, f32)> = hero_rings(&ally_mask, w, h, s, 0.5).into_iter()
            .map(|r| (r.x, r.y)).filter(plausible).take(5).collect();
        // Side-lane colours swap between teams; detect the layout from the lane lines themselves.
        let (lg, ly, rg, ry) = self.side_lane_hues(img);
        let evidence = lg + ly + rg + ry;
        let frame_mirrored = evidence >= 100 && (ly + rg) as f32 > 1.5 * (lg + ry) as f32;
        // Vote only with a clear colour majority either way.
        st.lanes_mirrored_frame = (evidence >= 100).then(|| if frame_mirrored { Some(true) } else if (lg + ry) as f32 > 1.5 * (ly + rg) as f32 { Some(false) } else { None }).flatten();
        let mirrored = layout.unwrap_or(frame_mirrored);
        st.lanes_mirrored = if layout.is_some() { layout } else { (evidence >= 100).then_some(mirrored) };
        // Your marker: a green ring (hue ~136-142). Recorded captures measured saturation 0.49-0.58,
        // below the general green mask, so it gets its own mask; the ring test rejects lane lines.
        let own_mask: Vec<bool> = img.pixels().map(|p| { let (hh, ss, vv) = hsv(p[0], p[1], p[2]); (128.0..148.0).contains(&hh) && ss > 0.45 && vv > 0.45 }).collect();
        // Occlusion-tolerant circle search at the marker size (~0.58 of a portrait ring), minus the
        // mint shop icons that sit at fixed map positions in every match.
        // Standing on a shop icon is common (rec5: 30+ frames lost at the mid-right shops): the
        // marker there is kept when the green mass is the marker's (~75 px within 9 px of the
        // centre, canonical) rather than the bare icon's (~18).
        let green_mass = |p: (f32, f32)| {
            let r = (9.0 * s) as i32; let (cx, cy) = ((p.0 * s) as i32, (p.1 * s) as i32);
            let mut n = 0u32;
            for dy in -r..=r { for dx in -r..=r {
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h && own_mask[(y as u32 * w + x as u32) as usize] { n += 1; }
            }}
            n as f32 / (s * s)
        };
        let own:Vec<_> = rings_scaled(&own_mask,w,h,s,0.6,0.58).into_iter().map(|r|(r.x,r.y))
            .filter(|p| SHOP_ICONS.iter().all(|q| dist(*p, *q) > 8.0) || green_mass(*p) >= 45.0)
            // Standing next to a tower is common; the ring test already rejects diamonds.
            // On the map disc only: rec3 read green game world beside the minimap as you (x=0.01),
            // rec5 frame 331 green scenery past the top-right rim (four rings, so no marker at all).
            .filter(|p| p.1>75.0 && p.1<325.0 && p.0>22.0 && p.0<313.0 && on_map(*p)).collect();
        // A unique bright-green ring is only a candidate; the temporal tracker confirms it.
        if let [p] = own.as_slice() {
            st.you=Some(norm(p.0,p.1,CW as u32,CH as u32)); mk.push((p.0,p.1,'Y'));
            // 24 px: standing beside your own guardian puts the marker 19-23 px off the lane line
            // (rec3: five such frames had no lane); jungle paths measured 27+ px away.
            if let Some((lane,progress,d))=self.project(*p) { if d<=24.0 {
                st.your_lane=Some(lane_of(lane, mirrored)); st.your_progress=Some(progress);
            }}
        }

        // Lane fronts.
        for k in 0..3 { let push = self.push_fraction(img, k, s, mirrored); lane_state(&mut st, k, mirrored).push = push; }

        // Troopers: small solid white (ally) / red (enemy) specks that sit on a lane.
        let min_a = (2.0 * s * s) as u32;
        let max_a = (28.0 * s * s) as u32;
        let max_side = (7.5 * s) as u32;
        for (mask, team) in [(&m.white, 0usize), (&m.red, 1usize)] {
            for b in components(mask, w, h) {
                if b.area < min_a { continue; }
                // A wave walking in file merges into one blob (rec5 frame 250: ~5 dots read as a
                // single 12x8 component of 70 px, and dropped as too large). Merged blobs count as
                // area / single-dot area (12-15 px measured), up to a full wave.
                let single = b.area <= max_a && b.w() <= max_side && b.h() <= max_side;
                let merged = !single && b.area as f32 <= 110.0 * s * s && (b.w().max(b.h()) as f32) <= 16.0 * s && b.fill() <= 0.75;
                if !single && !merged { continue; }
                let n = if single { 1 } else { ((b.area as f32 / (13.0 * s * s)).round() as u32).clamp(2, 8) };
                let p = (b.cx / s, b.cy / s);
                if self.near_structure(p, if single { 9.0 } else { 12.0 }) { continue; }
                // Small portrait highlights and fragmented rings are not trooper dots.
                // The ring edge is ~11 px out; troopers fighting right next to a hero must still count.
                if ring_pts.iter().chain(ally_pts.iter()).any(|h| dist(*h, p) < if single { 11.5 } else { 16.0 }) { continue; }
                if let Some((k, t, d)) = self.project(p) {
                    if d <= if single { 6.0 } else { 4.0 } && (0.10..=0.93).contains(&t) {
                        sums[k][team].0 += n;
                        sums[k][team].1 += t * n as f32;
                        mk.push((p.0, p.1, if team == 0 { 'a' } else { 'e' }));
                    }
                }
            }
        }
        for k in 0..3 {
            let lane = lane_state(&mut st, k, mirrored);
            lane.ally_troopers = sums[k][0].0;
            lane.enemy_troopers = sums[k][1].0;
            if sums[k][0].0 > 0 { lane.ally_progress = Some(sums[k][0].1 / sums[k][0].0 as f32); }
            if sums[k][1].0 > 0 { lane.enemy_progress = Some(sums[k][1].1 / sums[k][1].0 as f32); }
        }

        for p in &ring_pts { mk.push((p.0, p.1, 'E')); }
        st.enemy_count = ring_pts.len() as u32;
        st.enemies = ring_pts.iter().map(|(x, y)| norm(*x, *y, CW as u32, CH as u32)).collect();
        // Drop only the ring that is your own marker (centres within ~0.01); a teammate stacked
        // on you sat 0.017 away in rec3 and was deleted by the old 0.02 radius.
        st.allies = ally_pts.iter().map(|(x, y)| norm(*x, *y, CW as u32, CH as u32))
            .filter(|p| st.you.is_none_or(|me| (me.x-p.x).hypot(me.y-p.y)>0.01)).collect();

        // Midboss: gold winged icon near the map centre.
        let mut gold = 0u32;
        for b in components(&m.gold, w, h) {
            if dist((b.cx / s, b.cy / s), MIDBOSS_POS) < 26.0 { gold += b.area; mk.push((b.cx / s, b.cy / s, 'M')); }
        }
        st.midboss_up = gold as f32 >= 22.0 * s * s;

        // Urn: bright cyan vase that is not a lane diamond.
        let mut urn: Option<(f32, f32, u32)> = None;
        let mut on_bridge = [false; 2];
        // Join the vase cap and body before shape checks. Portrait highlights generally
        // lack this tall cap/body silhouette. Also exclude the small centre-lane diamond.
        let cyan_joined = dilate(&m.cyan, w, h, s.round().max(1.0) as i32);
        for b in components(&cyan_joined, w, h) {
            let p = (b.cx / s, b.cy / s);
            let (bw, bh) = (b.w() as f32 / s, b.h() as f32 / s);
            if self.near_structure(p, 12.0) || dist(p, (162.0, 201.0)) < 12.0 || !on_map(p) { continue; }
            if b.area as f32 / (s * s) < 40.0 || bw > 18.0 || bh > 24.0 || bh < 13.0 || bh < bw * 1.15 {
                // A carried vase is drawn smaller and partly covered by your view cone (rec5
                // 1:00-1:25 of a carry: 7x7 to 8x11 px). Kept as a weak mark for the tracker only.
                if b.area as f32 / (s * s) >= 25.0 && bh >= 7.0 && bw <= 14.0 && bh <= 18.0 && bh >= bw * 0.9 {
                    st.urn_marks.push(norm(p.0, p.1, CW as u32, CH as u32));
                }
                continue;
            }
            on_bridge[0] |= dist(p, BRIDGES[0]) < 14.0; on_bridge[1] |= dist(p, BRIDGES[1]) < 14.0;
            if urn.map(|u| b.area > u.2).unwrap_or(true) { urn = Some((p.0, p.1, b.area)); }
        }
        // Bridge power-ups spawn on both bridges at once (5:00, 10:00, ...), the urn is one object:
        // urn-shaped icons on both bridges are power-ups (a recorded match called them the urn).
        if on_bridge[0] && on_bridge[1] { urn = None; }
        if let Some((x, y, _)) = urn {
            mk.push((x, y, 'U'));
            st.urn_pos = Some(norm(x, y, CW as u32, CH as u32));
            st.urn = Some(UrnState::Visible);
            // Carried: a white pin/cone or a hero ring touches the vase.
            // Proximity alone does NOT establish possession. Carrier ownership is resolved
            // by the temporal tracker only if an urn and ring travel together.
        }
        st
    }
}

/// Short history suppresses flicker and expired evidence. No missing icon is interpreted
/// as a kill/delivery; it may simply be covered by a portrait or scoreboard tooltip.
#[derive(Default)]
pub struct Tracker {
    history: std::collections::VecDeque<(u64, MinimapState)>,
    heroes: Vec<HeroSighting>,
    next_id: u32,
    midboss: MidbossState,
    /// Last raw player-marker sighting (confirms the next one within 10 s).
    last_you: Option<(u64, Pt)>,
    /// Last confirmed lane, with where and when: held through short gaps nearby.
    last_lane: Option<(u64, Pt, coach_core::Lane, Option<f32>)>,
    /// Lane-layout votes this match: (mirrored, normal) weights and when last voted.
    layout_votes: (u32, u32, u64),
    /// Accepted zone label: (read at, zone, your position then).
    zone: Option<(u64, String, Option<Pt>)>,
    /// Last unconfirmed zone read (an unknown name needs a second identical read).
    zone_pending: Option<(u64, String)>,
    /// Carrying the urn: your marker is replaced by the vase. Last carried position.
    urn_you: Option<(u64, Pt)>,
    /// Last reported (confirmed) own position, for short holds on live frames.
    confirmed_you: Option<(u64, Pt)>,
}

/// A live frame without your marker (portraits stacked on it, effects) reports the last
/// confirmed position for this long: the frames just before a death are the ones that judge
/// "alone" and "forward", and a dive often covers the marker with enemy rings.
pub const OWN_HOLD_MS: u64 = 4_000;

/// Layout votes needed before the per-match layout overrides a frame's own colour evidence.
pub const LAYOUT_MIN_VOTES: u32 = 5;
/// A gap this long (or time going backwards) starts the layout vote over (a new match).
pub const LAYOUT_RESET_MS: u64 = 300_000;
/// An accepted zone label is held this long without a new read while you stay near.
pub const ZONE_HOLD_MS: u64 = 12_000;

/// The Midboss is alive from 0:00 and only leaves the map when killed (respawning 7 / 6 / 5 min
/// later). A recorded match flipped the per-frame flag 77 times, so state changes are events:
/// gone for 20 s straight = killed, seen again afterwards = respawned.
#[derive(Default)]
struct MidbossState { seen: bool, absent_since: Option<u64>, down_at: Option<u64>, back_at: Option<u64> }
pub const MIDBOSS_GONE_MS: u64 = 20_000;
/// Shortest Midboss respawn (5 min after the third and later kills), minus tracking slack.
pub const MIDBOSS_MIN_RESPAWN_MS: u64 = 270_000;

/// Internal track state: last sighting plus the one before it (for velocity).
#[derive(Clone)]
struct HeroSighting { id: u32, ally: bool, pos: Pt, seen_ms: u64, hits: u32, velocity: Option<Pt> }

/// Tracks older than this are forgotten: a hero unseen this long could be anywhere.
pub const TRACK_FORGET_MS: u64 = 45_000;

impl Tracker {
    fn vote_layout(&mut self, mirrored: bool, weight: u32, now_ms: u64) {
        if mirrored { self.layout_votes.0 += weight; } else { self.layout_votes.1 += weight; }
        self.layout_votes.2 = now_ms;
    }

    /// The last confirmed own position when at most `max_ms` old.
    pub fn held_you(&self, now_ms: u64, max_ms: u64) -> Option<Pt> {
        self.confirmed_you.filter(|(t, _)| now_ms >= *t && now_ms - t <= max_ms).map(|(_, p)| p)
    }
    /// Forget the held own position (death: the next live frame is a respawn elsewhere).
    pub fn forget_you(&mut self) { self.confirmed_you = None; }

    /// The lane layout latched for this match (majority of the votes so far), once at least
    /// `LAYOUT_MIN_VOTES` were cast. Feed it to `Analyzer::run_layout`.
    pub fn layout(&self) -> Option<bool> {
        let (m, n, _) = self.layout_votes;
        (m + n >= LAYOUT_MIN_VOTES && m != n).then_some(m > n)
    }

    /// Match this frame's portrait rings to existing tracks, per team. Greedy nearest-first
    /// assignment inside a gate that grows with elapsed time (heroes move; ziplines are fast).
    fn track_heroes(&mut self, raw: &MinimapState, now_ms: u64) -> Vec<coach_core::HeroTrack> {
        self.heroes.retain(|h| now_ms.saturating_sub(h.seen_ms) <= TRACK_FORGET_MS);
        for (ally, seen) in [(true, &raw.allies), (false, &raw.enemies)] {
            let mut pairs = vec![];
            for (ti, t) in self.heroes.iter().enumerate().filter(|(_, t)| t.ally == ally && t.seen_ms <= now_ms) {
                let dt = (now_ms - t.seen_ms) as f32 / 1000.0;
                let gate = (0.05 + 0.05 * dt).min(0.35);
                for (di, d) in seen.iter().enumerate() {
                    // Predict with the known velocity for short gaps only.
                    let lead = if dt <= 4.0 { t.velocity.unwrap_or_default() } else { Pt::default() };
                    let e = (t.pos.x + lead.x * dt - d.x).hypot(t.pos.y + lead.y * dt - d.y);
                    if e <= gate { pairs.push((e, ti, di)); }
                }
            }
            pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (mut used_t, mut used_d) = (vec![], vec![]);
            for (_, ti, di) in pairs {
                if used_t.contains(&ti) || used_d.contains(&di) { continue; }
                used_t.push(ti); used_d.push(di);
                let t = &mut self.heroes[ti]; let d = seen[di];
                let dt = (now_ms - t.seen_ms) as f32 / 1000.0;
                if (0.4..=8.0).contains(&dt) {
                    let v = Pt { x: (d.x - t.pos.x) / dt, y: (d.y - t.pos.y) / dt };
                    t.velocity = Some(match t.velocity { Some(o) => Pt { x: 0.5 * (o.x + v.x), y: 0.5 * (o.y + v.y) }, None => v });
                } else if dt > 8.0 { t.velocity = None; }
                t.pos = d; t.seen_ms = now_ms; t.hits += 1;
            }
            // A team has at most 6 heroes (5 allies besides you).
            let cap = if ally { 5 } else { 6 };
            for (di, d) in seen.iter().enumerate() {
                if used_d.contains(&di) { continue; }
                // With the whole team already tracked, an unexplained ring is one of them after a
                // jump (zipline, teleport, respawn) or a missed frame: continue the nearest track
                // not matched this frame instead of minting a new id (rec2: 140 ids for 11 heroes).
                let team = self.heroes.iter().filter(|h| h.ally == ally).count();
                let spare = (team >= cap).then(|| (0..self.heroes.len())
                    .filter(|i| self.heroes[*i].ally == ally && !used_t.contains(i))
                    .min_by(|a, b| { let e = |i: &usize| (self.heroes[*i].pos.x - d.x).hypot(self.heroes[*i].pos.y - d.y); e(a).total_cmp(&e(b)) })).flatten();
                if let Some(ti) = spare {
                    used_t.push(ti);
                    let t = &mut self.heroes[ti];
                    t.pos = *d; t.seen_ms = now_ms; t.hits += 1; t.velocity = None;
                    continue;
                }
                self.next_id += 1;
                self.heroes.push(HeroSighting { id: self.next_id, ally, pos: *d, seen_ms: now_ms, hits: 1, velocity: None });
            }
            // Drop the stalest extras beyond the team size.
            let mut team: Vec<usize> = (0..self.heroes.len()).filter(|i| self.heroes[*i].ally == ally).collect();
            if team.len() > cap {
                team.sort_by_key(|i| std::cmp::Reverse((self.heroes[*i].seen_ms, self.heroes[*i].hits)));
                let drop: Vec<u32> = team[cap..].iter().map(|i| self.heroes[*i].id).collect();
                self.heroes.retain(|h| !drop.contains(&h.id));
            }
        }
        let an = Analyzer::new();
        self.heroes.iter().map(|h| {
            let unseen_ms = now_ms.saturating_sub(h.seen_ms);
            let lane = an.project((h.pos.x * CW, h.pos.y * CH)).filter(|(_, _, d)| *d <= 14.0)
                .map(|(k, _, _)| lane_of(k, raw.lanes_mirrored.unwrap_or(false)));
            coach_core::HeroTrack { id: h.id, ally: h.ally, pos: h.pos, unseen_ms, hits: h.hits,
                velocity: if unseen_ms == 0 { h.velocity } else { None }, lane }
        }).collect()
    }

    pub fn observe(&mut self, mut raw: MinimapState, now_ms: u64) -> MinimapState {
        if self.history.back().map(|(t, _)| now_ms < *t || now_ms - *t > 20000).unwrap_or(false) {
            self.history.clear();
            self.heroes.clear();
            self.midboss = MidbossState::default();
        }
        // Lane layout: one per match. Majority of the frames' clear colour evidence (plus zone
        // label evidence below); reset only by a long gap.
        if now_ms < self.layout_votes.2 || now_ms - self.layout_votes.2 > LAYOUT_RESET_MS { self.layout_votes = (0, 0, now_ms); }
        if let Some(m) = raw.lanes_mirrored_frame { self.vote_layout(m, 1, now_ms); }
        if let Some(l) = self.layout() {
            if raw.lanes_mirrored.is_some_and(|m| m != l) {
                // Analyzed under the other layout (before the latch, or by a stateless analyzer):
                // the side lanes' fields belong to each other; their fronts used the wrong hue.
                std::mem::swap(&mut raw.green, &mut raw.yellow);
                raw.green.push = None; raw.yellow.push = None;
                raw.your_lane = raw.your_lane.map(|x| match x { coach_core::Lane::Green => coach_core::Lane::Yellow, coach_core::Lane::Yellow => coach_core::Lane::Green, b => b });
            }
            raw.lanes_mirrored = Some(l);
        }
        raw.heroes = self.track_heroes(&raw, now_ms);
        self.history.retain(|(t, _)| now_ms.saturating_sub(*t) <= 30000);
        self.history.push_back((now_ms, raw.clone()));
        while self.history.len() > 3 { self.history.pop_front(); }
        raw.observations = self.history.len() as u32;
        // Confirm the marker against the last sighting within 10 s, not strictly the previous
        // frame: one missed frame used to drop two (rec3: 64 frames without a position).
        let own_confirmed = raw.you.is_some_and(|p| self.last_you.is_some_and(|(t, old)| now_ms >= t && now_ms - t <= 10_000 && (p.x-old.x).hypot(p.y-old.y)<0.30));
        if let Some(p) = raw.you { self.last_you = Some((now_ms, p)); }
        if !own_confirmed { raw.you=None; raw.your_lane=None; raw.your_progress=None; }
        let an = Analyzer::new();
        let layout = raw.lanes_mirrored.unwrap_or(false);
        let lane_k = |l: coach_core::Lane| (0..3).find(|k| lane_of(*k, layout) == l).unwrap_or(1);
        let canon = |p: Pt| (p.x * CW, p.y * CH);
        // Carrying the urn replaces your marker with the vase (and your view cone): with your
        // marker gone, a vase where you just were is you (rec5 lost you for 30 frames of a carry).
        // A vase lying at a bridge spawn is only you when the HUD says you carry it.
        let mut urn_is_you = false;
        // The HUD's "DELIVER THE URN" prompt (set by the scene scanner) is proof on its own.
        let hud_carry = raw.urn == Some(UrnState::CarriedByYou);
        if raw.you.is_none() {
            let at_spawn = |p: Pt| BRIDGES.iter().any(|b| dist(*b, canon(p)) < 12.0);
            let anchor = self.urn_you.filter(|(t, _)| now_ms >= *t && now_ms - t <= 8000)
                .or(self.last_you.filter(|(t, _)| now_ms >= *t && now_ms - t <= 6000));
            let marks: Vec<Pt> = raw.urn_pos.into_iter().chain(raw.urn_marks.iter().copied()).filter(|p| hud_carry || !at_spawn(*p)).collect();
            let pick = match anchor {
                Some((t, a)) => {
                    let gate = (0.06 + 0.04 * (now_ms - t) as f32 / 1000.0).min(0.25);
                    marks.iter().copied().map(|p| ((p.x - a.x).hypot(p.y - a.y), p)).filter(|(d, _)| *d <= gate || hud_carry)
                        .min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, p)| p)
                }
                None => if hud_carry && marks.len() == 1 { Some(marks[0]) } else { None },
            };
            if let Some(p) = pick {
                raw.you = Some(p); raw.urn_pos = Some(p); raw.urn = Some(UrnState::CarriedByYou);
                self.urn_you = Some((now_ms, p)); self.last_you = Some((now_ms, p)); urn_is_you = true;
                if let Some((k, progress, d)) = an.project(canon(p)) { if d <= 24.0 { raw.your_lane = Some(lane_of(k, layout)); raw.your_progress = Some(progress); } }
            }
        } else { self.urn_you = None; }
        // Hold the lane through a short gap while you stay close to where it was last confirmed
        // (lane edges, portraits over the line); a respawn or a walk into the jungle ends it.
        match (raw.you, raw.your_lane) {
            (Some(p), Some(l)) => self.last_lane = Some((now_ms, p, l, raw.your_progress)),
            (Some(p), None) => if let Some((t, q, l, prog)) = self.last_lane {
                if now_ms >= t && now_ms - t <= 8000 && (p.x-q.x).hypot(p.y-q.y) < 0.05 { raw.your_lane = Some(l); raw.your_progress = prog; }
            },
            _ => {}
        }
        if let Some(p) = raw.you { self.confirmed_you = Some((now_ms, p)); }
        // Zone label: a read of a known name is accepted at once, anything else needs a second
        // identical read. Held while you stay near where it was read.
        let read_now = raw.zone.take();
        if let Some(z) = read_now.clone() {
            let same = self.zone.as_ref().is_some_and(|(_, c, _)| *c == z);
            let repeated = self.zone_pending.as_ref().is_some_and(|(t, p)| *p == z && now_ms >= *t && now_ms - t <= 8000);
            if crate::zone::is_known(&z) || same || repeated {
                let at = if same { self.zone.as_ref().and_then(|z| z.2).or(raw.you) } else { raw.you };
                self.zone = Some((now_ms, z, at)); self.zone_pending = None;
            } else { self.zone_pending = Some((now_ms, z)); }
        }
        if let Some((t, _, at)) = &self.zone {
            // Without any recent position of yours the label is held only briefly.
            let pos = raw.you.or(self.last_you.filter(|(lt, _)| now_ms >= *lt && now_ms - lt <= 6000).map(|(_, p)| p));
            let moved = pos.zip(*at).is_some_and(|(p, q)| (p.x - q.x).hypot(p.y - q.y) > 0.08);
            let hold = if pos.is_some() { ZONE_HOLD_MS } else { 4000 };
            if now_ms < *t || now_ms - t > hold || moved { self.zone = None; }
            else if at.is_none() { if let Some(z) = self.zone.as_mut() { z.2 = raw.you; } }
        }
        raw.zone = self.zone.as_ref().map(|z| z.1.clone());
        raw.zone_lane = raw.zone.as_deref().and_then(crate::zone::lane_of_prefix);
        if let (Some(z), Some(p)) = (read_now.as_deref().filter(|z| raw.zone.as_deref() == Some(*z)), raw.you) {
            // Independent layout evidence: the label names the lane you stand on, whose side
            // (left / right polyline) depends on the layout; a base label names whose base.
            let pc = canon(p);
            if let Some(l) = crate::zone::lane_of_prefix(z).filter(|l| *l != coach_core::Lane::Blue) {
                if let Some((k, _, d)) = an.project(pc) { if k != 1 && d <= 24.0 { self.vote_layout((l == coach_core::Lane::Yellow) == (k == 0), 2, now_ms); } }
            }
            let hidden = z.starts_with("HIDDEN KING"); let arch = z.starts_with("ARCHMOTHER");
            if (hidden || arch) && (p.y > 0.75 || p.y < 0.25) { self.vote_layout(hidden == (p.y > 0.75), 2, now_ms); }
        }
        raw.zone_area = raw.zone.as_deref().and_then(|z| crate::zone::area_of(z, self.layout().or(raw.lanes_mirrored)));
        if raw.zone_area == Some(coach_core::ZoneArea::Lane) {
            if let Some(p) = raw.you { if an.project(canon(p)).is_none_or(|(_, _, d)| d > 30.0) { raw.zone_area = Some(coach_core::ZoneArea::Jungle); } }
        }
        match (raw.zone_area, raw.zone_lane) {
            // At a base you are not in a lane (the lanes converge there, so the line test says one).
            (Some(coach_core::ZoneArea::OwnBase | coach_core::ZoneArea::EnemyBase | coach_core::ZoneArea::Base), _) => { raw.your_lane = None; raw.your_progress = None; }
            (Some(area), Some(zl)) if area != coach_core::ZoneArea::Jungle => match raw.you {
                // The label settles which lane you are on; progress from that lane's line.
                Some(p) => { let (d, prog) = an.project_lane(lane_k(zl), canon(p)); if d <= 30.0 { raw.your_lane = Some(zl); raw.your_progress = Some(prog); } }
                // Marker not found (covered, shop, effects): the label still names your lane.
                None => { raw.your_lane = Some(zl); raw.your_progress = None; }
            },
            _ => {}
        }
        let mb = &mut self.midboss;
        if raw.midboss_up {
            // A real kill keeps it away at least 5 minutes (respawn 7 / 6 / 5 min); a shorter
            // absence was a covered icon (rec3 announced a return 8 s after a phantom kill at 1:23).
            if let Some(d) = mb.down_at { if now_ms.saturating_sub(d) >= MIDBOSS_MIN_RESPAWN_MS { mb.back_at = Some(now_ms); } mb.down_at = None; }
            mb.seen = true; mb.absent_since = None;
        } else if mb.seen {
            let since = *mb.absent_since.get_or_insert(now_ms);
            if mb.down_at.is_none() && now_ms.saturating_sub(since) >= MIDBOSS_GONE_MS { mb.down_at = Some(now_ms); mb.back_at = None; }
        }
        raw.midboss_up = mb.seen && mb.down_at.is_none();
        raw.midboss_down_ms = mb.down_at.map(|t| now_ms.saturating_sub(t));
        raw.midboss_back_ms = mb.back_at.map(|t| now_ms.saturating_sub(t));
        for k in 0..3 {
            let values: Vec<f32> = self.history.iter().filter_map(|(_, m)| match k {
                0 => m.green.push, 1 => m.blue.push, _ => m.yellow.push,
            }).collect();
            let lane = match k { 0 => &mut raw.green, 1 => &mut raw.blue, _ => &mut raw.yellow };
            lane.push = if values.len() == 3 {
                let lo = values.iter().copied().fold(f32::INFINITY, f32::min);
                let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                (hi - lo <= 0.20).then(|| values.iter().sum::<f32>() / values.len() as f32)
            } else { None };
            // Trooper dots flicker under rings and pings (counts jumped 0-5-0 between frames):
            // report the median of the recent reads.
            let median = |f: fn(&coach_core::LaneState) -> u32| {
                let mut v: Vec<u32> = self.history.iter().map(|(_, m)| f(match k { 0 => &m.green, 1 => &m.blue, _ => &m.yellow })).collect();
                v.sort_unstable(); v[v.len() / 2]
            };
            let (a, e) = (median(|l| l.ally_troopers), median(|l| l.enemy_troopers));
            let lane = match k { 0 => &mut raw.green, 1 => &mut raw.blue, _ => &mut raw.yellow };
            lane.ally_troopers = a; lane.enemy_troopers = e;
        }
        // Require nearby urn positions on consecutive frames. Movement together with
        // a ring of the same team is evidence of carrying, rather than a single proximity.
        // (A vase established as you above is already tracked through your own positions.)
        if let Some(pos) = raw.urn_pos.filter(|_| !urn_is_you) {
            let previous = self.history.iter().rev().nth(1).and_then(|(_, m)| m.urn_pos);
            let supported = previous.map(|p| dist((p.x * CW, p.y * CH), (pos.x * CW, pos.y * CH)) < 90.0).unwrap_or(false);
            // An unsupported vase position is dropped, but not the HUD-proven carry (rec8 27:27
            // and 27:41 lost "you carry the urn" while the prompt was on screen).
            if !supported { raw.urn = hud_carry.then_some(UrnState::CarriedByYou); raw.urn_pos = None; }
            else if raw.urn != Some(UrnState::CarriedByYou) {
                let prev = &self.history[self.history.len() - 2].1;
                let moved = previous.map(|p| dist((p.x * CW, p.y * CH), (pos.x * CW, pos.y * CH)) > 4.0).unwrap_or(false);
                let near = |m: &MinimapState, p: Pt, enemy: bool| {
                    let heroes = if enemy { &m.enemies } else { &m.allies };
                    heroes.iter().any(|h| dist((h.x * CW, h.y * CH), (p.x * CW, p.y * CH)) < 16.0)
                };
                if moved && near(&raw, pos, true) && near(prev, previous.unwrap(), true) {
                    raw.urn = Some(UrnState::CarriedByEnemy);
                } else if moved && near(&raw, pos, false) && near(prev, previous.unwrap(), false) {
                    raw.urn = Some(UrnState::CarriedByAlly);
                } else {
                    // Only call a stationary vase "ground" at one of the known bridge
                    // spawn positions; movement elsewhere remains ambiguous.
                    let at_spawn = [(68.0, 175.0), (259.0, 175.0)].iter()
                        .any(|p| dist(*p, (pos.x * CW, pos.y * CH)) < 12.0);
                    raw.urn = Some(if !moved && at_spawn { UrnState::Ground } else { UrnState::Visible });
                }
            }
        }
        raw
    }
}
