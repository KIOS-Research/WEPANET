//! Map layers of DXF drawings: the lines of a drawing layer with their
//! entities' attributes, kept with the map layer (and in the project file),
//! drawn in the network's coordinates (closed lines as filled polygons),
//! found by Identify and listed in an attribute table.

use std::sync::Arc;

use egui::{Color32, Pos2, Rect, Sense, Shape, Ui, pos2, vec2};

use crate::app::EpanetApp;
use crate::dxf::{Entity, Kind};
use crate::layers::{MapLayer, Service};

/// The lines of a DXF layer, in the network's coordinates, with the box of
/// each for skipping those out of view.
#[derive(Default)]
pub struct LineSet {
    pub lines: Vec<Vec<[f64; 2]>>,
    /// Each line's entity, in the order of `lines`.
    pub entities: Vec<Entity>,
    boxes: Vec<[f64; 4]>,
    /// Whether each line is closed (a polygon, filled when its layer's are).
    closed: Vec<bool>,
    pub has_polygons: bool,
    pub bounds: Option<[f64; 4]>,
}
/// Marks the project file's lines with their entities (older ones have only
/// the lines).
const MAGIC: &[u8; 4] = b"DXL2";
/// The attribute table's columns.
pub const COLUMNS: [&str; 9] = [
    "Type",
    "Handle",
    "Color",
    "Linetype",
    "Elevation",
    "Thickness",
    "Points",
    "Length",
    "Area",
];

fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> [f64; 4] {
    a.map_or(b, |a| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })
}

impl LineSet {
    pub fn new(lines: Vec<Vec<[f64; 2]>>, mut entities: Vec<Entity>) -> Self {
        entities.resize(lines.len(), Entity::default());
        let closed: Vec<bool> = lines.iter().map(|l| is_closed(l)).collect();
        let has_polygons = closed.contains(&true);
        let mut bounds: Option<[f64; 4]> = None;
        let boxes = lines
            .iter()
            .map(|l| {
                let b = l
                    .iter()
                    .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, &[x, y]| {
                        [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]
                    });
                if !l.is_empty() {
                    bounds = Some(union(bounds, b));
                }
                b
            })
            .collect();
        LineSet {
            lines,
            entities,
            boxes,
            closed,
            has_polygons,
            bounds,
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether its polygons are worth filling at first: small ones (parcels,
    /// buildings), not rings of many points (contours, boundaries), which a
    /// fill only clouds.
    pub fn fill_by_default(&self) -> bool {
        let (n, pts) = self
            .lines
            .iter()
            .zip(&self.closed)
            .filter(|(_, c)| **c)
            .fold((0usize, 0usize), |(n, p), (l, _)| (n + 1, p + l.len()));
        n > 0 && pts <= 200 * n
    }

    pub fn bounds_of(&self, k: usize) -> Option<[f64; 4]> {
        self.boxes.get(k).copied().filter(|b| b[0] <= b[2])
    }

    fn length(&self, k: usize) -> f64 {
        self.lines[k]
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum()
    }

    fn area(&self, k: usize) -> Option<f64> {
        let l = &self.lines[k];
        is_closed(l).then(|| {
            l.windows(2)
                .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
                .sum::<f64>()
                .abs()
                / 2.0
        })
    }

    /// The text of column `c` (see [`COLUMNS`]) for line `k`.
    pub fn cell(&self, k: usize, c: usize) -> String {
        let e = &self.entities[k];
        let num = |v: Option<f64>| v.map(crate::import::fmt).unwrap_or_default();
        match c {
            0 => e.kind.name().into(),
            1 => e.handle.clone(),
            2 => match e.color {
                None | Some(256) => "ByLayer".into(),
                Some(0) => "ByBlock".into(),
                Some(n) => n.to_string(),
            },
            3 => {
                if e.linetype.is_empty() {
                    "ByLayer".into()
                } else {
                    e.linetype.clone()
                }
            }
            4 => num(e.elevation),
            5 => num(e.thickness),
            6 => self.lines[k].len().to_string(),
            7 => format!("{:.2}", self.length(k)),
            8 => self.area(k).map(|a| format!("{a:.2}")).unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Line `k`'s attributes, by column name.
    pub fn fields(&self, k: usize) -> Vec<(String, String)> {
        COLUMNS
            .iter()
            .enumerate()
            .map(|(c, name)| (crate::i18n::tr(name).to_string(), self.cell(k, c)))
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }

    /// The lines at `at` (in map coordinates): those within `tol` of it,
    /// nearest first, then the polygons around it, smallest first; at most
    /// `max`.
    pub fn hits(&self, at: [f64; 2], tol: f64, max: usize) -> Vec<usize> {
        let mut near: Vec<(f64, usize)> = Vec::new();
        let mut around: Vec<(f64, usize)> = Vec::new();
        for (k, (l, b)) in self.lines.iter().zip(&self.boxes).enumerate() {
            if at[0] < b[0] - tol || at[0] > b[2] + tol || at[1] < b[1] - tol || at[1] > b[3] + tol
            {
                continue;
            }
            let d = l
                .windows(2)
                .map(|w| dist_to_segment(at, w[0], w[1]))
                .fold(f64::MAX, f64::min);
            if d <= tol {
                near.push((d, k));
            } else if is_closed(l) && contains(l, at) {
                around.push((self.area(k).unwrap_or(0.0), k));
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        around.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.into_iter()
            .chain(around)
            .map(|(_, k)| k)
            .take(max)
            .collect()
    }

    /// The lines in or across the box `b` (map coordinates), and the
    /// polygons around it; at most `max`.
    pub fn in_box(&self, b: [f64; 4], max: usize) -> Vec<usize> {
        let center = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        let inside = |p: [f64; 2]| p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3];
        let mut out = Vec::new();
        for (k, (l, bx)) in self.lines.iter().zip(&self.boxes).enumerate() {
            if bx[2] < b[0] || bx[0] > b[2] || bx[3] < b[1] || bx[1] > b[3] {
                continue;
            }
            let hit = l.iter().any(|&p| inside(p))
                || l.windows(2).any(|w| crosses_box(w[0], w[1], b))
                || (self.closed[k] && contains(l, center));
            if hit {
                out.push(k);
                if out.len() > max {
                    break;
                }
            }
        }
        out
    }

    /// The lines and their entities as bytes for the project file: a mark,
    /// the number of lines, for each the number of its points and their x, y,
    /// then for each its entity (little-endian numbers).
    pub fn to_bytes(&self) -> Vec<u8> {
        let n: usize = self.lines.iter().map(|l| 4 + 16 * l.len()).sum();
        let mut out = Vec::with_capacity(8 + n + 40 * self.entities.len());
        out.extend(MAGIC);
        out.extend((self.lines.len() as u32).to_le_bytes());
        for l in &self.lines {
            out.extend((l.len() as u32).to_le_bytes());
            for [x, y] in l {
                out.extend(x.to_le_bytes());
                out.extend(y.to_le_bytes());
            }
        }
        let text = |out: &mut Vec<u8>, s: &str| {
            out.extend((s.len() as u32).to_le_bytes());
            out.extend(s.as_bytes());
        };
        for e in &self.entities {
            out.push(e.kind as u8);
            out.extend(e.color.unwrap_or(i16::MIN).to_le_bytes());
            out.extend(e.elevation.unwrap_or(f64::NAN).to_le_bytes());
            out.extend(e.thickness.unwrap_or(f64::NAN).to_le_bytes());
            text(&mut out, &e.handle);
            text(&mut out, &e.linetype);
        }
        out
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let mut r = Reader { b, at: 0 };
        let with_entities = b.starts_with(MAGIC);
        if with_entities {
            r.at = MAGIC.len();
        }
        let n = r.u32()?;
        let mut lines = Vec::with_capacity(n.min(b.len() / 4));
        for _ in 0..n {
            let m = r.u32()?;
            let pts = r.take(m.checked_mul(16)?)?;
            lines.push(
                pts.as_chunks::<16>()
                    .0
                    .iter()
                    .map(|c| [f64_at(&c[..8]), f64_at(&c[8..])])
                    .collect(),
            );
        }
        let mut entities = Vec::new();
        if with_entities {
            entities.reserve(n);
            for _ in 0..n {
                let kind = Kind::from_u8(r.take(1)?[0]);
                let color = i16::from_le_bytes(r.take(2)?.try_into().ok()?);
                let elevation = f64_at(r.take(8)?);
                let thickness = f64_at(r.take(8)?);
                let handle = r.text()?;
                let linetype = r.text()?;
                entities.push(Entity {
                    kind,
                    handle,
                    color: (color != i16::MIN).then_some(color),
                    linetype,
                    elevation: (!elevation.is_nan()).then_some(elevation),
                    thickness: (!thickness.is_nan()).then_some(thickness),
                });
            }
        }
        Some(LineSet::new(lines, entities))
    }
}

fn f64_at(s: &[u8]) -> f64 {
    f64::from_le_bytes(s.try_into().unwrap_or([0; 8]))
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<usize> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
    }
    fn text(&mut self) -> Option<String> {
        let n = self.u32()?;
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

/// Whether a line is closed: a ring.
fn is_closed(l: &[[f64; 2]]) -> bool {
    l.len() >= 4 && l[0] == l[l.len() - 1]
}

fn dist_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// Whether the segment `a`-`b` passes through the box `bx` (Liang-Barsky).
fn crosses_box(a: [f64; 2], b: [f64; 2], bx: [f64; 4]) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a[0] - bx[0]),
        (dx, bx[2] - a[0]),
        (-dy, a[1] - bx[1]),
        (dy, bx[3] - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t0 <= t1
}

/// Whether the ring `l` holds `p` (even-odd rule).
fn contains(l: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for w in l.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        {
            inside = !inside;
        }
    }
    inside
}

/// A layer's lines, shared by its copies (undo of map layers, the saved
/// state): two are equal when they are the same lines.
#[derive(Clone, Default)]
pub struct Lines(pub Arc<LineSet>);

impl PartialEq for Lines {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.0, &o.0) || (self.0.lines.is_empty() && o.0.lines.is_empty())
    }
}

impl std::fmt::Debug for Lines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lines({})", self.0.lines.len())
    }
}

// ----- drawing --------------------------------------------------------------

/// How long the view must stay still before the layers are drawn again (s).
const SETTLE: f64 = 0.25;
/// The picture reaches this far past the map on each side (of its size), so
/// that a small pan shows no empty edge.
const MARGIN: f32 = 0.15;
/// The largest picture, on either side (pixels).
const MAX_SIDE: f32 = 4096.0;

/// A picture of a run of DXF layers (consecutive in the Map Layers panel),
/// drawn once for a view: while the map moves the picture moves with it, and
/// once the view settles the layers are drawn again. This keeps the map
/// quick whatever the number of lines (a frame draws one picture).
#[derive(Default)]
pub struct Picture {
    tex: Option<egui::TextureHandle>,
    /// What it shows: the layers and how they are drawn.
    content: u64,
    /// The view it was drawn for: center, scale, size in pixels.
    view: Option<ViewKey>,
    /// The part of the map it covers: x min, y min, x max, y max.
    world: [f64; 4],
    /// The view waited for, and since when.
    wanted: Option<(ViewKey, f64)>,
}

type ViewKey = ([f64; 2], f64, [u32; 2]);

/// What the layers' picture depends on, besides the view.
fn content_key(layers: &[&MapLayer]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for l in layers {
        (Arc::as_ptr(&l.lines.0) as usize).hash(&mut h);
        l.color.hash(&mut h);
        l.opacity.to_bits().hash(&mut h);
        l.fill.hash(&mut h);
    }
    h.finish()
}

/// Draws the run of visible DXF layers `layers` (bottom first) with picture
/// `pic`, drawing it again once the view has settled.
#[allow(clippy::too_many_arguments)]
pub fn draw_run(
    pic: &mut Picture,
    layers: &[&MapLayer],
    view: &crate::map::MapView,
    painter: &egui::Painter,
    rect: Rect,
    now: f64,
    ctx: &egui::Context,
) {
    let ppp = ctx.pixels_per_point();
    let px = |v: f32| ((v * (1.0 + 2.0 * MARGIN) * ppp).round()).clamp(1.0, MAX_SIDE) as u32;
    let key: ViewKey = (
        view.center,
        view.scale,
        [px(rect.width()), px(rect.height())],
    );
    let content = content_key(layers);
    let changed = pic.tex.is_none() || pic.content != content;
    if changed || pic.view != Some(key) {
        // new content is drawn at once; a new view once it has settled
        let settled = match pic.wanted {
            Some((k, since)) if k == key => now - since >= SETTLE,
            _ => {
                pic.wanted = Some((key, now));
                false
            }
        };
        if changed || settled {
            let (w, h) = (key.2[0], key.2[1]);
            // the map box the picture covers, centered on the view
            let (hw, hh) = (
                w as f64 / ppp as f64 / view.scale / 2.0,
                h as f64 / ppp as f64 / view.scale / 2.0,
            );
            let c = view.to_world(rect, rect.center());
            let world = [c[0] - hw, c[1] - hh, c[0] + hw, c[1] + hh];
            let img = rasterize(layers, world, w as usize, h as usize, ppp);
            match &mut pic.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => {
                    pic.tex =
                        Some(ctx.load_texture("dxf layers", img, egui::TextureOptions::LINEAR))
                }
            }
            pic.content = content;
            pic.view = Some(key);
            pic.world = world;
            pic.wanted = None;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE));
        }
    }
    if let Some(t) = &pic.tex {
        let w = pic.world;
        let r = Rect::from_min_max(
            view.to_screen(rect, [w[0], w[3]]),
            view.to_screen(rect, [w[2], w[1]]),
        );
        painter.image(
            t.id(),
            r,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

/// A `w` x `h` picture of the map box `world` with the layers drawn in it,
/// bottom first: their polygons filled (when they are) and their lines.
pub fn rasterize(
    layers: &[&MapLayer],
    world: [f64; 4],
    w: usize,
    h: usize,
    ppp: f32,
) -> egui::ColorImage {
    let mut c = Canvas {
        w,
        h,
        px: vec![[0.0; 4]; w * h],
        brush: if ppp >= 1.5 { 2 } else { 1 },
    };
    let kx = w as f64 / (world[2] - world[0]);
    let ky = h as f64 / (world[3] - world[1]);
    let at = |[x, y]: [f64; 2]| [((x - world[0]) * kx) as f32, ((world[3] - y) * ky) as f32];
    let off =
        |b: &[f64; 4]| b[2] < world[0] || b[0] > world[2] || b[3] < world[1] || b[1] > world[3];
    let mut pts: Vec<[f32; 2]> = Vec::new();
    for layer in layers {
        let set = &layer.lines.0;
        if set.bounds.is_none_or(|b| off(&b)) {
            continue;
        }
        let [r, g, b] = layer.color.map(|v| v as f32 / 255.0);
        let a = layer.opacity.clamp(0.0, 1.0);
        let stroke = [r, g, b, a];
        let fill = [r, g, b, a * 0.3];
        if layer.fill && set.has_polygons {
            for ((l, bx), closed) in set.lines.iter().zip(&set.boxes).zip(&set.closed) {
                // a polygon of a pixel or two is its outline
                let small = (bx[2] - bx[0]) * kx < 2.0 || (bx[3] - bx[1]) * ky < 2.0;
                if *closed && !small && !off(bx) {
                    pts.clear();
                    pts.extend(l.iter().map(|&p| at(p)));
                    c.fill(&pts, fill);
                }
            }
        }
        for (l, bx) in set.lines.iter().zip(&set.boxes) {
            if off(bx) {
                continue;
            }
            // points in the pixel of the last one drawn are passed over
            let cell = |p: [f32; 2]| (p[0].floor() as i64, p[1].floor() as i64);
            let mut prev = at(l[0]);
            let mut prev_cell = cell(prev);
            for &p in &l[1..] {
                let q = at(p);
                let q_cell = cell(q);
                if q_cell != prev_cell {
                    c.segment(prev, q, stroke);
                    prev = q;
                    prev_cell = q_cell;
                }
            }
            // the last pixel (segments leave out their ends)
            c.plot(prev_cell.0, prev_cell.1, stroke);
        }
    }
    let rgba: Vec<u8> =
        c.px.iter()
            .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect();
    egui::ColorImage::from_rgba_premultiplied([w, h], &rgba)
}

/// Pixels in premultiplied colors, drawn over.
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
    /// The width of a line, in pixels.
    brush: i64,
}

impl Canvas {
    fn blend(&mut self, i: usize, [r, g, b, a]: [f32; 4]) {
        let d = &mut self.px[i];
        let k = 1.0 - a;
        *d = [
            r * a + d[0] * k,
            g * a + d[1] * k,
            b * a + d[2] * k,
            a + d[3] * k,
        ];
    }

    fn plot(&mut self, x: i64, y: i64, color: [f32; 4]) {
        for dy in 0..self.brush {
            for dx in 0..self.brush {
                let (x, y) = (x + dx, y + dy);
                if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
                    self.blend(y as usize * self.w + x as usize, color);
                }
            }
        }
    }

    /// A line from `a` to `b`, the part inside the picture.
    fn segment(&mut self, a: [f32; 2], b: [f32; 2], color: [f32; 4]) {
        let Some((a, b)) = clip(a, b, self.w as f32, self.h as f32) else {
            return;
        };
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let n = dx.abs().max(dy.abs()).ceil().max(1.0) as i64;
        let (sx, sy) = (dx / n as f32, dy / n as f32);
        // not the last pixel: it is the next segment's first
        for i in 0..n {
            let x = a[0] + sx * i as f32;
            let y = a[1] + sy * i as f32;
            self.plot(x.floor() as i64, y.floor() as i64, color);
        }
    }

    /// Fills the ring `pts` (even-odd), scanline by scanline.
    fn fill(&mut self, pts: &[[f32; 2]], color: [f32; 4]) {
        // the edges by their top row
        let mut edges: Vec<(f32, f32, f32, f32)> = pts
            .windows(2)
            .filter(|e| e[0][1] != e[1][1])
            .map(|e| {
                let (p, q) = if e[0][1] < e[1][1] {
                    (e[0], e[1])
                } else {
                    (e[1], e[0])
                };
                (p[1], q[1], p[0], (q[0] - p[0]) / (q[1] - p[1]))
            })
            .collect();
        if edges.is_empty() {
            return;
        }
        edges.sort_by(|a, b| a.0.total_cmp(&b.0));
        let top = edges[0].0.max(0.0).floor() as usize;
        let bottom = edges
            .iter()
            .map(|e| e.1)
            .fold(f32::MIN, f32::max)
            .min(self.h as f32)
            .ceil()
            .max(0.0) as usize;
        let mut next = 0;
        let mut active: Vec<usize> = Vec::new();
        let mut xs: Vec<f32> = Vec::new();
        for row in top..bottom {
            let y = row as f32 + 0.5;
            while next < edges.len() && edges[next].0 <= y {
                active.push(next);
                next += 1;
            }
            active.retain(|&k| edges[k].1 > y);
            xs.clear();
            xs.extend(active.iter().filter(|&&k| edges[k].0 <= y).map(|&k| {
                let e = edges[k];
                e.2 + (y - e.0) * e.3
            }));
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.as_chunks::<2>().0 {
                let x0 = (pair[0] + 0.5).floor().max(0.0) as usize;
                let x1 = ((pair[1] + 0.5).floor().max(0.0) as usize).min(self.w);
                for x in x0..x1 {
                    self.blend(row * self.w + x, color);
                }
            }
        }
    }
}

/// The part of the segment `a`-`b` inside 0..w x 0..h (Liang-Barsky).
fn clip(a: [f32; 2], b: [f32; 2], w: f32, h: f32) -> Option<([f32; 2], [f32; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, a[0] + 1.0),
        (dx, w - a[0]),
        (-dy, a[1] + 1.0),
        (dy, h - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then(|| {
        (
            [a[0] + t0 * dx, a[1] + t0 * dy],
            [a[0] + t1 * dx, a[1] + t1 * dy],
        )
    })
}

/// A line on screen, without points closer than a pixel to the previous one.
fn screen_line(view: &crate::map::MapView, rect: Rect, l: &[[f64; 2]]) -> Option<Vec<Pos2>> {
    let mut pts: Vec<Pos2> = Vec::with_capacity(l.len());
    for &p in l {
        let s = view.to_screen(rect, p);
        if pts.last().is_none_or(|q| q.distance_sq(s) >= 1.0) {
            pts.push(s);
        }
    }
    let end = view.to_screen(rect, *l.last()?);
    if pts.last() != Some(&end) {
        pts.push(end);
    }
    (pts.len() >= 2).then_some(pts)
}

/// Outlines a line found (by Identify or in the attribute table).
pub fn draw_outline(
    view: &crate::map::MapView,
    painter: &egui::Painter,
    rect: Rect,
    l: &[[f64; 2]],
    color: Color32,
) {
    let stroke = egui::Stroke::new(2.5, color);
    if let Some(pts) = screen_line(view, rect, l) {
        painter.add(Shape::line(pts, stroke));
    } else if let Some(&p) = l.first() {
        painter.circle_stroke(view.to_screen(rect, p), 6.0, stroke);
    }
}

/// Shows box `b` (map coordinates) on the map, with a margin.
pub fn zoom_to(app: &mut EpanetApp, b: [f64; 4]) {
    let Some(rect) = app.export.map_rect else {
        return;
    };
    // a point or a straight line still gets an area around it
    let pad = ((b[2] - b[0]).max(b[3] - b[1]) * 0.05).max(5.0);
    app.view
        .fit(rect, Some([b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad]));
    let (center, scale) = (app.view.center, app.view.scale);
    app.view.place(center, scale);
}

// ----- the attribute table --------------------------------------------------

/// An open attribute table: a DXF layer's lines, one row each.
pub struct Table {
    pub set: Lines,
    pub name: String,
    filter: String,
    /// The lines the filter lets through, and the filter they are for.
    rows: Vec<u32>,
    rows_for: Option<String>,
    /// The line chosen, outlined on the map.
    pub selected: Option<usize>,
}

impl Table {
    pub fn new(layer: &MapLayer) -> Self {
        Table {
            set: layer.lines.clone(),
            name: layer.name.clone(),
            filter: String::new(),
            rows: Vec::new(),
            rows_for: None,
            selected: None,
        }
    }

    fn refresh_rows(&mut self) {
        if self.rows_for.as_deref() == Some(self.filter.as_str()) {
            return;
        }
        let set = &self.set.0;
        let f = self.filter.trim().to_lowercase();
        self.rows = (0..set.len() as u32)
            .filter(|&k| {
                f.is_empty()
                    || (0..COLUMNS.len())
                        .any(|c| set.cell(k as usize, c).to_lowercase().contains(&f))
            })
            .collect();
        self.rows_for = Some(self.filter.clone());
    }
}

const WIDTHS: [f32; 10] = [56.0, 96.0, 72.0, 68.0, 90.0, 76.0, 76.0, 56.0, 90.0, 100.0];
const ROW: f32 = 20.0;

/// Opens the attribute table of map layer `k` (a DXF layer).
pub fn open_table(app: &mut EpanetApp, k: usize) {
    if let Some(l) = app
        .prefs
        .map_layers
        .get(k)
        .filter(|l| l.service == Service::Dxf)
    {
        app.layers.table = Some(Table::new(l));
    }
}

/// The attribute table window, while one is open (and its layer is there).
pub fn table_window(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(mut t) = app.layers.table.take() else {
        return;
    };
    if !app.prefs.map_layers.iter().any(|l| l.lines == t.set) {
        return; // its layer was removed
    }
    t.refresh_rows();
    let mut open = true;
    let mut zoom = None;
    let title = format!("{} — {}", crate::i18n::tr("Attribute Table"), t.name);
    egui::Window::new(title)
        .id(egui::Id::new("dxf_attribute_table"))
        .open(&mut open)
        .default_size([760.0, 420.0])
        .resizable(true)
        .show(ctx, |ui| {
            let set = t.set.0.clone();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut t.filter)
                        .hint_text(crate::i18n::tr("Filter"))
                        .desired_width(220.0),
                );
                ui.label(crate::i18n::trf(
                    "{} of {} features",
                    &[&t.rows.len(), &set.len()],
                ));
                let chosen = t.selected.and_then(|k| set.bounds_of(k));
                if ui
                    .add_enabled(
                        chosen.is_some(),
                        egui::Button::new(crate::i18n::tr("Zoom to Selected")),
                    )
                    .clicked()
                {
                    zoom = chosen;
                }
            });
            ui.add_space(4.0);
            egui::ScrollArea::horizontal()
                .id_salt("dxf_table_h")
                .show(ui, |ui| {
                    let total: f32 = WIDTHS.iter().sum();
                    let header = std::iter::once("#").chain(COLUMNS.iter().copied());
                    let (rect, _) = ui.allocate_exact_size(vec2(total, ROW), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
                    paint_cells(
                        ui,
                        rect,
                        header.map(|h| crate::i18n::tr(h).to_string()),
                        true,
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("dxf_table_v")
                        .auto_shrink([false, false])
                        .show_rows(ui, ROW, t.rows.len(), |ui, range| {
                            for r in range {
                                let k = t.rows[r] as usize;
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(total, ROW), Sense::click());
                                let v = ui.visuals();
                                if t.selected == Some(k) {
                                    ui.painter().rect_filled(rect, 0.0, v.selection.bg_fill);
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        0.0,
                                        v.widgets.hovered.weak_bg_fill,
                                    );
                                } else if r % 2 == 1 {
                                    ui.painter().rect_filled(rect, 0.0, v.faint_bg_color);
                                }
                                let cells = std::iter::once((k + 1).to_string())
                                    .chain((0..COLUMNS.len()).map(|c| set.cell(k, c)));
                                paint_cells(ui, rect, cells, false);
                                if resp.clicked() {
                                    t.selected = Some(k);
                                }
                                if resp.double_clicked() {
                                    zoom = set.bounds_of(k);
                                }
                            }
                        });
                });
        });
    if let Some(b) = zoom {
        zoom_to(app, b);
    }
    if open {
        app.layers.table = Some(t);
    }
}

fn paint_cells(ui: &Ui, rect: Rect, cells: impl Iterator<Item = String>, strong: bool) {
    let font = egui::FontId::proportional(13.0);
    let color = if strong {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    let mut x = rect.left();
    for (w, text) in WIDTHS.iter().zip(cells) {
        let cell = Rect::from_min_max(pos2(x, rect.top()), pos2(x + w, rect.bottom()));
        ui.painter()
            .with_clip_rect(cell.shrink2(vec2(4.0, 0.0)))
            .text(
                pos2(cell.left() + 4.0, cell.center().y),
                egui::Align2::LEFT_CENTER,
                text,
                font.clone(),
                color,
            );
        x += w;
    }
}

/// Outlines the line chosen in the attribute table.
pub fn draw_table_selection(app: &EpanetApp, painter: &egui::Painter, rect: Rect) {
    let Some(t) = &app.layers.table else {
        return;
    };
    if let Some(l) = t.selected.and_then(|k| t.set.0.lines.get(k)) {
        draw_outline(
            &app.view,
            painter,
            rect,
            l,
            Color32::from_rgb(0x00, 0xb4, 0xd8),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<[f64; 2]> {
        vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ]
    }

    #[test]
    fn finds_lines_near_and_polygons_around_a_point() {
        let set = LineSet::new(
            vec![square(), vec![[20.0, 0.0], [20.0, 10.0]]],
            vec![
                Entity {
                    kind: Kind::LwPolyline,
                    handle: "1A".into(),
                    ..Default::default()
                },
                Entity::default(),
            ],
        );
        assert!(set.has_polygons);
        assert_eq!(set.hits([5.0, 5.0], 0.5, 10), [0]);
        assert_eq!(set.hits([20.2, 5.0], 0.5, 10), [1]);
        assert!(set.hits([15.0, 5.0], 0.5, 10).is_empty());
        assert_eq!(set.cell(0, 0), "LWPOLYLINE");
        assert_eq!(set.cell(0, 8), "100.00");
        assert_eq!(set.cell(1, 7), "10.00");
        assert_eq!(set.cell(1, 8), "");
    }

    #[test]
    fn lines_and_entities_go_through_bytes() {
        let set = LineSet::new(
            vec![square()],
            vec![Entity {
                kind: Kind::Polyline,
                handle: "2F".into(),
                color: Some(3),
                linetype: "DASHED".into(),
                elevation: Some(12.5),
                thickness: None,
            }],
        );
        let back = LineSet::from_bytes(&set.to_bytes()).unwrap();
        assert_eq!(back.lines, set.lines);
        assert_eq!(back.entities, set.entities);
        assert!(LineSet::from_bytes(b"DXL2\x05\0\0\0").is_none());
    }

    /// A filled square: its outline in the layer's color, inside a light
    /// shade of it, outside nothing; without the fill, only the outline.
    #[test]
    fn rasterizes_lines_and_fills() {
        let mut layer = MapLayer {
            service: Service::Dxf,
            color: [255, 0, 0],
            lines: Lines(Arc::new(LineSet::new(vec![square()], Vec::new()))),
            ..Default::default()
        };
        assert!(layer.lines.0.fill_by_default());
        // 20 x 20 pixels over 0..20: the square spans pixels 0..10
        let px = |img: &egui::ColorImage, x: usize, y: usize| img.pixels[y * 20 + x];
        let img = rasterize(&[&layer], [0.0, 0.0, 20.0, 20.0], 20, 20, 1.0);
        assert_eq!(px(&img, 0, 15), Color32::from_rgb(255, 0, 0));
        let inside = px(&img, 5, 15);
        assert!(inside.a() > 50 && inside.a() < 120, "{inside:?}");
        assert_eq!(px(&img, 15, 5), Color32::TRANSPARENT);
        layer.fill = false;
        let img = rasterize(&[&layer], [0.0, 0.0, 20.0, 20.0], 20, 20, 1.0);
        assert_eq!(px(&img, 5, 15), Color32::TRANSPARENT);
        // a segment far outside the picture is clipped, not walked
        assert!(clip([-1e9, 5.0], [1e9, 5.0], 20.0, 20.0).is_some());
        assert!(clip([-1e9, -5.0], [1e9, -5.0], 20.0, 20.0).is_none());
    }
}
