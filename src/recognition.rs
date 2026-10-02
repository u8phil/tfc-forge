//! Local, dependency-free recognition of the TerraFirmaCraft anvil panel from a
//! screenshot.
//!
//! This is a faithful Rust port of the original TypeScript `src/recognition.ts`.
//! The implementation is derived from the canonical GUI geometry in
//! AnvilScreen.java / ForgeStep.java (TFC v3.2.25) and validated against the
//! bundled fixtures. No network, OCR service or native platform call is used:
//! everything runs on the raw RGBA pixel buffer.
//!
//! Detection geometry, thresholds, confidence math, warnings and fixture
//! outputs are preserved exactly from the TypeScript source.

use std::collections::HashSet;

use crate::domain::{ForgeRule, LIMIT, MIN_WORK, RuleOrder, RuleType};

/// Maximum supported integer GUI scale.
const MAX_SCALE: i32 = 5;
/// Rule-glyph icon size in GUI pixels.
const ICON_SIZE: i32 = 10;
/// Order-ladder overlay width in GUI pixels.
const OVERLAY_SIZE: i32 = 20;
/// Horizontal center of a 5px value marker minus its left edge.
const MARKER_OFFSET: f64 = 15.5;

const ACCEPT_GLYPH_SCORE: f64 = 0.5;
const ACCEPT_GLYPH_MARGIN: f64 = 0.04;
const ACCEPT_PANEL_VIVID: f64 = 0.55;

/// Glyph descriptor grid resolution.
const GLYPH_GRID: usize = 14;

/// Failure modes for constructing an [`RgbaImage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecognitionError {
    #[error(
        "RGBA buffer length {actual} does not match width*height*4 ({expected}) for a {width}x{height} image"
    )]
    InvalidBuffer {
        width: usize,
        height: usize,
        expected: usize,
        actual: usize,
    },
    #[error("image dimensions {width}x{height} overflow addressable memory")]
    DimensionOverflow { width: usize, height: usize },
}

/// A decoded RGBA8 pixel buffer, independent of any UI or image framework.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl RgbaImage {
    /// Construct a buffer, rejecting any length other than `width * height * 4`.
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Result<Self, RecognitionError> {
        let expected = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(RecognitionError::DimensionOverflow { width, height })?;
        if data.len() != expected {
            return Err(RecognitionError::InvalidBuffer {
                width,
                height,
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            data,
            width,
            height,
        })
    }
}

impl TryFrom<image::RgbaImage> for RgbaImage {
    type Error = RecognitionError;

    fn try_from(image: image::RgbaImage) -> Result<Self, Self::Error> {
        let (width, height) = image.dimensions();
        RgbaImage::new(width as usize, height as usize, image.into_raw())
    }
}

impl TryFrom<&image::RgbaImage> for RgbaImage {
    type Error = RecognitionError;

    fn try_from(image: &image::RgbaImage) -> Result<Self, Self::Error> {
        let (width, height) = image.dimensions();
        RgbaImage::new(width as usize, height as usize, image.as_raw().clone())
    }
}

/// Screen-space location of the canonical anvil panel grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelLocation {
    /// Screen-space pixel origin of the canonical panel grid.
    pub x: i32,
    pub y: i32,
    /// Integer GUI scale, 1..=5.
    pub scale: i32,
    /// Detection score, higher is better (roughly 0..1).
    pub score: f32,
}

/// Per-slot recognition detail, including confidence for unavailable fields.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleField {
    /// Which of the three rule slots this describes (0-based).
    pub slot: usize,
    pub rule_type: Option<RuleType>,
    pub order: Option<RuleOrder>,
    pub type_confidence: f64,
    pub order_confidence: f64,
    pub note: Option<String>,
}

/// Confidence for each recognized field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldConfidence {
    pub start: f64,
    pub target: f64,
    pub panel: f64,
    pub rules: f64,
}

/// The complete recognizer output for one screenshot.
#[derive(Clone, Debug, PartialEq)]
pub struct RecognitionResult {
    pub start: Option<i32>,
    pub target: Option<i32>,
    /// Recognized rules, in slot order. Between 1 and 3 entries on success.
    pub rules: Vec<ForgeRule>,
    /// Per-slot detail including confidence for unavailable fields.
    pub rule_fields: Vec<RuleField>,
    pub panel: Option<PanelLocation>,
    pub confidence: FieldConfidence,
    pub warnings: Vec<String>,
}

/// A canonical action-button anchor in GUI pixels.
#[derive(Clone, Copy, Debug)]
struct Point {
    x: i32,
    y: i32,
    rule_type: RuleType,
}

/// Canonical action-button anchors (16x16 each).
const BUTTONS: [Point; 8] = [
    Point {
        x: 53,
        y: 50,
        rule_type: RuleType::Hit,
    },
    Point {
        x: 71,
        y: 50,
        rule_type: RuleType::Hit,
    },
    Point {
        x: 89,
        y: 50,
        rule_type: RuleType::Punch,
    },
    Point {
        x: 107,
        y: 50,
        rule_type: RuleType::Bend,
    },
    Point {
        x: 53,
        y: 68,
        rule_type: RuleType::Hit,
    },
    Point {
        x: 71,
        y: 68,
        rule_type: RuleType::Draw,
    },
    Point {
        x: 89,
        y: 68,
        rule_type: RuleType::Upset,
    },
    Point {
        x: 107,
        y: 68,
        rule_type: RuleType::Shrink,
    },
];

/// Read an RGB triple at `(x, y)`, or `None` when out of bounds.
fn at(image: &RgbaImage, x: i32, y: i32) -> Option<[u8; 3]> {
    if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 {
        return None;
    }
    let i = ((y as usize) * image.width + x as usize) * 4;
    Some([image.data[i], image.data[i + 1], image.data[i + 2]])
}

fn is_vivid(p: Option<[u8; 3]>) -> bool {
    match p {
        None => false,
        Some([r, g, b]) => {
            let mx = i32::from(r.max(g).max(b));
            let mn = i32::from(r.min(g).min(b));
            mx - mn > 55 && mx > 95
        }
    }
}

fn is_orange(p: Option<[u8; 3]>) -> bool {
    match p {
        None => false,
        Some([r, g, b]) => {
            let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
            r > 165 && g > 40 && g < 205 && b < 90 && r - b > 90
        }
    }
}

fn is_green_core(p: Option<[u8; 3]>) -> bool {
    match p {
        None => false,
        Some([r, g, b]) => {
            let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
            g >= 140 && g - r >= 55 && g - b >= 55
        }
    }
}

/// A lit order-ladder rung is tinted orange when the rule is unmet and green
/// when the current history already satisfies it; unlit rungs stay grey.
fn is_lit_rung(p: Option<[u8; 3]>) -> bool {
    match p {
        None => false,
        Some([r, g, b]) => {
            let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
            is_orange(p) || (g >= 120 && g - r >= 45 && g - b >= 45)
        }
    }
}

fn is_red_core(p: Option<[u8; 3]>) -> bool {
    match p {
        None => false,
        Some([r, g, b]) => {
            let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
            r >= 165 && r - g >= 70 && r - b >= 70
        }
    }
}

/// Bright, near-greyscale pixels: the white glyph art drawn over the button.
fn is_glyph(p: Option<[u8; 3]>) -> bool {
    match p {
        None => false,
        Some([r, g, b]) => {
            let mx = i32::from(r.max(g).max(b));
            let mn = i32::from(r.min(g).min(b));
            mn > 150 && mx - mn < 80
        }
    }
}

/// An 8-connected component bounding box and area.
#[derive(Clone, Copy, Debug)]
struct Component {
    minx: i32,
    miny: i32,
    maxx: i32,
    w: i32,
    h: i32,
    area: i64,
}

/// Connected-component labelling over a predicate, 8-connected.
fn components<F>(width: i32, height: i32, test: F) -> Vec<Component>
where
    F: Fn(i32, i32) -> bool,
{
    let mut seen = vec![false; (i64::from(width) * i64::from(height)) as usize];
    let mut out: Vec<Component> = Vec::new();
    let mut stack: Vec<i64> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let p = i64::from(y) * i64::from(width) + i64::from(x);
            if seen[p as usize] || !test(x, y) {
                continue;
            }
            seen[p as usize] = true;
            stack.clear();
            stack.push(p);
            let mut minx = x;
            let mut maxx = x;
            let mut miny = y;
            let mut maxy = y;
            let mut area: i64 = 0;
            while let Some(q) = stack.pop() {
                let qx = (q % i64::from(width)) as i32;
                let qy = (q / i64::from(width)) as i32;
                area += 1;
                minx = minx.min(qx);
                maxx = maxx.max(qx);
                miny = miny.min(qy);
                maxy = maxy.max(qy);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = qx + dx;
                        let ny = qy + dy;
                        if nx < 0 || ny < 0 || nx >= width || ny >= height {
                            continue;
                        }
                        let np = (ny * width + nx) as usize;
                        if seen[np] || !test(nx, ny) {
                            continue;
                        }
                        seen[np] = true;
                        stack.push((ny * width + nx) as i64);
                    }
                }
            }
            out.push(Component {
                minx,
                miny,
                maxx,
                w: maxx - minx + 1,
                h: maxy - miny + 1,
                area,
            });
        }
    }
    out
}

fn vivid_fraction(image: &RgbaImage, x: i32, y: i32, w: i32, h: i32) -> f64 {
    let mut hits: u64 = 0;
    let mut total: u64 = 0;
    let mut yy = y;
    while yy < y + h {
        let mut xx = x;
        while xx < x + w {
            total += 1;
            if is_vivid(at(image, xx, yy)) {
                hits += 1;
            }
            xx += 1;
        }
        yy += 1;
    }
    if total > 0 {
        hits as f64 / total as f64
    } else {
        0.0
    }
}

fn orange_fraction(image: &RgbaImage, x: i32, y: i32, w: i32, h: i32) -> f64 {
    let mut hits: u64 = 0;
    let mut total: u64 = 0;
    let mut yy = y;
    while yy < y + h {
        let mut xx = x;
        while xx < x + w {
            total += 1;
            if is_orange(at(image, xx, yy)) {
                hits += 1;
            }
            xx += 1;
        }
        yy += 1;
    }
    if total > 0 {
        hits as f64 / total as f64
    } else {
        0.0
    }
}

/// A decoded value-marker position and its confidence.
#[derive(Clone, Copy, Debug)]
struct MarkerReading {
    value: i32,
    confidence: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkerKind {
    Current,
    Target,
}

fn detect_marker(
    image: &RgbaImage,
    ox: i32,
    oy: i32,
    scale: i32,
    kind: MarkerKind,
) -> Option<MarkerReading> {
    // The current (green) marker sits six GUI pixels below the target (red) marker.
    let (y0, y1) = match kind {
        MarkerKind::Current => (98, 105),
        MarkerKind::Target => (92, 100),
    };
    let gx0 = 10;
    let gx1 = 168;
    let sx0 = ox + gx0 * scale;
    let sy0 = oy + y0 * scale;
    let w = (gx1 - gx0) * scale;
    let h = (y1 - y0) * scale;
    if w <= 0 || h <= 0 {
        return None;
    }

    let test = |x: i32, y: i32| {
        let p = at(image, sx0 + x, sy0 + y);
        match kind {
            MarkerKind::Current => is_green_core(p),
            MarkerKind::Target => is_red_core(p),
        }
    };

    let mut candidates: Vec<Component> = components(w, h, test)
        .into_iter()
        .filter(|c| c.area >= i64::from(2.max(scale)) && c.w <= 6 * scale && c.h <= 6 * scale)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.area));
    let c = candidates[0];

    let center_screen = (i64::from(c.minx) + i64::from(c.maxx) + 1) as f64 / 2.0 + f64::from(sx0);
    let gui_center = (center_screen - f64::from(ox)) / f64::from(scale);
    let value = (gui_center - MARKER_OFFSET + 0.5).floor() as i32;
    if !(MIN_WORK..=LIMIT).contains(&value) {
        return None;
    }

    let confidence = (c.area as f64 / (4.0 * f64::from(scale)) + 0.4).min(1.0);
    Some(MarkerReading { value, confidence })
}

struct PanelHypothesis {
    ox: i32,
    oy: i32,
    scale: i32,
    score: f64,
    vivid: f64,
    current: Option<MarkerReading>,
    target: Option<MarkerReading>,
}

fn score_hypothesis(image: &RgbaImage, ox: i32, oy: i32, scale: i32) -> PanelHypothesis {
    let mut vivid = 0.0;
    for button in &BUTTONS {
        vivid += vivid_fraction(
            image,
            ox + button.x * scale,
            oy + button.y * scale,
            16 * scale,
            16 * scale,
        );
    }
    vivid /= BUTTONS.len() as f64;

    let mut orange = 0;
    for i in 0..3 {
        let fraction = orange_fraction(
            image,
            ox + (59 + 19 * i) * scale,
            oy + 7 * scale,
            OVERLAY_SIZE * scale,
            22 * scale,
        );
        if fraction > 0.02 {
            orange += 1;
        }
    }

    let current = detect_marker(image, ox, oy, scale, MarkerKind::Current);
    let target = detect_marker(image, ox, oy, scale, MarkerKind::Target);
    let score = vivid * 0.6
        + (f64::from(orange) / 3.0) * 0.2
        + if current.is_some() { 0.1 } else { 0.0 }
        + if target.is_some() { 0.1 } else { 0.0 };
    PanelHypothesis {
        ox,
        oy,
        scale,
        score,
        vivid,
        current,
        target,
    }
}

fn locate_panel(image: &RgbaImage) -> Option<PanelHypothesis> {
    let candidates: Vec<Component> = components(image.width as i32, image.height as i32, |x, y| {
        is_vivid(at(image, x, y))
    })
    .into_iter()
    .filter(|c| {
        c.area >= 30
            && c.w >= 8
            && c.h >= 8
            && c.w <= 16 * MAX_SCALE + 12
            && c.h <= 16 * MAX_SCALE + 12
            && f64::from(c.w) / f64::from(c.h) > 0.7
            && f64::from(c.w) / f64::from(c.h) < 1.4
    })
    .collect();

    let mut hypotheses: Vec<PanelHypothesis> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for c in candidates {
        let scale = ((f64::from(c.w) / 16.0 + f64::from(c.h) / 16.0) / 2.0 + 0.5).floor() as i32;
        if !(1..=MAX_SCALE).contains(&scale) {
            continue;
        }
        if (c.w - 16 * scale).abs() > scale + 2 || (c.h - 16 * scale).abs() > scale + 2 {
            continue;
        }
        for button in &BUTTONS {
            let ox = c.minx - scale * button.x;
            let oy = c.miny - scale * button.y;
            let key = format!("{scale}:{ox}:{oy}");
            if !seen.insert(key) {
                continue;
            }
            let hypothesis = score_hypothesis(image, ox, oy, scale);
            if hypothesis.current.is_some()
                && hypothesis.target.is_some()
                && hypothesis.vivid > ACCEPT_PANEL_VIVID
            {
                hypotheses.push(hypothesis);
            }
        }
    }
    if hypotheses.is_empty() {
        return None;
    }
    hypotheses.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hypotheses.into_iter().next()
}

/// Binary 0/1 mask.
type BinaryMask = Vec<Vec<u8>>;
/// Floating-point resampled grid.
type Grid = Vec<Vec<f64>>;
/// Axis-aligned bounding box `(x0, y0, x1, y1)`, inclusive.
type BBox = (usize, usize, usize, usize);

#[allow(clippy::too_many_arguments)]
fn glyph_mask(
    image: &RgbaImage,
    ox: i32,
    oy: i32,
    scale: i32,
    gx: i32,
    gy: i32,
    w: i32,
    h: i32,
) -> BinaryMask {
    let mut mask: BinaryMask = Vec::with_capacity((h * scale) as usize);
    for y in 0..h * scale {
        let mut row: Vec<u8> = Vec::with_capacity((w * scale) as usize);
        for x in 0..w * scale {
            row.push(u8::from(is_glyph(at(
                image,
                ox + gx * scale + x,
                oy + gy * scale + y,
            ))));
        }
        mask.push(row);
    }
    mask
}

fn mask_components(mask: &BinaryMask) -> Vec<Vec<(usize, usize)>> {
    let h = mask.len();
    let w = mask.first().map_or(0, |row| row.len());
    let mut seen = vec![vec![false; w]; h];
    let mut out: Vec<Vec<(usize, usize)>> = Vec::new();
    for (y, row) in mask.iter().enumerate() {
        for (x, &cell) in row.iter().enumerate() {
            if seen[y][x] || cell == 0 {
                continue;
            }
            seen[y][x] = true;
            let mut stack: Vec<(usize, usize)> = vec![(x, y)];
            let mut cells: Vec<(usize, usize)> = Vec::new();
            while let Some((cx, cy)) = stack.pop() {
                cells.push((cx, cy));
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let (nx, ny) = (nx as usize, ny as usize);
                        if seen[ny][nx] || mask[ny][nx] == 0 {
                            continue;
                        }
                        seen[ny][nx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            out.push(cells);
        }
    }
    out.sort_by_key(|cells| std::cmp::Reverse(cells.len()));
    out
}

fn mask_bbox(mask: &BinaryMask) -> Option<BBox> {
    let mut x0 = usize::MAX;
    let mut y0 = usize::MAX;
    let mut x1: Option<usize> = None;
    let mut y1: Option<usize> = None;
    for (y, row) in mask.iter().enumerate() {
        for (x, &cell) in row.iter().enumerate() {
            if cell == 0 {
                continue;
            }
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = Some(x1.map_or(x, |cur| cur.max(x)));
            y1 = Some(y1.map_or(y, |cur| cur.max(y)));
        }
    }
    match (x1, y1) {
        (Some(x1), Some(y1)) => Some((x0, y0, x1, y1)),
        _ => None,
    }
}

fn mask_from_cells(shape: (usize, usize), cells: &[(usize, usize)]) -> BinaryMask {
    let mut mask: BinaryMask = vec![vec![0; shape.1]; shape.0];
    for &(x, y) in cells {
        mask[y][x] = 1;
    }
    mask
}

fn mask_from_bbox(mask: &BinaryMask, bbox: BBox) -> BinaryMask {
    let (x0, y0, x1, y1) = bbox;
    let mut out: BinaryMask = Vec::with_capacity(y1 - y0 + 1);
    for row in mask.iter().take(y1 + 1).skip(y0) {
        out.push(row[x0..=x1].to_vec());
    }
    out
}

/// Resample a cropped mask onto an NxN grid using area averaging.
fn resample(mask: &BinaryMask, size: usize) -> Grid {
    let h = mask.len();
    let w = mask.first().map_or(0, |row| row.len());
    let mut out: Grid = Vec::with_capacity(size);
    for y in 0..size {
        let mut row: Vec<f64> = Vec::with_capacity(size);
        for x in 0..size {
            let sx0 = (x as f64 / size as f64) * w as f64;
            let sx1 = ((x + 1) as f64 / size as f64) * w as f64;
            let sy0 = (y as f64 / size as f64) * h as f64;
            let sy1 = ((y + 1) as f64 / size as f64) * h as f64;
            let mut sum = 0.0;
            let mut count = 0usize;
            let mut yy = sy0.floor() as i64;
            let y_end = sy1.ceil() as i64;
            while yy < y_end {
                let mut xx = sx0.floor() as i64;
                let x_end = sx1.ceil() as i64;
                while xx < x_end {
                    if yy >= 0 && yy < h as i64 && xx >= 0 && xx < w as i64 {
                        sum += mask[yy as usize][xx as usize] as f64;
                        count += 1;
                    }
                    xx += 1;
                }
                yy += 1;
            }
            row.push(if count > 0 { sum / count as f64 } else { 0.0 });
        }
        out.push(row);
    }
    out
}

fn cosine(a: &Grid, b: &Grid) -> f64 {
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for (row_a, row_b) in a.iter().zip(b.iter()) {
        for (&va, &vb) in row_a.iter().zip(row_b.iter()) {
            dot += va * vb;
            na += va * va;
            nb += vb * vb;
        }
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-9 { 0.0 } else { dot / denom }
}

struct GlyphDescriptor {
    full: Grid,
    icon: Grid,
}

fn describe_glyph(mask: &BinaryMask) -> GlyphDescriptor {
    let comps = mask_components(mask);
    let shape = (mask.len(), mask.first().map_or(0, |row| row.len()));
    let empty = || vec![vec![0.0; GLYPH_GRID]; GLYPH_GRID];
    let Some(full_box) = mask_bbox(mask) else {
        return GlyphDescriptor {
            full: empty(),
            icon: empty(),
        };
    };
    let icon_mask = if comps.is_empty() {
        mask.clone()
    } else {
        mask_from_cells(shape, &comps[0])
    };
    let icon_box = mask_bbox(&icon_mask);
    GlyphDescriptor {
        full: resample(&mask_from_bbox(mask, full_box), GLYPH_GRID),
        icon: resample(
            &mask_from_bbox(&icon_mask, icon_box.unwrap_or(full_box)),
            GLYPH_GRID,
        ),
    }
}

/// Glyph templates indexed by rule type, preserving button insertion order.
struct GlyphTemplates {
    by_type: Vec<(RuleType, Vec<GlyphDescriptor>)>,
}

/// Builds rule-glyph templates from the action buttons in the SAME screenshot.
fn build_glyph_templates(image: &RgbaImage, ox: i32, oy: i32, scale: i32) -> GlyphTemplates {
    let mut by_type: Vec<(RuleType, Vec<GlyphDescriptor>)> = Vec::new();
    for button in &BUTTONS {
        let mask = glyph_mask(image, ox, oy, scale, button.x + 1, button.y + 1, 14, 14);
        let descriptor = describe_glyph(&mask);
        if let Some(entry) = by_type
            .iter_mut()
            .find(|(rule_type, _)| *rule_type == button.rule_type)
        {
            entry.1.push(descriptor);
        } else {
            by_type.push((button.rule_type, vec![descriptor]));
        }
    }
    GlyphTemplates { by_type }
}

struct GlyphMatch {
    rule_type: RuleType,
    score: f64,
    margin: f64,
    fallback: bool,
}

fn classify_glyph(templates: &GlyphTemplates, mask: &BinaryMask) -> GlyphMatch {
    let target = describe_glyph(mask);
    let mut best: Vec<(RuleType, f64)> = Vec::new();
    for (rule_type, list) in &templates.by_type {
        let mut top = 0.0;
        for tpl in list {
            let score =
                0.5 * cosine(&target.full, &tpl.full) + 0.5 * cosine(&target.icon, &tpl.icon);
            if score > top {
                top = score;
            }
        }
        best.push((*rule_type, top));
    }
    let mut ranked = best.clone();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let (rule_type, score) = ranked[0];
    let runner_up = ranked.get(1).map_or(0.0, |entry| entry.1);

    // Generic bare-hammer fallback: a hit glyph has no delta text, so if nothing
    // matches confidently but the icon resembles the hammer templates, call it a hit.
    let hit_score = best
        .iter()
        .find(|(candidate, _)| *candidate == RuleType::Hit)
        .map_or(0.0, |entry| entry.1);
    if rule_type != RuleType::Hit
        && score < ACCEPT_GLYPH_SCORE
        && hit_score >= ACCEPT_GLYPH_SCORE - 0.1
    {
        return GlyphMatch {
            rule_type: RuleType::Hit,
            score: hit_score,
            margin: hit_score - runner_up,
            fallback: true,
        };
    }
    GlyphMatch {
        rule_type,
        score,
        margin: score - runner_up,
        fallback: false,
    }
}

struct LadderReading {
    order: Option<RuleOrder>,
    confidence: f64,
}

fn read_ladder(image: &RgbaImage, ox: i32, oy: i32, scale: i32, slot: i32) -> LadderReading {
    let rx = ox + (59 + 19 * slot) * scale;
    let mut lit = [false; 3];
    for (rung, rung_lit) in lit.iter_mut().enumerate() {
        let ry = oy + (23 + 2 * rung as i32) * scale;
        let mut count = 0;
        for gx in 0..OVERLAY_SIZE {
            for dy in 0..scale {
                if is_lit_rung(at(image, rx + gx * scale, ry + dy)) {
                    count += 1;
                }
            }
        }
        *rung_lit = count > 3 * scale;
    }
    let [top, mid, bottom] = lit;
    let order = if top && mid && bottom {
        Some(RuleOrder::Any)
    } else if top && mid && !bottom {
        Some(RuleOrder::NotLast)
    } else if !top && !mid && bottom {
        Some(RuleOrder::Last)
    } else if !top && mid && !bottom {
        Some(RuleOrder::SecondLast)
    } else if top && !mid && !bottom {
        Some(RuleOrder::ThirdLast)
    } else {
        None
    };

    let confidence = if order.is_some() { 0.9 } else { 0.0 };
    LadderReading { order, confidence }
}

fn empty_rule_fields() -> Vec<RuleField> {
    (0..3)
        .map(|slot| RuleField {
            slot,
            rule_type: None,
            order: None,
            type_confidence: 0.0,
            order_confidence: 0.0,
            note: None,
        })
        .collect()
}

fn empty_result(rule_fields: Vec<RuleField>, warnings: Vec<String>) -> RecognitionResult {
    RecognitionResult {
        start: None,
        target: None,
        rules: Vec::new(),
        rule_fields,
        panel: None,
        confidence: FieldConfidence {
            start: 0.0,
            target: 0.0,
            panel: 0.0,
            rules: 0.0,
        },
        warnings,
    }
}

/// Recognize the current value, target and up to three rules from an anvil
/// screenshot. Fails closed per field: unavailable or low-confidence fields are
/// returned as `None` with a warning rather than a guess.
pub fn recognize_anvil(image: &RgbaImage) -> RecognitionResult {
    let mut warnings: Vec<String> = Vec::new();
    let mut rule_fields = empty_rule_fields();

    if image.width < 32 || image.height < 32 {
        warnings.push("Image is too small to contain an anvil panel.".to_string());
        return empty_result(rule_fields, warnings);
    }

    let Some(panel) = locate_panel(image) else {
        warnings
            .push("Could not locate the TFC anvil panel. Is the whole GUI visible?".to_string());
        return empty_result(rule_fields, warnings);
    };

    let location = PanelLocation {
        x: panel.ox,
        y: panel.oy,
        scale: panel.scale,
        score: panel.score as f32,
    };

    if panel.current.is_none() {
        warnings.push("Could not read the current (green) value marker.".to_string());
    }
    if panel.target.is_none() {
        warnings.push("Could not read the target (red) value marker.".to_string());
    }

    let templates = build_glyph_templates(image, panel.ox, panel.oy, panel.scale);
    let mut rules: Vec<ForgeRule> = Vec::new();
    let mut min_rule_confidence: f64 = 1.0;
    let mut rule_count = 0usize;

    for (slot, field) in rule_fields.iter_mut().enumerate() {
        let slot_i = slot as i32;
        let ladder = read_ladder(image, panel.ox, panel.oy, panel.scale, slot_i);
        let glyph = classify_glyph(
            &templates,
            &glyph_mask(
                image,
                panel.ox,
                panel.oy,
                panel.scale,
                64 + 19 * slot_i,
                10,
                ICON_SIZE,
                ICON_SIZE,
            ),
        );

        field.type_confidence = glyph.score;
        field.order_confidence = ladder.confidence;

        if glyph.fallback {
            field.note = Some("Matched by generic bare-hammer fallback".to_string());
        }

        let confident_type =
            glyph.score >= ACCEPT_GLYPH_SCORE && glyph.margin >= ACCEPT_GLYPH_MARGIN;
        if confident_type {
            field.rule_type = Some(glyph.rule_type);
        }
        field.order = ladder.order;

        let Some(order) = ladder.order else {
            warnings.push(format!(
                "Rule {}: could not read the order ladder.",
                slot + 1
            ));
            continue;
        };
        if !confident_type {
            warnings.push(format!(
                "Rule {}: low confidence rule glyph, skipped.",
                slot + 1
            ));
            continue;
        }

        rules.push(ForgeRule {
            rule_type: glyph.rule_type,
            order,
        });
        rule_count += 1;
        let combined = (glyph.score * 0.7 + glyph.margin * 4.0 + ladder.confidence * 0.3 - 0.5)
            .clamp(0.0, 1.0);
        min_rule_confidence = min_rule_confidence.min(combined);
    }

    if rule_count == 0 {
        warnings.push("No forge rules were recognized.".to_string());
    }

    RecognitionResult {
        start: panel.current.map(|marker| marker.value),
        target: panel.target.map(|marker| marker.value),
        rules,
        rule_fields,
        panel: Some(location),
        confidence: FieldConfidence {
            start: panel.current.map_or(0.0, |marker| marker.confidence),
            target: panel.target.map_or(0.0, |marker| marker.confidence),
            panel: panel.score,
            rules: if rule_count > 0 {
                min_rule_confidence
            } else {
                0.0
            },
        },
        warnings,
    }
}
