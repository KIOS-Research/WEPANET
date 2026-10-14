//! A small reader for ESRI shapefiles: the geometry (.shp), the attribute
//! table (.dbf, dBase III) and the coordinate system (.prj). Only what the
//! Shapefile Importer needs: points and polylines (with or without Z/M).

/// The geometry of one record.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Null,
    Point([f64; 2]),
    /// All the vertices of a polyline, its parts one after the other.
    Line(Vec<[f64; 2]>),
}

fn le_i32(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn be_i32(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn le_f64(b: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Reads the shapes of a .shp file.
pub fn read_shp(b: &[u8]) -> Result<Vec<Shape>, String> {
    if b.len() < 100 || be_i32(b, 0) != Some(9994) {
        return Err("not a shapefile (.shp)".into());
    }
    let mut shapes = Vec::new();
    let mut at = 100;
    while at + 8 <= b.len() {
        let len = be_i32(b, at + 4).ok_or("truncated record")? as usize * 2;
        let rec = b.get(at + 8..at + 8 + len).ok_or("truncated record")?;
        at += 8 + len;
        let kind = le_i32(rec, 0).unwrap_or(0);
        let shape = match kind {
            0 => Shape::Null,
            // Point, PointZ, PointM
            1 | 11 | 21 => Shape::Point([
                le_f64(rec, 4).ok_or("bad point")?,
                le_f64(rec, 12).ok_or("bad point")?,
            ]),
            // PolyLine, PolyLineZ, PolyLineM
            3 | 13 | 23 => {
                let parts = le_i32(rec, 36).ok_or("bad polyline")? as usize;
                let n = le_i32(rec, 40).ok_or("bad polyline")? as usize;
                let first = 44 + 4 * parts;
                let pts = (0..n)
                    .map(|k| {
                        Some([
                            le_f64(rec, first + 16 * k)?,
                            le_f64(rec, first + 16 * k + 8)?,
                        ])
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or("bad polyline")?;
                Shape::Line(pts)
            }
            // MultiPoint: its first point
            8 | 18 | 28 => match (le_f64(rec, 40), le_f64(rec, 48)) {
                (Some(x), Some(y)) => Shape::Point([x, y]),
                _ => Shape::Null,
            },
            k => {
                return Err(format!(
                    "shape type {k} is not supported (points and lines only)"
                ));
            }
        };
        shapes.push(shape);
    }
    Ok(shapes)
}

/// A dBase attribute table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dbf {
    pub fields: Vec<String>,
    /// Whether each field is numeric.
    pub numeric: Vec<bool>,
    pub records: Vec<Vec<String>>,
}

impl Dbf {
    #[cfg(test)]
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields
            .iter()
            .position(|f| f.eq_ignore_ascii_case(name))
    }

    /// A record's value (trimmed text), if the field and record exist.
    pub fn text(&self, record: usize, field: Option<usize>) -> Option<&str> {
        let v = self.records.get(record)?.get(field?)?.as_str();
        (!v.is_empty()).then_some(v)
    }

    pub fn number(&self, record: usize, field: Option<usize>) -> Option<f64> {
        self.text(record, field)?.parse().ok()
    }
}

fn decode(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

/// Reads a .dbf attribute table.
pub fn read_dbf(b: &[u8]) -> Result<Dbf, String> {
    if b.len() < 32 {
        return Err("not a dBase file (.dbf)".into());
    }
    let count = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
    let header = u16::from_le_bytes(b[8..10].try_into().unwrap()) as usize;
    let rec_len = u16::from_le_bytes(b[10..12].try_into().unwrap()) as usize;
    let mut fields = Vec::new();
    let mut at = 32;
    while at + 32 <= b.len() && b[at] != 0x0D && at < header {
        let d = &b[at..at + 32];
        let name = decode(&d[..11]).trim_end_matches('\0').trim().to_string();
        fields.push((name, d[11] as char, d[16] as usize));
        at += 32;
    }
    let mut records = Vec::with_capacity(count);
    for r in 0..count {
        let start = header + r * rec_len;
        let Some(rec) = b.get(start..start + rec_len) else {
            break;
        };
        if rec[0] == b'*' {
            continue; // deleted
        }
        let mut pos = 1;
        let mut row = Vec::with_capacity(fields.len());
        for (_, _, w) in &fields {
            let v = rec.get(pos..pos + w).map(decode).unwrap_or_default();
            row.push(v.trim().to_string());
            pos += w;
        }
        records.push(row);
    }
    Ok(Dbf {
        numeric: fields
            .iter()
            .map(|f| matches!(f.1, 'N' | 'F' | 'I' | 'B' | 'O'))
            .collect(),
        fields: fields.into_iter().map(|f| f.0).collect(),
        records,
    })
}

/// The EPSG code and map units a .prj file names, as far as can be told: its
/// AUTHORITY["EPSG", …] or a few well-known ESRI names.
pub fn read_prj(text: &str) -> (Option<u32>, Option<&'static str>) {
    let t = text.to_ascii_uppercase();
    let units = if t.contains("UNIT[\"DEGREE") && !t.starts_with("PROJCS") {
        Some("Degrees")
    } else if t.contains("FOOT") || t.contains("FEET") {
        Some("Feet")
    } else if t.contains("METER") || t.contains("METRE") {
        Some("Meters")
    } else if t.contains("DEGREE") {
        Some("Degrees")
    } else {
        None
    };
    // the last AUTHORITY is the whole system's
    let epsg = t
        .rfind("AUTHORITY[\"EPSG\",")
        .and_then(|k| {
            t[k + 18..]
                .trim_start_matches(['"', ' '])
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        })
        .or_else(|| {
            let name = t.split('"').nth(1).unwrap_or("");
            if name.contains("GREEK_GRID") || name.contains("GGRS_1987") {
                Some(2100)
            } else if name.contains("CYPRUS") && name.contains("TRANSVERSE") {
                Some(6312)
            } else if let Some(z) = name.strip_prefix("WGS_1984_UTM_ZONE_") {
                let n: u32 = z.trim_end_matches(['N', 'S']).parse().ok()?;
                Some(if z.ends_with('S') { 32700 } else { 32600 } + n)
            } else if name == "GCS_WGS_1984" || name == "WGS 84" {
                Some(4326)
            } else if name.contains("WEB_MERCATOR") || name.contains("PSEUDO-MERCATOR") {
                Some(3857)
            } else {
                None
            }
        });
    (epsg, units)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Writes a small shapefile (for tests): points or polylines.
    pub fn write_shp(shapes: &[Shape]) -> Vec<u8> {
        let mut body = Vec::new();
        for (k, s) in shapes.iter().enumerate() {
            let mut rec = Vec::new();
            match s {
                Shape::Null => rec.extend(0i32.to_le_bytes()),
                Shape::Point([x, y]) => {
                    rec.extend(1i32.to_le_bytes());
                    rec.extend(x.to_le_bytes());
                    rec.extend(y.to_le_bytes());
                }
                Shape::Line(pts) => {
                    rec.extend(3i32.to_le_bytes());
                    rec.extend([0u8; 32]);
                    rec.extend(1i32.to_le_bytes());
                    rec.extend((pts.len() as i32).to_le_bytes());
                    rec.extend(0i32.to_le_bytes());
                    for [x, y] in pts {
                        rec.extend(x.to_le_bytes());
                        rec.extend(y.to_le_bytes());
                    }
                }
            }
            body.extend((k as i32 + 1).to_be_bytes());
            body.extend(((rec.len() / 2) as i32).to_be_bytes());
            body.extend(rec);
        }
        let mut out = vec![0u8; 100];
        out[..4].copy_from_slice(&9994i32.to_be_bytes());
        out[24..28].copy_from_slice(&(((100 + body.len()) / 2) as i32).to_be_bytes());
        out.extend(body);
        out
    }

    /// Writes a small dBase table of text fields (for tests).
    pub fn write_dbf(fields: &[(&str, char)], rows: &[Vec<&str>]) -> Vec<u8> {
        let w = 12usize;
        let header = 32 + 32 * fields.len() + 1;
        let rec_len = 1 + w * fields.len();
        let mut out = vec![0u8; 32];
        out[0] = 3;
        out[4..8].copy_from_slice(&(rows.len() as u32).to_le_bytes());
        out[8..10].copy_from_slice(&(header as u16).to_le_bytes());
        out[10..12].copy_from_slice(&(rec_len as u16).to_le_bytes());
        for (name, t) in fields {
            let mut d = [0u8; 32];
            d[..name.len()].copy_from_slice(name.as_bytes());
            d[11] = *t as u8;
            d[16] = w as u8;
            out.extend(d);
        }
        out.push(0x0D);
        for r in rows {
            out.push(b' ');
            for v in r {
                out.extend(format!("{v:<w$}").as_bytes());
            }
        }
        out
    }

    #[test]
    fn reads_points_lines_and_attributes() {
        let shp = write_shp(&[
            Shape::Line(vec![[0.0, 0.0], [5.0, 1.0], [10.0, 0.0]]),
            Shape::Point([3.0, 4.0]),
        ]);
        assert_eq!(
            read_shp(&shp).unwrap(),
            vec![
                Shape::Line(vec![[0.0, 0.0], [5.0, 1.0], [10.0, 0.0]]),
                Shape::Point([3.0, 4.0])
            ]
        );
        let dbf = read_dbf(&write_dbf(
            &[("ID", 'C'), ("DIAM", 'N')],
            &[vec!["P1", "150"], vec!["P2", "200"]],
        ))
        .unwrap();
        assert_eq!(dbf.fields, ["ID", "DIAM"]);
        assert_eq!(dbf.numeric, [false, true]);
        assert_eq!(dbf.text(1, dbf.field_index("id")), Some("P2"));
        assert_eq!(dbf.number(0, Some(1)), Some(150.0));
    }

    #[test]
    fn reads_crs_from_prj() {
        let greek = r#"PROJCS["GGRS_1987_Greek_Grid",GEOGCS["GCS_GGRS_1987",DATUM["D_GGRS_1987",SPHEROID["GRS_1980",6378137.0,298.257222101]],PRIMEM["Greenwich",0.0],UNIT["Degree",0.0174532925199433]],PROJECTION["Transverse_Mercator"],UNIT["Meter",1.0]]"#;
        assert_eq!(read_prj(greek), (Some(2100), Some("Meters")));
        let utm = r#"PROJCS["WGS 84 / UTM zone 36N",GEOGCS["WGS 84",AUTHORITY["EPSG","4326"]],UNIT["metre",1],AUTHORITY["EPSG","32636"]]"#;
        assert_eq!(read_prj(utm), (Some(32636), Some("Meters")));
        let wgs = r#"GEOGCS["GCS_WGS_1984",DATUM["D_WGS_1984",SPHEROID["WGS_1984",6378137.0,298.257223563]],PRIMEM["Greenwich",0.0],UNIT["Degree",0.0174532925199433]]"#;
        assert_eq!(read_prj(wgs), (Some(4326), Some("Degrees")));
    }
}
