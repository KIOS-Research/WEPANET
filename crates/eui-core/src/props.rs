//! Element and analysis-option properties, as listed in EPANET-UI's
//! Property Editor (see `properties.pas` in the Lazarus version).
//!
//! Many properties do not live on the element's own INP line but in other
//! sections ([TAGS], [EMITTERS], [REACTIONS], [ENERGY], ...). This module
//! hides that: every property is read and written through a [`PropKey`].

use crate::network::{LinkKind, Network, NodeKind, RawSection, tokenize_line};
use crate::num;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropKind {
    Text,
    Number,
    Choice(&'static [&'static str]),
    ReadOnly,
}

/// Where a property's value is stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropKey {
    Id,
    StartNode,
    EndNode,
    Description,
    Tag,
    /// Column of the element's own INP line (after the ID / end nodes).
    Field(usize),
    /// Keyword/value pair on a pump line (HEAD, POWER, SPEED, PATTERN).
    PumpParam(&'static str),
    /// `id v1 v2 ...` line in a section; column 1 is the first value.
    Keyed(&'static str, usize),
    /// Everything after the ID on an `id ...` line (e.g. [SOURCES]).
    KeyedRest(&'static str),
    /// `KEYWORD id value` line in [REACTIONS] (BULK, WALL, TANK).
    Reaction(&'static str),
    /// `PUMP id KEYWORD value` line in [ENERGY] (EFFIC, PRICE, PATTERN).
    PumpEnergy(&'static str),
    /// Number of [DEMANDS] categories (read only).
    DemandCategories,
}

#[derive(Clone, Debug)]
pub struct Prop {
    pub name: &'static str,
    pub value: String,
    pub kind: PropKind,
    pub key: PropKey,
}

/// What a property row refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    Node(usize),
    Link(usize),
}

const TANK_MIXING: &[&str] = &["", "MIXED", "2COMP", "FIFO", "LIFO"];
const PIPE_STATUS: &[&str] = &["Open", "Closed", "CV"];
const FIXED_STATUS: &[&str] = &["", "Open", "Closed"];
const VALVE_TYPES: &[&str] = &["PRV", "PSV", "PBV", "FCV", "TCV", "GPV", "PCV"];
const YES_NO: &[&str] = &["", "Yes", "No"];

fn spec(kind: NodeKind) -> Vec<(&'static str, PropKey, PropKind)> {
    use PropKey::*;
    use PropKind::*;
    let mut v = vec![
        ("ID", Id, Text),
        ("Description", Description, Text),
        ("Tag", Tag, Text),
    ];
    match kind {
        NodeKind::Junction => v.extend([
            ("Elevation", Field(0), Number),
            ("Base Demand", Field(1), Number),
            ("Demand Pattern", Field(2), Text),
            ("Demand Categories", DemandCategories, ReadOnly),
            ("Emitter Coeff.", Keyed("EMITTERS", 1), Number),
            ("Initial Quality", Keyed("QUALITY", 1), Number),
            ("Source Quality", KeyedRest("SOURCES"), Text),
        ]),
        NodeKind::Reservoir => v.extend([
            ("Elevation", Field(0), Number),
            ("Elev. Pattern", Field(1), Text),
            ("Initial Quality", Keyed("QUALITY", 1), Number),
            ("Source Quality", KeyedRest("SOURCES"), Text),
        ]),
        NodeKind::Tank => v.extend([
            ("Elevation", Field(0), Number),
            ("Initial Depth", Field(1), Number),
            ("Minimum Depth", Field(2), Number),
            ("Maximum Depth", Field(3), Number),
            ("Diameter", Field(4), Number),
            ("Minimum Volume", Field(5), Number),
            ("Volume Curve", Field(6), Text),
            ("Can Overflow", Field(7), Choice(YES_NO)),
            ("Mixing Model", Keyed("MIXING", 1), Choice(TANK_MIXING)),
            ("Mixing Fraction", Keyed("MIXING", 2), Number),
            ("Reaction Coeff.", Reaction("TANK"), Number),
            ("Initial Quality", Keyed("QUALITY", 1), Number),
            ("Source Quality", KeyedRest("SOURCES"), Text),
        ]),
    }
    v
}

fn link_spec(kind: LinkKind) -> Vec<(&'static str, PropKey, PropKind)> {
    use PropKey::*;
    use PropKind::*;
    let mut v = vec![
        ("ID", Id, Text),
        ("Start Node", StartNode, ReadOnly),
        ("End Node", EndNode, ReadOnly),
        ("Description", Description, Text),
        ("Tag", Tag, Text),
    ];
    match kind {
        LinkKind::Pipe => v.extend([
            ("Length", Field(0), Number),
            ("Diameter", Field(1), Number),
            ("Roughness", Field(2), Number),
            ("Loss Coeff.", Field(3), Number),
            ("Initial Status", Field(4), Choice(PIPE_STATUS)),
            ("Bulk Coeff.", Reaction("BULK"), Number),
            ("Wall Coeff.", Reaction("WALL"), Number),
            ("Leak Area", Keyed("LEAKAGE", 1), Number),
            ("Leak Expansion", Keyed("LEAKAGE", 2), Number),
        ]),
        LinkKind::Pump => v.extend([
            ("Pump Curve", PumpParam("HEAD"), Text),
            ("Power", PumpParam("POWER"), Number),
            ("Initial Speed", PumpParam("SPEED"), Number),
            ("Speed Pattern", PumpParam("PATTERN"), Text),
            ("Initial Status", Keyed("STATUS", 1), Choice(FIXED_STATUS)),
            ("Effic. Curve", PumpEnergy("EFFIC"), Text),
            ("Energy Price", PumpEnergy("PRICE"), Number),
            ("Price Pattern", PumpEnergy("PATTERN"), Text),
        ]),
        LinkKind::Valve => v.extend([
            ("Diameter", Field(0), Number),
            ("Valve Type", Field(1), Choice(VALVE_TYPES)),
            ("Initial Setting", Field(2), Text),
            ("Loss Coeff.", Field(3), Number),
            ("Fixed Status", Keyed("STATUS", 1), Choice(FIXED_STATUS)),
        ]),
    }
    v
}

impl Network {
    fn element_id(&self, e: Element) -> &str {
        match e {
            Element::Node(i) => &self.nodes[i].id,
            Element::Link(i) => &self.links[i].id,
        }
    }

    /// The editable properties of a node or link, in EPANET-UI order.
    pub fn element_props(&self, e: Element) -> Vec<Prop> {
        let spec = match e {
            Element::Node(i) => spec(self.nodes[i].kind),
            Element::Link(i) => link_spec(self.links[i].kind),
        };
        spec.into_iter()
            .map(|(name, key, kind)| Prop {
                name,
                value: self.get_prop(e, key),
                kind,
                key,
            })
            .collect()
    }

    pub fn get_prop(&self, e: Element, key: PropKey) -> String {
        let id = self.element_id(e);
        match key {
            PropKey::Id => id.to_string(),
            PropKey::StartNode => match e {
                Element::Link(i) => self.links[i].from.clone(),
                _ => String::new(),
            },
            PropKey::EndNode => match e {
                Element::Link(i) => self.links[i].to.clone(),
                _ => String::new(),
            },
            PropKey::Description => match e {
                Element::Node(i) => self.nodes[i].comment.clone(),
                Element::Link(i) => self.links[i].comment.clone(),
            },
            PropKey::Tag => {
                let kw = if matches!(e, Element::Node(_)) {
                    "NODE"
                } else {
                    "LINK"
                };
                self.find_line("TAGS", |t| eq(t, 0, kw) && eq(t, 1, id))
                    .and_then(|t| t.get(2).cloned())
                    .unwrap_or_default()
            }
            PropKey::Field(k) => match e {
                Element::Node(i) => self.nodes[i].field(k).to_string(),
                Element::Link(i) => self.links[i].field(k).to_string(),
            },
            PropKey::PumpParam(kw) => match e {
                Element::Link(i) => pump_param(&self.links[i].fields, kw).unwrap_or_default(),
                _ => String::new(),
            },
            PropKey::Keyed(sec, col) => self
                .find_line(sec, |t| eq(t, 0, id))
                .and_then(|t| t.get(col).cloned())
                .unwrap_or_default(),
            PropKey::KeyedRest(sec) => self
                .find_line(sec, |t| eq(t, 0, id))
                .map(|t| t[1..].join(" "))
                .unwrap_or_default(),
            PropKey::Reaction(kw) => self
                .find_line("REACTIONS", |t| eq(t, 0, kw) && eq(t, 1, id))
                .and_then(|t| t.get(2).cloned())
                .unwrap_or_default(),
            PropKey::PumpEnergy(kw) => self
                .find_line("ENERGY", |t| {
                    eq(t, 0, "PUMP") && eq(t, 1, id) && eq(t, 2, kw)
                })
                .and_then(|t| t.get(3).cloned())
                .unwrap_or_default(),
            PropKey::DemandCategories => {
                let n = self.raw_lines("DEMANDS").filter(|t| eq(t, 0, id)).count();
                if n == 0 { "1".into() } else { n.to_string() }
            }
        }
    }

    /// Sets a property. Returns an error message for invalid input.
    /// An empty value removes optional entries (tags, emitters, ...).
    pub fn set_prop(&mut self, e: Element, key: PropKey, value: &str) -> Result<(), String> {
        let value = value.trim();
        self.check_prop(e, key, value)?;
        let id = self.element_id(e).to_string();
        match key {
            PropKey::Id => {
                let ok = match e {
                    Element::Node(i) => self.rename_node(i, value),
                    Element::Link(i) => self.rename_link(i, value),
                };
                if !ok {
                    return Err(format!("\"{value}\" is empty, invalid or already used"));
                }
            }
            PropKey::StartNode | PropKey::EndNode | PropKey::DemandCategories => {}
            PropKey::Description => {
                let v = value.replace(';', ",");
                match e {
                    Element::Node(i) => self.nodes[i].comment = v,
                    Element::Link(i) => self.links[i].comment = v,
                }
            }
            PropKey::Tag => {
                let kw = if matches!(e, Element::Node(_)) {
                    "NODE"
                } else {
                    "LINK"
                };
                let line = (!value.is_empty()).then(|| format!(" {kw} \t{id} \t{value}"));
                self.upsert_line("TAGS", |t| eq(t, 0, kw) && eq(t, 1, &id), line);
            }
            PropKey::Field(k) => {
                let fields = match e {
                    Element::Node(i) => &mut self.nodes[i].fields,
                    Element::Link(i) => &mut self.links[i].fields,
                };
                if fields.len() <= k {
                    fields.resize(k + 1, String::new());
                }
                fields[k] = value.to_string();
            }
            PropKey::PumpParam(kw) => {
                if let Element::Link(i) = e {
                    set_pump_param(&mut self.links[i].fields, kw, value);
                }
            }
            PropKey::Keyed(sec, col) => {
                // Clearing the first value removes the entry altogether.
                let line = if value.is_empty() && col == 1 {
                    None
                } else {
                    let mut tokens = self
                        .find_line(sec, |t| eq(t, 0, &id))
                        .unwrap_or_else(|| vec![id.clone()]);
                    while tokens.len() <= col {
                        tokens.push(default_for(sec, tokens.len()).to_string());
                    }
                    tokens[col] = value.to_string();
                    Some(join_line(&tokens))
                };
                self.upsert_line(sec, |t| eq(t, 0, &id), line);
            }
            PropKey::KeyedRest(sec) => {
                let line = (!value.is_empty()).then(|| format!(" {id} \t{value}"));
                self.upsert_line(sec, |t| eq(t, 0, &id), line);
            }
            PropKey::Reaction(kw) => {
                let line = (!value.is_empty()).then(|| format!(" {kw} \t{id} \t{value}"));
                self.upsert_line("REACTIONS", |t| eq(t, 0, kw) && eq(t, 1, &id), line);
            }
            PropKey::PumpEnergy(kw) => {
                let line = (!value.is_empty()).then(|| format!(" PUMP \t{id} \t{kw} \t{value}"));
                self.upsert_line(
                    "ENERGY",
                    |t| eq(t, 0, "PUMP") && eq(t, 1, &id) && eq(t, 2, kw),
                    line,
                );
            }
        }
        Ok(())
    }

    /// Checks a new property value: numbers within the range EPANET accepts
    /// and tank levels that stay consistent.
    fn check_prop(&self, e: Element, key: PropKey, value: &str) -> Result<(), String> {
        use PropKey::*;
        let range = match e {
            Element::Node(i) => match (self.nodes[i].kind, key) {
                (NodeKind::Tank, Field(k)) if k <= 5 => Range::NonNegative,
                (NodeKind::Tank, Keyed("MIXING", 2) | Keyed("QUALITY", 1)) => Range::NonNegative,
                _ => Range::Any,
            },
            Element::Link(i) => match (self.links[i].kind, key) {
                (LinkKind::Pipe, Field(k)) if k <= 2 => Range::Positive,
                (LinkKind::Pipe, Field(3) | Keyed("LEAKAGE", _)) => Range::NonNegative,
                (LinkKind::Pump, PumpParam("POWER" | "SPEED") | PumpEnergy("PRICE")) => {
                    Range::NonNegative
                }
                (LinkKind::Valve, Field(0 | 3)) => Range::NonNegative,
                // a GPV's setting is the ID of its head loss curve
                (LinkKind::Valve, Field(2))
                    if !self.links[i].field(1).eq_ignore_ascii_case("GPV") =>
                {
                    Range::NonNegative
                }
                _ => Range::Any,
            },
        };
        // columns of the element's own INP line cannot be left empty
        let required = matches!(key, Field(_)) && range != Range::Any;
        if range == Range::Any || (value.is_empty() && !required) {
            return Ok(());
        }
        let x = num::parse(value).ok_or_else(|| format!("\"{value}\" is not a valid number"))?;
        range.check(x)?;
        if let (Element::Node(i), Field(k @ 1..=3)) = (e, key)
            && self.nodes[i].kind == NodeKind::Tank
        {
            let n = &self.nodes[i];
            let level = |j: usize| if j == k { Some(x) } else { n.num(j) };
            if let (Some(init), Some(min), Some(max)) = (level(1), level(2), level(3))
                && !(min <= init && init <= max && min < max)
            {
                return Err("Inconsistent tank levels.".into());
            }
        }
        Ok(())
    }

    /// Checks a new analysis option value (empty restores EPANET's default).
    pub fn check_option(&self, section: &str, key: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(());
        }
        if section == "TIMES" && key != "STATISTIC" {
            return match crate::analysis::parse_time(value) {
                Some(_) => Ok(()),
                None => Err("Invalid time value.".into()),
            };
        }
        let range = match key {
            "TRIALS" | "ACCURACY" | "EMITTER EXPONENT" => Range::Positive,
            "HEADERROR" | "FLOWCHANGE" | "CHECKFREQ" | "MAXCHECK" | "DAMPLIMIT"
            | "DEMAND MULTIPLIER" | "REQUIRED PRESSURE" | "MINIMUM PRESSURE"
            | "PRESSURE EXPONENT" | "GLOBAL EFFICIENCY" | "GLOBAL PRICE" | "DEMAND CHARGE" => {
                Range::NonNegative
            }
            _ => return Ok(()),
        };
        let x = num::parse(value).ok_or_else(|| format!("\"{value}\" is not a valid number"))?;
        range.check(x)?;
        let pressure = |key: &str, default: f64| {
            self.option("OPTIONS", key)
                .and_then(|v| num::parse(&v))
                .unwrap_or(default)
        };
        match key {
            "GLOBAL EFFICIENCY" if x > 100.0 => Err("Value > 100 not allowed.".into()),
            "REQUIRED PRESSURE" if x < pressure("MINIMUM PRESSURE", 0.0) => {
                Err("Service pressure must be greater than minimum pressure.".into())
            }
            "MINIMUM PRESSURE" if x > pressure("REQUIRED PRESSURE", 0.1) => {
                Err("Minimum pressure must be less than service pressure.".into())
            }
            _ => Ok(()),
        }
    }

    // ----- raw section helpers -------------------------------------------

    /// Tokens of every data line in a raw section.
    pub fn raw_lines<'a>(&'a self, section: &'a str) -> impl Iterator<Item = Vec<String>> + 'a {
        self.raw
            .iter()
            .filter(move |s| s.name == section)
            .flat_map(|s| s.lines.iter())
            .map(|l| tokenize_line(l).0)
            .filter(|t| !t.is_empty())
    }

    fn find_line(&self, section: &str, pred: impl Fn(&[String]) -> bool) -> Option<Vec<String>> {
        self.raw_lines(section).find(|t| pred(t))
    }

    /// Replaces (or removes, with `None`) the first line matching `pred`;
    /// appends `line` if nothing matched.
    fn upsert_line(
        &mut self,
        section: &str,
        pred: impl Fn(&[String]) -> bool,
        line: Option<String>,
    ) {
        let sec = match self.raw.iter().position(|s| s.name == section) {
            Some(i) => &mut self.raw[i],
            None => {
                if line.is_none() {
                    return;
                }
                self.raw.push(RawSection {
                    name: section.to_string(),
                    lines: Vec::new(),
                });
                self.raw.last_mut().unwrap()
            }
        };
        let pos = sec.lines.iter().position(|l| {
            let t = tokenize_line(l).0;
            !t.is_empty() && pred(&t)
        });
        match (pos, line) {
            (Some(p), Some(l)) => sec.lines[p] = l,
            (Some(p), None) => {
                sec.lines.remove(p);
            }
            (None, Some(l)) => sec.lines.push(l),
            (None, None) => {}
        }
    }

    // ----- patterns & curves ----------------------------------------------

    /// Time patterns in file order: (ID, multipliers).
    pub fn patterns(&self) -> Vec<(String, Vec<f64>)> {
        let mut out: Vec<(String, Vec<f64>)> = Vec::new();
        for t in self.raw_lines("PATTERNS") {
            let vals = t[1..].iter().filter_map(|v| v.parse().ok());
            match out.iter_mut().find(|(id, _)| *id == t[0]) {
                Some((_, v)) => v.extend(vals),
                None => out.push((t[0].clone(), vals.collect())),
            }
        }
        out
    }

    /// Data curves in file order: (ID, type comment, points).
    pub fn curves(&self) -> Vec<(String, String, Vec<[f64; 2]>)> {
        let mut out: Vec<(String, String, Vec<[f64; 2]>)> = Vec::new();
        let mut pending_type = String::new();
        for s in self.raw.iter().filter(|s| s.name == "CURVES") {
            for l in &s.lines {
                let (t, comment) = tokenize_line(l);
                if t.is_empty() {
                    // EPANET writes ";PUMP: ..." comments before a curve to record its type.
                    if let Some((kind, _)) = comment.split_once(':') {
                        pending_type = kind.trim().to_ascii_uppercase();
                    }
                    continue;
                }
                let (Some(x), Some(y)) = (
                    t.get(1).and_then(|v| v.parse().ok()),
                    t.get(2).and_then(|v| v.parse().ok()),
                ) else {
                    continue;
                };
                match out.iter_mut().find(|(id, _, _)| *id == t[0]) {
                    Some((_, _, pts)) => pts.push([x, y]),
                    None => out.push((
                        t[0].clone(),
                        std::mem::take(&mut pending_type),
                        vec![[x, y]],
                    )),
                }
            }
        }
        out
    }

    pub fn set_patterns(&mut self, patterns: &[(String, Vec<f64>)]) {
        let mut lines = vec![";ID              \tMultipliers".to_string()];
        for (id, vals) in patterns {
            for chunk in vals.chunks(6) {
                let v: Vec<String> = chunk.iter().map(|x| format!("{x}")).collect();
                lines.push(format!(" {id:<16}\t{}", v.join("\t")));
            }
        }
        self.replace_section("PATTERNS", lines);
    }

    pub fn set_curves(&mut self, curves: &[(String, String, Vec<[f64; 2]>)]) {
        let mut lines = vec![";ID              \tX-Value     \tY-Value".to_string()];
        for (id, kind, pts) in curves {
            if !kind.is_empty() {
                lines.push(format!(";{kind}: {id}"));
            }
            for [x, y] in pts {
                lines.push(format!(" {id:<16}\t{x:<12}\t{y}"));
            }
        }
        self.replace_section("CURVES", lines);
    }

    /// Text of a raw section (used by the controls / rules editors).
    pub fn section_text(&self, section: &str) -> String {
        self.raw
            .iter()
            .filter(|s| s.name == section)
            .flat_map(|s| s.lines.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn replace_section(&mut self, section: &str, lines: Vec<String>) {
        let lines: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();
        match self.raw.iter().position(|s| s.name == section) {
            Some(i) => self.raw[i].lines = lines,
            None => self.raw.push(RawSection {
                name: section.to_string(),
                lines,
            }),
        }
    }

    /// Reverses a link's direction (end nodes and vertices).
    pub fn reverse_link(&mut self, i: usize) {
        let l = &mut self.links[i];
        std::mem::swap(&mut l.from, &mut l.to);
        l.vertices.reverse();
    }

    /// Changes a link to another type, keeping its ID, ends and diameter.
    pub fn convert_link(&mut self, i: usize, kind: LinkKind) {
        let diam = match self.links[i].kind {
            LinkKind::Pipe => self.links[i].num(1),
            LinkKind::Valve => self.links[i].num(0),
            LinkKind::Pump => None,
        };
        let l = &mut self.links[i];
        if l.kind == kind {
            return;
        }
        l.kind = kind;
        l.fields = kind.default_fields();
        if let Some(d) = diam {
            match kind {
                LinkKind::Pipe => l.fields[1] = d.to_string(),
                LinkKind::Valve => l.fields[0] = d.to_string(),
                LinkKind::Pump => {}
            }
        }
    }

    /// Straight-line length of a link from its coordinates and vertices.
    pub fn link_map_length(&self, i: usize) -> Option<f64> {
        let l = &self.links[i];
        let (a, b) = self.link_ends(l)?;
        let mut pts = vec![self.nodes[a].pos?];
        pts.extend(l.vertices.iter().copied());
        pts.push(self.nodes[b].pos?);
        Some(
            pts.windows(2)
                .map(|w| ((w[0][0] - w[1][0]).powi(2) + (w[0][1] - w[1][1]).powi(2)).sqrt())
                .sum(),
        )
    }
}

/// The values a numeric property may take.
#[derive(Clone, Copy, PartialEq)]
enum Range {
    Any,
    NonNegative,
    Positive,
}

impl Range {
    fn check(self, x: f64) -> Result<(), String> {
        match self {
            Range::NonNegative if x < 0.0 => Err("Value less than 0 not allowed.".into()),
            Range::Positive if x <= 0.0 => Err("Value <= 0 not allowed.".into()),
            _ => Ok(()),
        }
    }
}

fn eq(t: &[String], i: usize, s: &str) -> bool {
    t.get(i).is_some_and(|v| v.eq_ignore_ascii_case(s))
}

fn join_line(tokens: &[String]) -> String {
    let last = tokens
        .iter()
        .rposition(|t| !t.is_empty())
        .map_or(0, |i| i + 1);
    format!(
        " {}",
        tokens[..last]
            .iter()
            .map(|t| if t.is_empty() { "*" } else { t })
            .collect::<Vec<_>>()
            .join(" \t")
    )
}

fn default_for(section: &str, col: usize) -> &'static str {
    match (section, col) {
        ("MIXING", 1) => "MIXED",
        ("LEAKAGE", 1) => "0",
        ("LEAKAGE", 2) => "0",
        _ => "",
    }
}

fn pump_param(fields: &[String], kw: &str) -> Option<String> {
    let i = fields.iter().position(|f| f.eq_ignore_ascii_case(kw))?;
    fields.get(i + 1).cloned()
}

fn set_pump_param(fields: &mut Vec<String>, kw: &str, value: &str) {
    match fields.iter().position(|f| f.eq_ignore_ascii_case(kw)) {
        Some(i) if value.is_empty() => {
            fields.drain(i..(i + 2).min(fields.len()));
        }
        Some(i) => {
            if i + 1 < fields.len() {
                fields[i + 1] = value.to_string();
            } else {
                fields.push(value.to_string());
            }
        }
        None if !value.is_empty() => {
            fields.push(kw.to_string());
            fields.push(value.to_string());
        }
        None => {}
    }
}

// ----- analysis options ------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionCategory {
    Hydraulics,
    Demands,
    Quality,
    Times,
    Energy,
}

impl OptionCategory {
    pub const ALL: [OptionCategory; 5] = [
        OptionCategory::Hydraulics,
        OptionCategory::Demands,
        OptionCategory::Quality,
        OptionCategory::Times,
        OptionCategory::Energy,
    ];
    pub fn label(self) -> &'static str {
        match self {
            OptionCategory::Hydraulics => "Hydraulics",
            OptionCategory::Demands => "Demands",
            OptionCategory::Quality => "Water Quality",
            OptionCategory::Times => "Times",
            OptionCategory::Energy => "Energy",
        }
    }
}

/// An analysis option: label, INP section, keyword, kind and EPANET's default.
pub struct OptionSpec {
    pub name: &'static str,
    pub section: &'static str,
    pub key: &'static str,
    pub kind: PropKind,
    pub default: &'static str,
}

const fn o(
    name: &'static str,
    section: &'static str,
    key: &'static str,
    kind: PropKind,
    default: &'static str,
) -> OptionSpec {
    OptionSpec {
        name,
        section,
        key,
        kind,
        default,
    }
}

const N: PropKind = PropKind::Number;
const T: PropKind = PropKind::Text;

pub fn option_specs(cat: OptionCategory) -> Vec<OptionSpec> {
    use PropKind::Choice;
    match cat {
        // EPANET-UI keeps the units, head loss model, specific gravity and
        // viscosity in Project Setup
        OptionCategory::Hydraulics => vec![
            o("Maximum Trials", "OPTIONS", "TRIALS", N, "200"),
            o("Accuracy", "OPTIONS", "ACCURACY", N, "0.001"),
            o("Head Tolerance", "OPTIONS", "HEADERROR", N, "0"),
            o("Flow Tolerance", "OPTIONS", "FLOWCHANGE", N, "0"),
            o("If Unbalanced", "OPTIONS", "UNBALANCED", T, "STOP"),
            o(
                "Status Reporting",
                "REPORT",
                "STATUS",
                Choice(&["NO", "YES", "FULL"]),
                "NO",
            ),
            o("CHECKFREQ", "OPTIONS", "CHECKFREQ", N, "2"),
            o("MAXCHECK", "OPTIONS", "MAXCHECK", N, "10"),
            o("DAMPLIMIT", "OPTIONS", "DAMPLIMIT", N, "0"),
        ],
        OptionCategory::Demands => vec![
            o(
                "Demand Model",
                "OPTIONS",
                "DEMAND MODEL",
                Choice(&["DDA", "PDA"]),
                "DDA",
            ),
            o("Default Pattern", "OPTIONS", "PATTERN", T, "1"),
            o(
                "Demand Multiplier",
                "OPTIONS",
                "DEMAND MULTIPLIER",
                N,
                "1.0",
            ),
            o("Service Pressure", "OPTIONS", "REQUIRED PRESSURE", N, "0.1"),
            o("Minimum Pressure", "OPTIONS", "MINIMUM PRESSURE", N, "0"),
            o(
                "Pressure Exponent",
                "OPTIONS",
                "PRESSURE EXPONENT",
                N,
                "0.5",
            ),
            o("Emitter Exponent", "OPTIONS", "EMITTER EXPONENT", N, "0.5"),
            o(
                "Emitter Can Backflow",
                "OPTIONS",
                "EMITTER BACKFLOW",
                Choice(&["YES", "NO"]),
                "YES",
            ),
        ],
        // edited in the Single Species Quality Editor (see `quality_specs`)
        OptionCategory::Quality => Vec::new(),
        OptionCategory::Times => vec![
            o("Duration", "TIMES", "DURATION", T, "0:00"),
            o("Hydraulic Step", "TIMES", "HYDRAULIC TIMESTEP", T, "1:00"),
            o("Quality Step", "TIMES", "QUALITY TIMESTEP", T, "0:05"),
            o("Pattern Step", "TIMES", "PATTERN TIMESTEP", T, "1:00"),
            o("Pattern Start", "TIMES", "PATTERN START", T, "0:00"),
            o("Report Step", "TIMES", "REPORT TIMESTEP", T, "1:00"),
            o("Report Start", "TIMES", "REPORT START", T, "0:00"),
            o("Rule Step", "TIMES", "RULE TIMESTEP", T, "0:06"),
            o("Clock Start", "TIMES", "START CLOCKTIME", T, "0:00"),
            o(
                "Statistic",
                "TIMES",
                "STATISTIC",
                Choice(&["NONE", "AVERAGED", "MINIMUM", "MAXIMUM", "RANGE"]),
                "NONE",
            ),
        ],
        OptionCategory::Energy => vec![
            o(
                "Pump Efficiency (%)",
                "ENERGY",
                "GLOBAL EFFICIENCY",
                N,
                "75",
            ),
            o("Energy Price / kwh", "ENERGY", "GLOBAL PRICE", N, "0"),
            o("Price Pattern", "ENERGY", "GLOBAL PATTERN", T, ""),
            o("Demand Charge", "ENERGY", "DEMAND CHARGE", N, "0"),
        ],
    }
}

/// The single species water quality options of EPANET-UI's Single Species
/// Quality Editor (besides the QUALITY keyword, which packs model, chemical
/// name, units and trace node: see `quality_parts` / `set_quality_parts`).
pub fn quality_specs() -> Vec<OptionSpec> {
    use PropKind::Choice;
    vec![
        o("Quality Tolerance", "OPTIONS", "TOLERANCE", N, "0.01"),
        o("Bulk Reaction Order", "REACTIONS", "ORDER BULK", N, "1"),
        o("Tank Reaction Order", "REACTIONS", "ORDER TANK", N, "1"),
        o(
            "Wall Reaction Order",
            "REACTIONS",
            "ORDER WALL",
            Choice(&["0", "1"]),
            "1",
        ),
        o(
            "Limiting Concentration",
            "REACTIONS",
            "LIMITING POTENTIAL",
            N,
            "0",
        ),
        o("Specific Diffusivity", "OPTIONS", "DIFFUSIVITY", N, "1.0"),
        o("Global Bulk Coeff.", "REACTIONS", "GLOBAL BULK", N, "0"),
        o("Global Wall Coeff.", "REACTIONS", "GLOBAL WALL", N, "0"),
        o(
            "Wall Coeff. Correlation",
            "REACTIONS",
            "ROUGHNESS CORRELATION",
            N,
            "0",
        ),
    ]
}

/// The QUALITY option split into (model, chemical name, mass units, trace node).
pub fn quality_parts(net: &Network) -> (String, String, String, String) {
    let q = net
        .option("OPTIONS", "QUALITY")
        .unwrap_or_else(|| "NONE".into());
    let t: Vec<&str> = q.split_whitespace().collect();
    let first = t
        .first()
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "NONE".into());
    match first.as_str() {
        "NONE" => ("None".into(), String::new(), String::new(), String::new()),
        "AGE" => ("Age".into(), String::new(), String::new(), String::new()),
        "TRACE" => (
            "Trace".into(),
            String::new(),
            String::new(),
            t.get(1).unwrap_or(&"").to_string(),
        ),
        _ => (
            "Chemical".into(),
            t[0].to_string(),
            t.get(1).map_or_else(
                || net.option("OPTIONS", "MASS UNITS").unwrap_or_default(),
                |s| s.to_string(),
            ),
            String::new(),
        ),
    }
}

pub fn set_quality_parts(net: &mut Network, model: &str, chem: &str, units: &str, trace: &str) {
    let v = match model {
        "Age" => "Age".to_string(),
        "Trace" => format!(
            "Trace {}",
            if trace.is_empty() {
                net.nodes.first().map_or("", |n| &n.id)
            } else {
                trace
            }
        ),
        "Chemical" => {
            let chem = if chem.is_empty() { "Chemical" } else { chem };
            let units = if units.is_empty() { "mg/L" } else { units };
            format!("{chem} {units}")
        }
        _ => "None".into(),
    };
    net.set_option("OPTIONS", "QUALITY", &v);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;

    #[test]
    fn reads_net1_properties() {
        let net = Network::from_inp(examples::NET1).unwrap();
        let p = net.link_index("10").unwrap();
        let props = net.element_props(Element::Link(p));
        let get = |n: &str| props.iter().find(|x| x.name == n).unwrap().value.clone();
        assert_eq!(get("Length"), "10530");
        assert_eq!(get("Diameter"), "18");
        let pump = net.link_index("9").unwrap();
        assert_eq!(
            net.get_prop(Element::Link(pump), PropKey::PumpParam("HEAD")),
            "1"
        );
        let j = net.node_index("11").unwrap();
        assert_eq!(
            net.get_prop(Element::Node(j), PropKey::Keyed("QUALITY", 1)),
            "0.5"
        );
        assert_eq!(net.patterns().len(), 1);
        assert_eq!(net.patterns()[0].1.len(), 12);
        assert_eq!(net.curves().len(), 1);
    }

    #[test]
    fn writes_properties_to_their_sections() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let j = Element::Node(net.node_index("11").unwrap());
        net.set_prop(j, PropKey::Tag, "Zone-A").unwrap();
        net.set_prop(j, PropKey::Keyed("EMITTERS", 1), "0.7")
            .unwrap();
        let p = Element::Link(net.link_index("10").unwrap());
        net.set_prop(p, PropKey::Reaction("BULK"), "-0.9").unwrap();
        let pump = Element::Link(net.link_index("9").unwrap());
        net.set_prop(pump, PropKey::PumpParam("SPEED"), "1.2")
            .unwrap();
        net.set_prop(pump, PropKey::PumpEnergy("PRICE"), "0.1")
            .unwrap();

        let again = Network::from_inp(&net.to_inp()).unwrap();
        assert_eq!(again.get_prop(j, PropKey::Tag), "Zone-A");
        assert_eq!(again.get_prop(j, PropKey::Keyed("EMITTERS", 1)), "0.7");
        assert_eq!(again.get_prop(p, PropKey::Reaction("BULK")), "-0.9");
        assert_eq!(again.get_prop(pump, PropKey::PumpParam("SPEED")), "1.2");
        assert_eq!(again.get_prop(pump, PropKey::PumpEnergy("PRICE")), "0.1");

        net.set_prop(j, PropKey::Tag, "").unwrap();
        assert_eq!(net.get_prop(j, PropKey::Tag), "");
    }

    #[test]
    fn quality_option_roundtrip() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        assert_eq!(quality_parts(&net).0, "Chemical");
        set_quality_parts(&mut net, "Trace", "", "", "9");
        assert_eq!(
            quality_parts(&net),
            ("Trace".into(), "".into(), "".into(), "9".into())
        );
    }

    #[test]
    fn rejects_values_out_of_range() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let pipe = Element::Link(net.link_index("10").unwrap());
        let diam = PropKey::Field(1);
        assert!(net.set_prop(pipe, diam, "0").is_err());
        assert!(net.set_prop(pipe, diam, "").is_err());
        assert!(net.set_prop(pipe, diam, "12.5").is_ok());
        assert_eq!(net.get_prop(pipe, diam), "12.5");
        assert!(net.set_prop(pipe, PropKey::Field(3), "-1").is_err());
        // an empty leak area removes the entry
        assert!(net.set_prop(pipe, PropKey::Keyed("LEAKAGE", 1), "").is_ok());

        let tank = Element::Node(net.node_index("2").unwrap());
        assert!(net.set_prop(tank, PropKey::Field(1), "200").is_err());
        assert!(net.set_prop(tank, PropKey::Field(2), "130").is_err());
        assert!(net.set_prop(tank, PropKey::Field(3), "110").is_err());
        assert!(net.set_prop(tank, PropKey::Field(1), "140").is_ok());
        assert!(net.set_prop(tank, PropKey::Field(4), "-5").is_err());

        let pump = Element::Link(net.link_index("9").unwrap());
        assert!(
            net.set_prop(pump, PropKey::PumpEnergy("PRICE"), "-1")
                .is_err()
        );

        assert!(net.check_option("OPTIONS", "TRIALS", "0").is_err());
        assert!(net.check_option("OPTIONS", "HEADERROR", "0").is_ok());
        assert!(
            net.check_option("ENERGY", "GLOBAL EFFICIENCY", "120")
                .is_err()
        );
        assert!(net.check_option("TIMES", "DURATION", "abc").is_err());
        assert!(net.check_option("TIMES", "DURATION", "48").is_ok());
        assert!(
            net.check_option("OPTIONS", "MINIMUM PRESSURE", "5")
                .is_err()
        );
    }
}
