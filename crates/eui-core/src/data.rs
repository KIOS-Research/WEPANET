//! Time patterns and data curves with their descriptions, as EPANET-UI's
//! selectors and editors show them, and where the network uses them (to
//! rename them everywhere, or not delete one in use).

use crate::network::{Network, tokenize_line};
use crate::props::{Element, PropKey};

/// A time pattern: ID, description (the comment line above it) and
/// multipliers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimePattern {
    pub id: String,
    pub description: String,
    pub values: Vec<f64>,
}

/// A data curve: ID, type (`PUMP`, `EFFICIENCY`, `VOLUME`, `HEADLOSS`,
/// `VALVE` or empty for a generic one), description and points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataCurve {
    pub id: String,
    pub kind: String,
    pub description: String,
    pub points: Vec<[f64; 2]>,
}

/// The curve types EPANET records in the comment above a curve.
pub const CURVE_TYPES: [&str; 5] = ["PUMP", "EFFICIENCY", "VOLUME", "HEADLOSS", "VALVE"];

/// Element properties that name a time pattern.
const PATTERN_PROPS: [&str; 4] = [
    "Demand Pattern",
    "Elev. Pattern",
    "Speed Pattern",
    "Price Pattern",
];

/// Element properties that name a data curve.
const CURVE_PROPS: [&str; 3] = ["Volume Curve", "Pump Curve", "Effic. Curve"];

/// Lines of other sections that name a pattern or curve: section, the
/// column holding it, and the leading keywords the line must have.
type RawRef = (&'static str, usize, &'static [&'static str]);

const PATTERN_LINES: [RawRef; 4] = [
    ("DEMANDS", 2, &[]),
    ("SOURCES", 3, &[]),
    ("ENERGY", 2, &["GLOBAL", "PATTERN"]),
    ("OPTIONS", 1, &["PATTERN"]),
];

const CURVE_LINES: [RawRef; 1] = [("ENERGY", 2, &["GLOBAL", "EFFIC"])];

fn is_header(comment: &str) -> bool {
    comment.trim_start().to_ascii_uppercase().starts_with("ID ")
}

fn line(tokens: &[String]) -> String {
    let t: Vec<String> = tokens
        .iter()
        .map(|t| {
            if t.contains(char::is_whitespace) {
                format!("\"{t}\"")
            } else {
                t.clone()
            }
        })
        .collect();
    format!(" {}", t.join("\t"))
}

impl Network {
    /// The time patterns in file order, with their descriptions.
    pub fn time_patterns(&self) -> Vec<TimePattern> {
        let mut out: Vec<TimePattern> = Vec::new();
        let mut comment = String::new();
        for s in self.raw.iter().filter(|s| s.name == "PATTERNS") {
            for l in &s.lines {
                let (t, c) = tokenize_line(l);
                if t.is_empty() {
                    if !c.is_empty() && !is_header(&c) {
                        comment = c;
                    }
                    continue;
                }
                let vals = t[1..].iter().filter_map(|v| v.parse::<f64>().ok());
                match out.iter_mut().find(|p| p.id == t[0]) {
                    Some(p) => p.values.extend(vals),
                    None => out.push(TimePattern {
                        id: t[0].clone(),
                        description: std::mem::take(&mut comment),
                        values: vals.collect(),
                    }),
                }
            }
        }
        out
    }

    /// Writes the time patterns (six multipliers to a line, each pattern's
    /// description on the line above it).
    pub fn set_time_patterns(&mut self, patterns: &[TimePattern]) {
        let mut lines = vec![";ID              \tMultipliers".to_string()];
        for p in patterns {
            if !p.description.trim().is_empty() {
                lines.push(format!(";{}", p.description.trim()));
            }
            for chunk in p.values.chunks(6) {
                let v: Vec<String> = chunk.iter().map(|x| format!("{x}")).collect();
                lines.push(format!(" {:<16}\t{}", p.id, v.join("\t")));
            }
        }
        self.replace_section("PATTERNS", lines);
    }

    /// The data curves in file order. EPANET writes `;TYPE: description`
    /// above a curve.
    pub fn data_curves(&self) -> Vec<DataCurve> {
        let mut out: Vec<DataCurve> = Vec::new();
        let mut comment = String::new();
        for s in self.raw.iter().filter(|s| s.name == "CURVES") {
            for l in &s.lines {
                let (t, c) = tokenize_line(l);
                if t.is_empty() {
                    if !c.is_empty() && !is_header(&c) {
                        comment = c;
                    }
                    continue;
                }
                let (Some(x), Some(y)) = (
                    t.get(1).and_then(|v| v.parse().ok()),
                    t.get(2).and_then(|v| v.parse().ok()),
                ) else {
                    continue;
                };
                match out.iter_mut().find(|c| c.id == t[0]) {
                    Some(c) => c.points.push([x, y]),
                    None => {
                        let c = std::mem::take(&mut comment);
                        let (kind, description) = match c.split_once(':') {
                            Some((k, d))
                                if CURVE_TYPES
                                    .contains(&k.trim().to_ascii_uppercase().as_str()) =>
                            {
                                (k.trim().to_ascii_uppercase(), d.trim().to_string())
                            }
                            _ => (String::new(), c),
                        };
                        out.push(DataCurve {
                            id: t[0].clone(),
                            kind,
                            description,
                            points: vec![[x, y]],
                        });
                    }
                }
            }
        }
        out
    }

    pub fn set_data_curves(&mut self, curves: &[DataCurve]) {
        let mut lines = vec![";ID              \tX-Value     \tY-Value".to_string()];
        for c in curves {
            let d = c.description.trim();
            match (c.kind.is_empty(), d.is_empty()) {
                (false, _) => lines.push(format!(";{}: {d}", c.kind).trim_end().to_string()),
                (true, false) => lines.push(format!(";{d}")),
                (true, true) => {}
            }
            for [x, y] in &c.points {
                lines.push(format!(" {:<16}\t{x:<12}\t{y}", c.id));
            }
        }
        self.replace_section("CURVES", lines);
    }

    /// Where a pattern is used, e.g. "Junction 10 (Demand Pattern)".
    pub fn pattern_users(&self, id: &str) -> Vec<String> {
        self.users(id, &PATTERN_PROPS, &PATTERN_LINES)
    }

    /// Where a curve is used, e.g. "Pump 9 (Pump Curve)".
    pub fn curve_users(&self, id: &str) -> Vec<String> {
        let mut out = self.users(id, &CURVE_PROPS, &CURVE_LINES);
        // a GPV's setting is its head loss curve
        for l in &self.links {
            if l.kind == crate::LinkKind::Valve
                && l.field(1).eq_ignore_ascii_case("GPV")
                && l.field(2) == id
            {
                out.push(format!("Valve {} (Head Loss Curve)", l.id));
            }
        }
        out
    }

    fn users(&self, id: &str, props: &[&str], lines: &[RawRef]) -> Vec<String> {
        let mut out = Vec::new();
        for e in self.elements() {
            for p in self.element_props(e) {
                if props.contains(&p.name) && p.value == id {
                    out.push(format!("{} ({})", self.element_label(e), p.name));
                }
            }
        }
        for &(sec, col, kw) in lines {
            for t in self.raw_lines(sec) {
                if raw_ref(&t, col, kw) && t[col] == id {
                    out.push(format!("[{sec}] {}", t.join(" ")));
                }
            }
        }
        out
    }

    /// Renames a pattern, and every use of it.
    pub fn rename_pattern(&mut self, old: &str, new: &str) {
        self.rename_used(old, new, &PATTERN_PROPS, &PATTERN_LINES);
        let mut pats = self.time_patterns();
        for p in pats.iter_mut().filter(|p| p.id == old) {
            p.id = new.to_string();
        }
        self.set_time_patterns(&pats);
    }

    /// Renames a curve, and every use of it.
    pub fn rename_curve(&mut self, old: &str, new: &str) {
        self.rename_used(old, new, &CURVE_PROPS, &CURVE_LINES);
        for l in &mut self.links {
            if l.kind == crate::LinkKind::Valve
                && l.field(1).eq_ignore_ascii_case("GPV")
                && l.field(2) == old
                && let Some(f) = l.fields.get_mut(2)
            {
                *f = new.to_string();
            }
        }
        let mut curves = self.data_curves();
        for c in curves.iter_mut().filter(|c| c.id == old) {
            c.id = new.to_string();
        }
        self.set_data_curves(&curves);
    }

    fn rename_used(&mut self, old: &str, new: &str, props: &[&str], lines: &[RawRef]) {
        let mut changes: Vec<(Element, PropKey)> = Vec::new();
        for e in self.elements() {
            for p in self.element_props(e) {
                if props.contains(&p.name) && p.value == old {
                    changes.push((e, p.key));
                }
            }
        }
        for (e, key) in changes {
            let _ = self.set_prop(e, key, new);
        }
        for s in &mut self.raw {
            for &(sec, col, kw) in lines {
                if s.name != sec {
                    continue;
                }
                for l in &mut s.lines {
                    let (mut t, comment) = tokenize_line(l);
                    if raw_ref(&t, col, kw) && t[col] == old {
                        t[col] = new.to_string();
                        *l = line(&t);
                        if !comment.is_empty() {
                            l.push_str(&format!("\t;{comment}"));
                        }
                    }
                }
            }
        }
    }

    fn elements(&self) -> Vec<Element> {
        (0..self.nodes.len())
            .map(Element::Node)
            .chain((0..self.links.len()).map(Element::Link))
            .collect()
    }

    fn element_label(&self, e: Element) -> String {
        match e {
            Element::Node(i) => format!("{} {}", self.nodes[i].kind.label(), self.nodes[i].id),
            Element::Link(i) => format!("{} {}", self.links[i].kind.label(), self.links[i].id),
        }
    }
}

/// Whether a line names a pattern or curve in column `col` (after the
/// keywords `kw`).
fn raw_ref(t: &[String], col: usize, kw: &[&str]) -> bool {
    t.len() > col
        && kw
            .iter()
            .enumerate()
            .all(|(k, w)| t[k].eq_ignore_ascii_case(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;

    #[test]
    fn patterns_and_curves_with_descriptions() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let p = net.time_patterns();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].description, "Demand Pattern");
        assert_eq!(p[0].values.len(), 12);
        let c = net.data_curves();
        assert_eq!(c[0].kind, "PUMP");
        assert_eq!(c[0].description, "Pump Curve for Pump 9");
        // written back as read
        net.set_time_patterns(&p);
        net.set_data_curves(&c);
        assert_eq!(net.time_patterns(), p);
        assert_eq!(net.data_curves(), c);
        let again = Network::from_inp(&net.to_inp()).unwrap();
        assert_eq!(again.time_patterns(), p);
    }

    #[test]
    fn renames_and_finds_uses() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let users = net.pattern_users("1");
        // Net1's junctions use the default pattern of [OPTIONS]
        assert_eq!(users, ["[OPTIONS] Pattern 1"]);
        assert!(
            net.curve_users("1")
                .iter()
                .any(|u| u == "Pump 9 (Pump Curve)")
        );
        net.rename_pattern("1", "DAY");
        net.rename_curve("1", "PC");
        assert!(net.pattern_users("1").is_empty());
        assert!(!net.pattern_users("DAY").is_empty());
        assert_eq!(net.data_curves()[0].id, "PC");
        assert!(
            net.curve_users("PC")
                .iter()
                .any(|u| u == "Pump 9 (Pump Curve)")
        );
        // still a valid project
        let again = Network::from_inp(&net.to_inp()).unwrap();
        assert_eq!(again.time_patterns()[0].id, "DAY");
    }
}
