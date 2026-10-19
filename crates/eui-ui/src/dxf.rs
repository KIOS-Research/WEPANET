//! Reader for the lines of an ASCII DXF drawing (EPANET-UI's `dxfloader`):
//! its layers and the LINE, LWPOLYLINE and POLYLINE entities on them.

use crate::import::MapUnits;

/// A drawing's layers and lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// Layer names, in the order of the file's layer table, then of first use.
    pub layers: Vec<String>,
    /// Lines: the index of their layer and their points.
    pub lines: Vec<(usize, Vec<[f64; 2]>)>,
    /// The drawing units of the header's $INSUNITS, when feet or meters.
    pub units: Option<MapUnits>,
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

/// The (group code, value) pairs of the file.
fn pairs(text: &str) -> Result<Vec<(i32, &str)>, String> {
    let mut lines = text.lines();
    let mut out = Vec::new();
    while let Some(code) = lines.next() {
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        let code: i32 = code
            .parse()
            .map_err(|_| format!("\"{code}\" is not a DXF group code"))?;
        let value = lines.next().unwrap_or("").trim();
        out.push((code, value));
    }
    Ok(out)
}

pub fn read(bytes: &[u8]) -> Result<Drawing, String> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(
            "Binary DXF files cannot be read: save the drawing as an ASCII DXF file".into(),
        );
    }
    let text = crate::io::decode_text(bytes);
    let pairs = pairs(&text)?;
    let mut d = Drawing::default();
    let layer = |d: &mut Drawing, name: &str| -> usize {
        let name = if name.is_empty() { "0" } else { name };
        match d.layers.iter().position(|l| l == name) {
            Some(i) => i,
            None => {
                d.layers.push(name.to_string());
                d.layers.len() - 1
            }
        }
    };
    let mut section = "";
    let mut k = 0;
    while k < pairs.len() {
        let (code, value) = pairs[k];
        k += 1;
        if code == 0 && value == "SECTION" {
            if let Some(&(2, name)) = pairs.get(k) {
                section = name;
                k += 1;
            }
            continue;
        }
        if code == 0 && value == "ENDSEC" {
            section = "";
            continue;
        }
        match section {
            "HEADER" if code == 9 && value == "$INSUNITS" => {
                if let Some(&(70, u)) = pairs.get(k) {
                    d.units = match u.trim() {
                        "2" => Some(MapUnits::Feet),
                        "6" => Some(MapUnits::Meters),
                        _ => None,
                    };
                }
            }
            "TABLES" if code == 0 && value == "LAYER" => {
                // a layer table entry: its name follows as code 2
                while k < pairs.len() && pairs[k].0 != 0 {
                    if pairs[k].0 == 2 {
                        layer(&mut d, pairs[k].1);
                    }
                    k += 1;
                }
            }
            "ENTITIES" if code == 0 => {
                // the entity's own group codes
                let start = k;
                while k < pairs.len() && pairs[k].0 != 0 {
                    k += 1;
                }
                let body = &pairs[start..k];
                let on = body
                    .iter()
                    .find(|p| p.0 == 8)
                    .map_or("0", |p| p.1)
                    .to_string();
                let num = |s: &str| s.trim().parse::<f64>().ok();
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
                let closed = body
                    .iter()
                    .find(|p| p.0 == 70)
                    .and_then(|p| p.1.trim().parse::<i32>().ok())
                    .is_some_and(|f| f & 1 == 1);
                let mut pts = match value {
                    "LINE" => {
                        let mut p = points(10, 20);
                        p.extend(points(11, 21));
                        p
                    }
                    "LWPOLYLINE" => points(10, 20),
                    "POLYLINE" => {
                        // its VERTEX entities, up to SEQEND
                        let mut p = Vec::new();
                        while k < pairs.len() && pairs[k] == (0, "VERTEX") {
                            k += 1;
                            let (mut x, mut y) = (None, None);
                            while k < pairs.len() && pairs[k].0 != 0 {
                                match pairs[k].0 {
                                    10 => x = num(pairs[k].1),
                                    20 => y = num(pairs[k].1),
                                    _ => {}
                                }
                                k += 1;
                            }
                            if let (Some(x), Some(y)) = (x, y) {
                                p.push([x, y]);
                            }
                        }
                        if k < pairs.len() && pairs[k] == (0, "SEQEND") {
                            k += 1;
                            while k < pairs.len() && pairs[k].0 != 0 {
                                k += 1;
                            }
                        }
                        p
                    }
                    _ => continue,
                };
                if closed && value != "LINE" && pts.len() > 2 {
                    pts.push(pts[0]);
                }
                pts.dedup();
                if pts.len() >= 2 {
                    let l = layer(&mut d, &on);
                    d.lines.push((l, pts));
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
