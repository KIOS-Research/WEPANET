//! Reader for the lines of an ASCII DXF drawing (EPANET-UI's `dxfloader`):
//! its layers and the LINE, LWPOLYLINE, POLYLINE, ARC, CIRCLE and SPLINE
//! entities on them, also those inside blocks (INSERT), which go on the
//! layer of the insert.

use crate::import::MapUnits;

/// A drawing's layers and lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// Layer names, in the order of the file's layer table, then of first use.
    pub layers: Vec<String>,
    /// Lines: the index of their layer and their points.
    pub lines: Vec<(usize, Vec<[f64; 2]>)>,
    /// What each line's entity says of itself, in the order of `lines`.
    pub entities: Vec<Entity>,
    /// The drawing units of the header's $INSUNITS, when feet or meters.
    pub units: Option<MapUnits>,
}

/// The kind of entity a line was drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Line,
    LwPolyline,
    Polyline,
    Arc,
    Circle,
    Spline,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Line => "LINE",
            Kind::LwPolyline => "LWPOLYLINE",
            Kind::Polyline => "POLYLINE",
            Kind::Arc => "ARC",
            Kind::Circle => "CIRCLE",
            Kind::Spline => "SPLINE",
        }
    }

    pub fn from_u8(k: u8) -> Self {
        match k {
            1 => Kind::LwPolyline,
            2 => Kind::Polyline,
            3 => Kind::Arc,
            4 => Kind::Circle,
            5 => Kind::Spline,
            _ => Kind::Line,
        }
    }
}

/// An entity's own properties (its attributes in the drawing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entity {
    pub kind: Kind,
    /// Its handle (group code 5), the drawing's ID for it.
    pub handle: String,
    /// Its AutoCAD color index (62): 0 by block, 256 by layer.
    pub color: Option<i16>,
    /// Its linetype (6), when it has its own.
    pub linetype: String,
    /// Its elevation (38, or the z of a line's start).
    pub elevation: Option<f64>,
    /// Its thickness (39).
    pub thickness: Option<f64>,
}

impl Drawing {
    /// Number of lines on each layer.
    pub fn counts(&self) -> Vec<usize> {
        let mut n = vec![0; self.layers.len()];
        for (l, _) in &self.lines {
            n[*l] += 1;
        }
        n
    }
}

/// The (group code, value) pairs of the file, read one at a time (a large
/// drawing has tens of millions of them), with one pair of look-ahead.
struct Pairs<'a> {
    lines: std::str::Lines<'a>,
    next: Option<(i32, &'a str)>,
}

impl<'a> Pairs<'a> {
    fn new(text: &'a str) -> Result<Self, String> {
        let mut p = Pairs {
            lines: text.lines(),
            next: None,
        };
        p.next = p.read()?;
        Ok(p)
    }

    fn read(&mut self) -> Result<Option<(i32, &'a str)>, String> {
        while let Some(code) = self.lines.next() {
            let code = code.trim();
            if code.is_empty() {
                continue;
            }
            let code: i32 = code
                .parse()
                .map_err(|_| format!("\"{code}\" is not a DXF group code"))?;
            let value = self.lines.next().unwrap_or("").trim();
            return Ok(Some((code, value)));
        }
        Ok(None)
    }

    fn peek(&self) -> Option<(i32, &'a str)> {
        self.next
    }

    fn take(&mut self) -> Result<Option<(i32, &'a str)>, String> {
        let p = self.next;
        if p.is_some() {
            self.next = self.read()?;
        }
        Ok(p)
    }

    /// The pairs up to the next code 0, into `out`.
    fn body(&mut self, out: &mut Vec<(i32, &'a str)>) -> Result<(), String> {
        out.clear();
        while let Some(p) = self.peek().filter(|p| p.0 != 0) {
            out.push(p);
            self.take()?;
        }
        Ok(())
    }
}

/// The text of the file, borrowed when it is UTF-8 (no copy of a large file).
fn text(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(s) => std::borrow::Cow::Borrowed(s.trim_start_matches('\u{feff}')),
        Err(_) => std::borrow::Cow::Owned(crate::io::decode_text(bytes)),
    }
}

/// What an entity is on the map: a line (with its layer and properties), or
/// a block inserted.
enum Item<'a> {
    Line(&'a str, Vec<[f64; 2]>, Entity),
    Insert(Insert<'a>),
}

/// A block reference (INSERT): the block's lines moved, scaled and turned.
struct Insert<'a> {
    layer: &'a str,
    block: &'a str,
    at: [f64; 2],
    scale: [f64; 2],
    /// Degrees, counter-clockwise.
    rotation: f64,
}

/// A block's base point and what is drawn in it.
#[derive(Default)]
struct Block<'a> {
    base: [f64; 2],
    items: Vec<Item<'a>>,
}

/// An affine map: x' = a x + b y + c, y' = d x + e y + f.
#[derive(Clone, Copy)]
struct Affine([f64; 6]);

impl Affine {
    fn apply(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let m = &self.0;
        [m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]]
    }

    /// `self` after `o` (o first).
    fn then_after(&self, o: &Affine) -> Affine {
        let (a, b) = (&self.0, &o.0);
        Affine([
            a[0] * b[0] + a[1] * b[3],
            a[0] * b[1] + a[1] * b[4],
            a[0] * b[2] + a[1] * b[5] + a[2],
            a[3] * b[0] + a[4] * b[3],
            a[3] * b[1] + a[4] * b[4],
            a[3] * b[2] + a[4] * b[5] + a[5],
        ])
    }

    /// Where an insert puts its block's points (base point at `at`).
    fn of_insert(ins: &Insert, base: [f64; 2]) -> Affine {
        let (s, c) = ins.rotation.to_radians().sin_cos();
        let [sx, sy] = ins.scale;
        let rs = Affine([c * sx, -s * sy, 0.0, s * sx, c * sy, 0.0]);
        let shifted = Affine([1.0, 0.0, -base[0], 0.0, 1.0, -base[1]]);
        let moved = Affine([1.0, 0.0, ins.at[0], 0.0, 1.0, ins.at[1]]);
        moved.then_after(&rs.then_after(&shifted))
    }
}

/// Blocks inside blocks are followed this deep (a block cannot hold itself).
const MAX_NESTING: usize = 8;

/// Points around an arc of `radius` about `c`, from `a0` to `a1` degrees
/// counter-clockwise, about every 5 degrees.
fn arc_points(c: [f64; 2], radius: f64, a0: f64, a1: f64) -> Vec<[f64; 2]> {
    let mut sweep = (a1 - a0).rem_euclid(360.0);
    if sweep == 0.0 {
        sweep = 360.0;
    }
    let n = ((sweep / 5.0).ceil() as usize).max(4);
    (0..=n)
        .map(|i| {
            let a = (a0 + sweep * i as f64 / n as f64).to_radians();
            [c[0] + radius * a.cos(), c[1] + radius * a.sin()]
        })
        .collect()
}

/// Reads one entity of kind `value` from its group codes `body` (a
/// POLYLINE's VERTEX entities follow it in `pairs`).
fn read_entity<'a>(
    value: &str,
    body: &[(i32, &'a str)],
    pairs: &mut Pairs<'a>,
) -> Result<Option<Item<'a>>, String> {
    let num = |s: &str| s.trim().parse::<f64>().ok();
    let code = |c: i32| body.iter().find(|p| p.0 == c).map(|p| p.1);
    let codef = |c: i32| code(c).and_then(num);
    let on = code(8).unwrap_or("0");
    let points = |xc: i32, yc: i32| -> Vec<[f64; 2]> {
        let mut pts = Vec::new();
        let mut x = None;
        for &(c, v) in body {
            if c == xc {
                x = num(v);
            } else if c == yc
                && let (Some(px), Some(py)) = (x.take(), num(v))
            {
                pts.push([px, py]);
            }
        }
        pts
    };
    let flags = code(70)
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(0);
    let closed = flags & 1 == 1;
    // drawn upside down (extrusion -z): x is mirrored
    let mirrored = codef(230).is_some_and(|z| z < 0.0);
    let center = || Some([codef(10)?, codef(20)?]);
    let (kind, mut pts) = match value {
        "INSERT" => {
            return Ok(code(2).map(|block| {
                Item::Insert(Insert {
                    layer: on,
                    block,
                    at: [codef(10).unwrap_or(0.0), codef(20).unwrap_or(0.0)],
                    scale: [codef(41).unwrap_or(1.0), codef(42).unwrap_or(1.0)],
                    rotation: codef(50).unwrap_or(0.0),
                })
            }));
        }
        "LINE" => {
            let mut p = points(10, 20);
            p.extend(points(11, 21));
            (Kind::Line, p)
        }
        "LWPOLYLINE" => {
            let mut p = points(10, 20);
            if closed && p.len() > 2 {
                p.push(p[0]);
            }
            (Kind::LwPolyline, p)
        }
        "POLYLINE" => {
            // its VERTEX entities, up to SEQEND
            let mut p = Vec::new();
            while pairs.peek() == Some((0, "VERTEX")) {
                pairs.take()?;
                let (mut x, mut y) = (None, None);
                while let Some((c, v)) = pairs.peek().filter(|p| p.0 != 0) {
                    match c {
                        10 => x = num(v),
                        20 => y = num(v),
                        _ => {}
                    }
                    pairs.take()?;
                }
                if let (Some(x), Some(y)) = (x, y) {
                    p.push([x, y]);
                }
            }
            if pairs.peek() == Some((0, "SEQEND")) {
                pairs.take()?;
                while pairs.peek().is_some_and(|p| p.0 != 0) {
                    pairs.take()?;
                }
            }
            if closed && p.len() > 2 {
                p.push(p[0]);
            }
            (Kind::Polyline, p)
        }
        "ARC" => {
            let (Some(c), Some(r)) = (center(), codef(40)) else {
                return Ok(None);
            };
            let a0 = codef(50).unwrap_or(0.0);
            let a1 = codef(51).unwrap_or(360.0);
            (Kind::Arc, arc_points(c, r, a0, a1))
        }
        "CIRCLE" => {
            let (Some(c), Some(r)) = (center(), codef(40)) else {
                return Ok(None);
            };
            let mut p = arc_points(c, r, 0.0, 360.0);
            // exactly closed, a ring
            let first = p[0];
            *p.last_mut().unwrap() = first;
            (Kind::Circle, p)
        }
        "SPLINE" => {
            // through its fit points, else along its control points
            let fit = points(11, 21);
            let mut p = if fit.len() >= 2 { fit } else { points(10, 20) };
            if closed && p.len() > 2 {
                p.push(p[0]);
            }
            (Kind::Spline, p)
        }
        _ => return Ok(None),
    };
    if mirrored && matches!(kind, Kind::Arc | Kind::Circle | Kind::LwPolyline) {
        for p in &mut pts {
            p[0] = -p[0];
        }
    }
    pts.dedup();
    if pts.len() < 2 {
        return Ok(None);
    }
    let entity = Entity {
        kind,
        handle: code(5).unwrap_or_default().to_string(),
        color: code(62).and_then(|v| v.trim().parse().ok()),
        linetype: code(6).unwrap_or_default().to_string(),
        elevation: codef(38).or_else(|| codef(30)),
        thickness: codef(39),
    };
    Ok(Some(Item::Line(on, pts, entity)))
}

/// Adds the lines of block `name`, mapped by `m`, on layer `on` (the layer
/// of the outermost insert: a block's drawing belongs to where it is put).
fn expand(
    blocks: &std::collections::HashMap<&str, Block>,
    name: &str,
    m: &Affine,
    depth: usize,
    out: &mut dyn FnMut(Vec<[f64; 2]>, Entity),
) {
    let Some(b) = blocks.get(name) else {
        return;
    };
    if depth > MAX_NESTING {
        return;
    }
    for item in &b.items {
        match item {
            Item::Line(_, pts, e) => {
                let mut p: Vec<[f64; 2]> = pts.iter().map(|&q| m.apply(q)).collect();
                p.dedup();
                if p.len() >= 2 {
                    out(p, e.clone());
                }
            }
            Item::Insert(ins) => {
                let Some(inner) = blocks.get(ins.block) else {
                    continue;
                };
                let mm = m.then_after(&Affine::of_insert(ins, inner.base));
                expand(blocks, ins.block, &mm, depth + 1, out);
            }
        }
    }
}

pub fn read(bytes: &[u8]) -> Result<Drawing, String> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(
            "Binary DXF files cannot be read: save the drawing as an ASCII DXF file".into(),
        );
    }
    let text = text(bytes);
    let mut pairs = Pairs::new(&text)?;
    let mut d = Drawing::default();
    let mut index: std::collections::HashMap<String, usize> = Default::default();
    let mut layer = |d: &mut Drawing, name: &str| -> usize {
        let name = if name.is_empty() { "0" } else { name };
        if let Some(&i) = index.get(name) {
            return i;
        }
        d.layers.push(name.to_string());
        index.insert(name.to_string(), d.layers.len() - 1);
        d.layers.len() - 1
    };
    let mut blocks: std::collections::HashMap<&str, Block> = Default::default();
    // the block being read in the BLOCKS section
    let mut current: Option<(&str, Block)> = None;
    let mut section = "";
    let mut body = Vec::new();
    while let Some((code, value)) = pairs.take()? {
        if code == 0 && value == "SECTION" {
            if let Some((2, name)) = pairs.peek() {
                section = name;
                pairs.take()?;
            }
            continue;
        }
        if code == 0 && value == "ENDSEC" {
            section = "";
            continue;
        }
        match section {
            "HEADER" if code == 9 && value == "$INSUNITS" => {
                if let Some((70, u)) = pairs.peek() {
                    d.units = match u.trim() {
                        "2" => Some(MapUnits::Feet),
                        "6" => Some(MapUnits::Meters),
                        _ => None,
                    };
                }
            }
            "TABLES" if code == 0 && value == "LAYER" => {
                // a layer table entry: its name follows as code 2
                pairs.body(&mut body)?;
                for &(c, v) in &body {
                    if c == 2 {
                        layer(&mut d, v);
                    }
                }
            }
            "BLOCKS" if code == 0 => {
                pairs.body(&mut body)?;
                match value {
                    "BLOCK" => {
                        let get = |c: i32| body.iter().find(|p| p.0 == c).map(|p| p.1);
                        let f = |c: i32| get(c).and_then(|v| v.trim().parse::<f64>().ok());
                        let base = [f(10).unwrap_or(0.0), f(20).unwrap_or(0.0)];
                        current = get(2).map(|name| {
                            (
                                name,
                                Block {
                                    base,
                                    items: Vec::new(),
                                },
                            )
                        });
                    }
                    "ENDBLK" => {
                        if let Some((name, b)) = current.take() {
                            blocks.insert(name, b);
                        }
                    }
                    _ => {
                        if let Some(item) = read_entity(value, &body, &mut pairs)?
                            && let Some((_, b)) = current.as_mut()
                        {
                            b.items.push(item);
                        }
                    }
                }
            }
            "ENTITIES" if code == 0 => {
                pairs.body(&mut body)?;
                match read_entity(value, &body, &mut pairs)? {
                    Some(Item::Line(on, pts, e)) => {
                        let l = layer(&mut d, on);
                        d.lines.push((l, pts));
                        d.entities.push(e);
                    }
                    Some(Item::Insert(ins)) => {
                        let l = layer(&mut d, ins.layer);
                        let Some(b) = blocks.get(ins.block) else {
                            continue;
                        };
                        let m = Affine::of_insert(&ins, b.base);
                        let (lines, entities) = (&mut d.lines, &mut d.entities);
                        expand(&blocks, ins.block, &m, 0, &mut |pts, e| {
                            lines.push((l, pts));
                            entities.push(e);
                        });
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }
    if d.lines.is_empty() && !text.contains("ENTITIES") {
        return Err("The file is not a DXF drawing".into());
    }
    Ok(d)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A block with a line and a circle, inserted twice on a layer of its
    /// own: moved, scaled and turned 90 degrees.
    #[test]
    fn expands_blocks_on_the_layer_of_the_insert() {
        let dxf = "  0\nSECTION\n  2\nBLOCKS
  0\nBLOCK\n  8\n0\n  2\nNET\n 10\n1.0\n 20\n0.0
  0\nLINE\n  8\nPIPES\n  5\nA1\n 10\n1.0\n 20\n0.0\n 11\n3.0\n 21\n0.0
  0\nCIRCLE\n  8\n0\n 10\n1.0\n 20\n0.0\n 40\n0.5
  0\nENDBLK\n  0\nENDSEC
  0\nSECTION\n  2\nENTITIES
  0\nINSERT\n  8\nWDD-NAPA\n  2\nNET\n 10\n100.0\n 20\n200.0
  0\nINSERT\n  8\nWDD-NAPA\n  2\nNET\n 10\n0.0\n 20\n0.0\n 41\n2.0\n 42\n2.0\n 50\n90.0
  0\nINSERT\n  8\nOTHER\n  2\nMISSING\n 10\n0.0\n 20\n0.0
  0\nENDSEC\n  0\nEOF\n";
        let d = read(dxf.as_bytes()).unwrap();
        assert_eq!(d.layers, ["WDD-NAPA", "OTHER"]);
        assert_eq!(d.counts(), [4, 0]);
        // the line, its start at the insertion point
        assert_eq!(d.lines[0].1, vec![[100.0, 200.0], [102.0, 200.0]]);
        assert_eq!(d.entities[0].handle, "A1");
        assert_eq!(d.entities[1].kind, Kind::Circle);
        // scaled 2 and turned: along y
        let p = &d.lines[2].1;
        assert!((p[1][0] - 0.0).abs() < 1e-9 && (p[1][1] - 4.0).abs() < 1e-9);
        // a circle is a ring
        let c = &d.lines[1].1;
        assert_eq!(c.first(), c.last());
    }

    /// A small ASCII DXF: a layer table, a LINE, an LWPOLYLINE and a
    /// POLYLINE with its VERTEX entities.
    pub(crate) const SAMPLE: &str = "  0\nSECTION\n  2\nHEADER\n  9\n$INSUNITS\n 70\n6\n  0\nENDSEC
  0\nSECTION\n  2\nTABLES\n  0\nTABLE\n  2\nLAYER\n  0\nLAYER\n  2\nPIPES\n 70\n0\n  0\nLAYER\n  2\nTEXT\n 70\n0
  0\nENDTAB\n  0\nENDSEC\n  0\nSECTION\n  2\nENTITIES
  0\nLINE\n  8\nPIPES\n 10\n0.0\n 20\n0.0\n 30\n0.0\n 11\n100.0\n 21\n0.0\n 31\n0.0
  0\nLWPOLYLINE\n  8\nPIPES\n 90\n3\n 70\n0\n 10\n100.2\n 20\n0.0\n 10\n100.0\n 20\n80.0\n 10\n200.0\n 20\n80.0
  0\nPOLYLINE\n  8\nMAINS\n 66\n1\n 70\n0\n  0\nVERTEX\n  8\nMAINS\n 10\n200.0\n 20\n80.0\n  0\nVERTEX\n  8\nMAINS\n 10\n200.0\n 20\n180.0\n  0\nSEQEND\n  8\nMAINS
  0\nTEXT\n  8\nTEXT\n 10\n5.0\n 20\n5.0\n  1\nPump station
  0\nENDSEC\n  0\nEOF\n";

    #[test]
    fn reads_layers_and_lines() {
        let d = read(SAMPLE.as_bytes()).unwrap();
        assert_eq!(d.layers, ["PIPES", "TEXT", "MAINS"]);
        assert_eq!(d.units, Some(MapUnits::Meters));
        assert_eq!(d.lines.len(), 3);
        assert_eq!(d.lines[0], (0, vec![[0.0, 0.0], [100.0, 0.0]]));
        assert_eq!(d.lines[1].1.len(), 3);
        assert_eq!(d.lines[2], (2, vec![[200.0, 80.0], [200.0, 180.0]]));
        assert_eq!(d.counts(), [2, 0, 1]);
    }

    #[test]
    fn rejects_other_files() {
        assert!(read(b"AutoCAD Binary DXF\r\n\x1a\0").is_err());
        assert!(read(b"hello\nworld\n").is_err());
    }
}
