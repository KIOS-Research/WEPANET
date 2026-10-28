//! Editable EPANET network model with a lossless-enough INP reader/writer.
//!
//! The sections that the UI edits directly (nodes, links, coordinates,
//! vertices and the title) are parsed into typed records. Every other section
//! (patterns, curves, controls, rules, options, ...) is kept verbatim and
//! written back unchanged, so opening and saving a file never drops data.

use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Junction,
    Reservoir,
    Tank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkKind {
    Pipe,
    Pump,
    Valve,
}

impl NodeKind {
    pub const ALL: [NodeKind; 3] = [NodeKind::Junction, NodeKind::Reservoir, NodeKind::Tank];

    pub fn section(self) -> &'static str {
        match self {
            NodeKind::Junction => "JUNCTIONS",
            NodeKind::Reservoir => "RESERVOIRS",
            NodeKind::Tank => "TANKS",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NodeKind::Junction => "Junction",
            NodeKind::Reservoir => "Reservoir",
            NodeKind::Tank => "Tank",
        }
    }

    /// Names of the data columns that follow the node ID in the INP file.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            NodeKind::Junction => &["Elevation", "Demand", "Pattern"],
            NodeKind::Reservoir => &["Total Head", "Head Pattern"],
            NodeKind::Tank => &[
                "Elevation",
                "Initial Level",
                "Minimum Level",
                "Maximum Level",
                "Diameter",
                "Minimum Volume",
                "Volume Curve",
                "Overflow",
            ],
        }
    }

    pub fn default_fields(self) -> Vec<String> {
        let v: &[&str] = match self {
            NodeKind::Junction => &["0", "0", ""],
            NodeKind::Reservoir => &["100", ""],
            NodeKind::Tank => &["0", "10", "0", "20", "50", "0", "", ""],
        };
        v.iter().map(|s| s.to_string()).collect()
    }

    fn prefix(self) -> &'static str {
        match self {
            NodeKind::Junction => "J",
            NodeKind::Reservoir => "R",
            NodeKind::Tank => "T",
        }
    }
}

impl LinkKind {
    pub const ALL: [LinkKind; 3] = [LinkKind::Pipe, LinkKind::Pump, LinkKind::Valve];

    pub fn section(self) -> &'static str {
        match self {
            LinkKind::Pipe => "PIPES",
            LinkKind::Pump => "PUMPS",
            LinkKind::Valve => "VALVES",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LinkKind::Pipe => "Pipe",
            LinkKind::Pump => "Pump",
            LinkKind::Valve => "Valve",
        }
    }

    /// Names of the data columns that follow `ID Node1 Node2` in the INP file.
    /// Pumps have a free-form keyword list, shown as a single field.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            LinkKind::Pipe => &["Length", "Diameter", "Roughness", "Minor Loss", "Status"],
            LinkKind::Pump => &["Parameters"],
            LinkKind::Valve => &["Diameter", "Type", "Setting", "Minor Loss"],
        }
    }

    pub fn default_fields(self) -> Vec<String> {
        let v: &[&str] = match self {
            LinkKind::Pipe => &["1000", "12", "100", "0", "Open"],
            LinkKind::Pump => &["POWER 10"],
            LinkKind::Valve => &["12", "TCV", "0", "0"],
        };
        v.iter().map(|s| s.to_string()).collect()
    }

    fn prefix(self) -> &'static str {
        match self {
            LinkKind::Pipe => "P",
            LinkKind::Pump => "PU",
            LinkKind::Valve => "V",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    /// Raw column values after the ID, see [`NodeKind::fields`].
    pub fields: Vec<String>,
    pub comment: String,
    pub pos: Option<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub id: String,
    pub kind: LinkKind,
    pub from: String,
    pub to: String,
    /// Raw column values after the end nodes, see [`LinkKind::fields`].
    pub fields: Vec<String>,
    pub comment: String,
    pub vertices: Vec<[f64; 2]>,
}

impl Node {
    pub fn field(&self, i: usize) -> &str {
        self.fields.get(i).map(String::as_str).unwrap_or("")
    }
    pub fn num(&self, i: usize) -> Option<f64> {
        self.field(i).parse().ok()
    }
}

impl Link {
    pub fn field(&self, i: usize) -> &str {
        self.fields.get(i).map(String::as_str).unwrap_or("")
    }
    pub fn num(&self, i: usize) -> Option<f64> {
        self.field(i).parse().ok()
    }
}

/// One INP section as a table (see [`Network::data_tables`]).
#[derive(Clone, Debug, PartialEq)]
pub struct DataTable {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// A section kept verbatim.
#[derive(Clone, Debug, PartialEq)]
pub struct RawSection {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Network {
    pub title: Vec<String>,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub raw: Vec<RawSection>,
    /// The project's EPANET-MSX input file, kept with the network (it names
    /// its nodes and links, and undo covers it) but saved to its own file.
    pub msx: Option<crate::MsxFile>,
    /// The coordinate reference system of the coordinates ("EPSG:nnnn" or a
    /// proj string). EPANET has no place for it, so it is kept in a comment,
    /// `;CRS: EPSG:2100`, at the head of [COORDINATES].
    pub crs: Option<String>,
    node_lookup: HashMap<String, usize>,
    link_lookup: HashMap<String, usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum InpError {
    #[error("line {line}: {msg}")]
    Parse { line: usize, msg: String },
}

/// Canonical section order, as written by the EPANET GUI.
const SECTION_ORDER: &[&str] = &[
    "TITLE",
    "JUNCTIONS",
    "RESERVOIRS",
    "TANKS",
    "PIPES",
    "PUMPS",
    "VALVES",
    "TAGS",
    "DEMANDS",
    "STATUS",
    "PATTERNS",
    "CURVES",
    "CONTROLS",
    "RULES",
    "ENERGY",
    "EMITTERS",
    "LEAKAGE",
    "QUALITY",
    "SOURCES",
    "REACTIONS",
    "MIXING",
    "TIMES",
    "REPORT",
    "OPTIONS",
    "COORDINATES",
    "VERTICES",
    "LABELS",
    "BACKDROP",
];

/// Column holding a node (or link) ID in a line of another section, used to
/// rename or drop references. Node and link IDs may coincide (Net1 has node 10
/// and pipe 10), so the element type matters.
fn ref_column(section: &str, t: &[String], is_node: bool) -> Option<usize> {
    let first = t.first()?.to_ascii_uppercase();
    match (section, is_node) {
        ("DEMANDS" | "EMITTERS" | "QUALITY" | "SOURCES" | "MIXING", true) => Some(0),
        ("STATUS" | "LEAKAGE", false) => Some(0),
        ("TAGS", true) if first == "NODE" => Some(1),
        ("TAGS", false) if first == "LINK" => Some(1),
        ("REACTIONS", true) if first == "TANK" => Some(1),
        ("REACTIONS", false) if first == "BULK" || first == "WALL" => Some(1),
        ("ENERGY", false) if first == "PUMP" => Some(1),
        _ => None,
    }
}

fn reference_columns(section: &str, t: &[String], is_node: bool) -> Vec<usize> {
    let first = t
        .first()
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_default();
    match section {
        "CONTROLS" | "RULES" => t
            .windows(2)
            .enumerate()
            .filter_map(|(i, pair)| {
                let kind = pair[0].to_ascii_uppercase();
                let position = if section == "CONTROLS" {
                    i == 0 || i == 4
                } else {
                    i == 1
                };
                let matches = position
                    && if is_node {
                        matches!(kind.as_str(), "NODE" | "JUNCTION" | "RESERVOIR" | "TANK")
                    } else {
                        matches!(kind.as_str(), "LINK" | "PIPE" | "PUMP" | "VALVE")
                    };
                matches.then_some(i + 1)
            })
            .collect(),
        "OPTIONS"
            if is_node
                && first == "QUALITY"
                && t.get(1).is_some_and(|s| s.eq_ignore_ascii_case("TRACE")) =>
        {
            vec![2]
        }
        "REPORT" if (is_node && first == "NODES") || (!is_node && first == "LINKS") => {
            if t.get(1)
                .is_some_and(|s| s.eq_ignore_ascii_case("ALL") || s.eq_ignore_ascii_case("NONE"))
            {
                Vec::new()
            } else {
                (1..t.len()).collect()
            }
        }
        _ => ref_column(section, t, is_node).into_iter().collect(),
    }
}

/// EPANET only compares the first 4 letters of a section name (e.g. `[JUNC]`).
fn canonical_section(name: &str) -> String {
    let up = name.trim().to_ascii_uppercase();
    if let Some(prefix) = up.get(..4)
        && let Some(full) = SECTION_ORDER.iter().find(|s| s.starts_with(prefix))
    {
        return full.to_string();
    }
    up
}

/// Splits an INP line into tokens and trailing comment. Supports quoted tokens.
/// The coordinate reference system a `;CRS: …` comment line names.
fn crs_comment(line: &str) -> Option<String> {
    let rest = line.strip_prefix(';')?.trim_start();
    let (key, value) = rest.split_once(':')?;
    let value = value.trim();
    (key.trim().eq_ignore_ascii_case("CRS") && !value.is_empty()).then(|| value.to_string())
}

/// A section line from its tokens and comment, as [`tokenize_line`] splits it.
fn join_line(tokens: &[String], comment: &str) -> String {
    let mut l = format!(
        " {}",
        tokens
            .iter()
            .map(|t| quote(t))
            .collect::<Vec<_>>()
            .join("\t")
    );
    if !comment.is_empty() {
        let _ = write!(l, "\t;{comment}");
    }
    l
}

pub(crate) fn tokenize_line(line: &str) -> (Vec<String>, String) {
    let (data, comment) = match line.find(';') {
        Some(i) => (&line[..i], line[i + 1..].trim().to_string()),
        None => (line, String::new()),
    };
    let mut tokens = Vec::new();
    let mut chars = data.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let tok: String = chars.by_ref().take_while(|&c| c != '"').collect();
            tokens.push(tok);
        } else {
            let mut tok = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                tok.push(c);
                chars.next();
            }
            tokens.push(tok);
        }
    }
    (tokens, comment)
}

fn quote(s: &str) -> String {
    if s.contains(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

impl Network {
    pub fn new() -> Self {
        let mut net = Network::default();
        net.raw.push(RawSection {
            name: "OPTIONS".into(),
            lines: vec![
                " Units              \tLPS".into(),
                " Headloss           \tH-W".into(),
            ],
        });
        net.raw.push(RawSection {
            name: "TIMES".into(),
            lines: vec![
                " Duration           \t24:00".into(),
                " Hydraulic Timestep \t1:00".into(),
            ],
        });
        net
    }

    /// Parses INP text.
    pub fn from_inp(text: &str) -> Result<Self, InpError> {
        let mut net = Network::default();
        let mut section = String::new();
        let mut coords: Vec<(String, [f64; 2])> = Vec::new();
        let mut verts: Vec<(String, [f64; 2])> = Vec::new();

        for (lineno, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('[') {
                let end = trimmed.find(']').unwrap_or(trimmed.len());
                section = canonical_section(&trimmed[1..end]);
                if section == "END" {
                    break;
                }
                continue;
            }
            if section == "TITLE" {
                net.title.push(trimmed.to_string());
                continue;
            }
            let err = |msg: &str| InpError::Parse {
                line: lineno + 1,
                msg: msg.to_string(),
            };
            let node_kind = NodeKind::ALL.into_iter().find(|k| k.section() == section);
            let link_kind = LinkKind::ALL.into_iter().find(|k| k.section() == section);
            let managed = node_kind.is_some()
                || link_kind.is_some()
                || section == "COORDINATES"
                || section == "VERTICES";

            if !managed {
                match net.raw.iter_mut().find(|s| s.name == section) {
                    Some(s) => s.lines.push(line.trim_end().to_string()),
                    None => net.raw.push(RawSection {
                        name: section.clone(),
                        lines: vec![line.trim_end().to_string()],
                    }),
                }
                continue;
            }

            if section == "COORDINATES"
                && let Some(crs) = crs_comment(trimmed)
            {
                net.crs = Some(crs);
                continue;
            }
            let (tokens, comment) = tokenize_line(trimmed);
            if tokens.is_empty() {
                continue; // comment-only line, e.g. column headers
            }
            if let Some(kind) = node_kind {
                net.nodes.push(Node {
                    id: tokens[0].clone(),
                    kind,
                    fields: tokens[1..].to_vec(),
                    comment,
                    pos: None,
                });
            } else if let Some(kind) = link_kind {
                if tokens.len() < 3 {
                    return Err(err("link needs an ID and two end nodes"));
                }
                net.links.push(Link {
                    id: tokens[0].clone(),
                    kind,
                    from: tokens[1].clone(),
                    to: tokens[2].clone(),
                    fields: tokens[3..].to_vec(),
                    comment,
                    vertices: Vec::new(),
                });
            } else if tokens.len() >= 3 {
                let x: f64 = tokens[1].parse().map_err(|_| err("bad X coordinate"))?;
                let y: f64 = tokens[2].parse().map_err(|_| err("bad Y coordinate"))?;
                if section == "COORDINATES" {
                    coords.push((tokens[0].clone(), [x, y]));
                } else {
                    verts.push((tokens[0].clone(), [x, y]));
                }
            }
        }

        net.rebuild_index();
        for (id, p) in coords {
            if let Some(&i) = net.node_lookup.get(&id) {
                net.nodes[i].pos = Some(p);
            }
        }
        for (id, p) in verts {
            if let Some(&i) = net.link_lookup.get(&id) {
                net.links[i].vertices.push(p);
            }
        }
        Ok(net)
    }

    /// Writes the network as INP text.
    pub fn to_inp(&self) -> String {
        let mut out = String::new();
        let mut written = vec![false; self.raw.len()];
        // Lines before the first section (usually comments) stay at the top.
        if let Some(i) = self.raw.iter().position(|s| s.name.is_empty()) {
            written[i] = true;
            self.write_raw(&mut out, &self.raw[i]);
        }
        for &name in SECTION_ORDER {
            if let Some(i) = self.raw.iter().position(|s| s.name == name) {
                written[i] = true;
                self.write_raw(&mut out, &self.raw[i]);
                continue;
            }
            match name {
                "TITLE" => {
                    out.push_str("[TITLE]\n");
                    for l in &self.title {
                        out.push_str(l);
                        out.push('\n');
                    }
                    out.push('\n');
                }
                "COORDINATES" => self.write_coordinates(&mut out),
                "VERTICES" => self.write_vertices(&mut out),
                _ => {
                    if let Some(k) = NodeKind::ALL.into_iter().find(|k| k.section() == name) {
                        self.write_nodes(&mut out, k);
                    } else if let Some(k) = LinkKind::ALL.into_iter().find(|k| k.section() == name)
                    {
                        self.write_links(&mut out, k);
                    }
                }
            }
        }
        for (i, s) in self.raw.iter().enumerate() {
            if !written[i] {
                self.write_raw(&mut out, s);
            }
        }
        out.push_str("[END]\n");
        out
    }

    /// The project as tables, one per INP section, for EPANET-UI's Project
    /// Data view. Column names come from the section's comment header line
    /// (e.g. `;ID  Elev  Demand`) when there is one.
    pub fn data_tables(&self) -> Vec<DataTable> {
        let mut tables: Vec<DataTable> = Vec::new();
        for line in self.to_inp().lines() {
            let trimmed = line.trim();
            if let Some(name) = trimmed.strip_prefix('[') {
                let name = name.trim_end_matches(']').to_string();
                if name != "END" {
                    tables.push(DataTable {
                        name,
                        columns: Vec::new(),
                        rows: Vec::new(),
                    });
                }
                continue;
            }
            let Some(t) = tables.last_mut() else { continue };
            if trimmed.is_empty() {
                continue;
            }
            if t.name == "TITLE" {
                t.rows.push(vec![trimmed.to_string()]);
                continue;
            }
            let (tokens, comment) = tokenize_line(trimmed);
            if tokens.is_empty() {
                if t.rows.is_empty() && t.columns.is_empty() {
                    t.columns = comment.split_whitespace().map(str::to_string).collect();
                }
                continue;
            }
            t.rows.push(tokens);
        }
        tables
    }

    fn write_raw(&self, out: &mut String, s: &RawSection) {
        if !s.name.is_empty() {
            let _ = writeln!(out, "[{}]", s.name);
        }
        for l in &s.lines {
            out.push_str(l);
            out.push('\n');
        }
        out.push('\n');
    }

    fn write_nodes(&self, out: &mut String, kind: NodeKind) {
        let _ = writeln!(out, "[{}]", kind.section());
        let _ = writeln!(out, ";{:<16}\t{}", "ID", kind.fields().join("\t"));
        for n in self.nodes.iter().filter(|n| n.kind == kind) {
            write_record(out, &[&n.id], &n.fields, &n.comment);
        }
        out.push('\n');
    }

    fn write_links(&self, out: &mut String, kind: LinkKind) {
        let _ = writeln!(out, "[{}]", kind.section());
        let _ = writeln!(
            out,
            ";{:<16}\t{:<16}\t{:<16}\t{}",
            "ID",
            "Node1",
            "Node2",
            kind.fields().join("\t")
        );
        for l in self.links.iter().filter(|l| l.kind == kind) {
            write_record(out, &[&l.id, &l.from, &l.to], &l.fields, &l.comment);
        }
        out.push('\n');
    }

    fn write_coordinates(&self, out: &mut String) {
        out.push_str("[COORDINATES]\n");
        if let Some(crs) = &self.crs {
            let _ = writeln!(out, ";CRS: {crs}");
        }
        let _ = writeln!(out, ";{:<16}\t{:<16}\tY-Coord", "Node", "X-Coord");
        for n in &self.nodes {
            if let Some([x, y]) = n.pos {
                let _ = writeln!(out, " {:<16}\t{:<16}\t{}", quote(&n.id), x, y);
            }
        }
        out.push('\n');
    }

    fn write_vertices(&self, out: &mut String) {
        out.push_str("[VERTICES]\n");
        let _ = writeln!(out, ";{:<16}\t{:<16}\tY-Coord", "Link", "X-Coord");
        for l in &self.links {
            for [x, y] in &l.vertices {
                let _ = writeln!(out, " {:<16}\t{:<16}\t{}", quote(&l.id), x, y);
            }
        }
        out.push('\n');
    }

    // ----- lookups -------------------------------------------------------

    pub fn rebuild_index(&mut self) {
        self.node_lookup = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        self.link_lookup = self
            .links
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect();
    }

    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.node_lookup.get(id).copied()
    }

    pub fn link_index(&self, id: &str) -> Option<usize> {
        self.link_lookup.get(id).copied()
    }

    /// End-node indices of a link.
    pub fn link_ends(&self, link: &Link) -> Option<(usize, usize)> {
        Some((self.node_index(&link.from)?, self.node_index(&link.to)?))
    }

    /// `[min_x, min_y, max_x, max_y]` of all node coordinates and vertices.
    pub fn bounds(&self) -> Option<[f64; 4]> {
        let pts = self
            .nodes
            .iter()
            .filter_map(|n| n.pos)
            .chain(self.links.iter().flat_map(|l| l.vertices.iter().copied()));
        let mut b: Option<[f64; 4]> = None;
        for [x, y] in pts {
            b = Some(match b {
                None => [x, y, x, y],
                Some([a, c, d, e]) => [a.min(x), c.min(y), d.max(x), e.max(y)],
            });
        }
        b
    }

    pub fn count_nodes(&self, kind: NodeKind) -> usize {
        self.nodes.iter().filter(|n| n.kind == kind).count()
    }

    pub fn count_links(&self, kind: LinkKind) -> usize {
        self.links.iter().filter(|l| l.kind == kind).count()
    }

    // ----- options -------------------------------------------------------

    fn section_lines(&self, name: &str) -> &[String] {
        self.raw
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.lines.as_slice())
            .unwrap_or(&[])
    }

    /// Reads a keyword value from [OPTIONS] or [TIMES], e.g. `option("TIMES", "DURATION")`.
    /// Multi-word keywords (e.g. `"DEMAND MODEL"`) are supported.
    pub fn option(&self, section: &str, key: &str) -> Option<String> {
        let n = key.split_whitespace().count();
        self.section_lines(section).iter().find_map(|line| {
            let (tokens, _) = tokenize_line(line);
            (keyword_matches(&tokens, key) && tokens.len() > n).then(|| tokens[n..].join(" "))
        })
    }

    /// Sets (or adds) a keyword value in a keyword section such as [OPTIONS],
    /// [TIMES], [REPORT], [ENERGY] or [REACTIONS]. An empty value removes it,
    /// so EPANET falls back to its default.
    pub fn set_option(&mut self, section: &str, key: &str, value: &str) {
        let matches = |line: &str| keyword_matches(&tokenize_line(line).0, key);
        if value.trim().is_empty() {
            for s in self.raw.iter_mut().filter(|s| s.name == section) {
                s.lines.retain(|l| !matches(l));
            }
            return;
        }
        let new_line = format!(" {key:<20}\t{}", value.trim());
        let sec = match self.raw.iter().position(|s| s.name == section) {
            Some(i) => &mut self.raw[i],
            None => {
                self.raw.push(RawSection {
                    name: section.to_string(),
                    lines: Vec::new(),
                });
                self.raw.last_mut().unwrap()
            }
        };
        for line in sec.lines.iter_mut() {
            if keyword_matches(&tokenize_line(line).0, key) {
                *line = new_line;
                return;
            }
        }
        sec.lines.push(new_line);
    }

    /// Flow units keyword (e.g. `"GPM"`, `"LPS"`).
    pub fn flow_units(&self) -> String {
        self.option("OPTIONS", "UNITS")
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "GPM".into())
    }

    /// True if the network uses SI (metric) units.
    pub fn is_si(&self) -> bool {
        crate::units::is_si_flow(&self.flow_units())
    }

    // ----- editing -------------------------------------------------------

    fn unique_id(&self, prefix: &str, taken: impl Fn(&str) -> bool) -> String {
        (1..)
            .map(|i| format!("{prefix}{i}"))
            .find(|id| !taken(id))
            .expect("infinite iterator")
    }

    pub fn add_node(&mut self, kind: NodeKind, pos: [f64; 2]) -> usize {
        self.add_node_as(kind, pos, kind.prefix())
    }

    /// Adds a node whose ID is `prefix` followed by the first free number.
    pub fn add_node_as(&mut self, kind: NodeKind, pos: [f64; 2], prefix: &str) -> usize {
        let id = self.unique_id(prefix.trim(), |id| self.node_lookup.contains_key(id));
        self.nodes.push(Node {
            id: id.clone(),
            kind,
            fields: kind.default_fields(),
            comment: String::new(),
            pos: Some(pos),
        });
        let i = self.nodes.len() - 1;
        self.node_lookup.insert(id, i);
        i
    }

    pub fn add_link(&mut self, kind: LinkKind, from: usize, to: usize) -> usize {
        self.add_link_as(kind, from, to, kind.prefix())
    }

    /// Adds a link whose ID is `prefix` followed by the first free number.
    pub fn add_link_as(&mut self, kind: LinkKind, from: usize, to: usize, prefix: &str) -> usize {
        let id = self.unique_id(prefix.trim(), |id| self.link_lookup.contains_key(id));
        let mut fields = kind.default_fields();
        if kind == LinkKind::Pipe
            && let (Some(a), Some(b)) = (self.nodes[from].pos, self.nodes[to].pos)
        {
            let len = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
            if len > 0.0 {
                fields[0] = format!("{len:.1}");
            }
        }
        self.links.push(Link {
            id: id.clone(),
            kind,
            from: self.nodes[from].id.clone(),
            to: self.nodes[to].id.clone(),
            fields,
            comment: String::new(),
            vertices: Vec::new(),
        });
        let i = self.links.len() - 1;
        self.link_lookup.insert(id, i);
        i
    }

    /// Splits pipe `idx` with node `node`, moved onto the pipe's segment
    /// `seg` (0 runs from the start node to the first vertex). The pipe keeps
    /// its first part; a copy of it, named `prefix` and the first free number,
    /// takes the rest. The length is shared in proportion to the map distance,
    /// the node's elevation is interpolated between the end nodes, and
    /// the pipe's lines in [STATUS], [TAGS], [REACTIONS] and [LEAKAGE] are
    /// copied for the new pipe. Returns the new pipe's index.
    pub fn split_link(
        &mut self,
        idx: usize,
        seg: usize,
        node: usize,
        prefix: &str,
    ) -> Option<usize> {
        let link = &self.links[idx];
        if link.kind != LinkKind::Pipe {
            return None;
        }
        let (a, b) = self.link_ends(link)?;
        let mut pts = vec![self.nodes[a].pos?];
        pts.extend(link.vertices.iter().copied());
        pts.push(self.nodes[b].pos?);
        let (p, q) = (*pts.get(seg)?, *pts.get(seg + 1)?);
        let dist =
            |p: [f64; 2], q: [f64; 2]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();

        // the point of the segment nearest the junction
        let at = self.nodes[node].pos?;
        let d = [q[0] - p[0], q[1] - p[1]];
        let dd = d[0] * d[0] + d[1] * d[1];
        let t = if dd > 0.0 {
            (((at[0] - p[0]) * d[0] + (at[1] - p[1]) * d[1]) / dd).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let at = [p[0] + d[0] * t, p[1] + d[1] * t];
        let total: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        let before: f64 = pts[..=seg]
            .windows(2)
            .map(|w| dist(w[0], w[1]))
            .sum::<f64>()
            + dist(p, at);
        let share = if total > 0.0 { before / total } else { 0.5 };
        let num = |v: f64| {
            let s = format!("{v:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        };

        let elevation = |n: &Node| match n.kind {
            NodeKind::Reservoir => None,
            _ => n.num(0),
        };
        let elev = match (elevation(&self.nodes[a]), elevation(&self.nodes[b])) {
            (Some(x), Some(y)) => Some(x + (y - x) * share),
            (x, y) => x.or(y),
        };
        let n = &mut self.nodes[node];
        n.pos = Some(at);
        // a reservoir's first field is its head, not an elevation
        if let Some(e) = elev
            && n.kind != NodeKind::Reservoir
        {
            n.fields[0] = num(e);
        }
        let node_id = n.id.clone();

        let id = self.unique_id(prefix.trim(), |id| self.link_lookup.contains_key(id));
        let link = &mut self.links[idx];
        let mut rest = link.clone();
        rest.id = id.clone();
        rest.from = node_id.clone();
        rest.vertices = link.vertices.split_off(seg.min(link.vertices.len()));
        link.to = node_id;
        if let Some(len) = link.fields.first().and_then(|f| f.parse::<f64>().ok()) {
            link.fields[0] = num(len * share);
            rest.fields[0] = num(len * (1.0 - share));
        }
        let old = link.id.clone();
        self.links.push(rest);
        let i = self.links.len() - 1;
        self.link_lookup.insert(id.clone(), i);

        for s in &mut self.raw {
            if !matches!(s.name.as_str(), "STATUS" | "TAGS" | "REACTIONS" | "LEAKAGE") {
                continue;
            }
            let mut copies = Vec::new();
            for line in &s.lines {
                let (mut tokens, comment) = tokenize_line(line);
                if let Some(col) = ref_column(&s.name, &tokens, false)
                    && tokens.get(col) == Some(&old)
                {
                    tokens[col] = id.clone();
                    copies.push(join_line(&tokens, &comment));
                }
            }
            s.lines.extend(copies);
        }
        Some(i)
    }

    /// Deletes a node together with the links attached to it.
    pub fn delete_node(&mut self, idx: usize) {
        let id = self.nodes.remove(idx).id;
        let dead_links: Vec<String> = self
            .links
            .iter()
            .filter(|l| l.from == id || l.to == id)
            .map(|l| l.id.clone())
            .collect();
        self.links.retain(|l| l.from != id && l.to != id);
        self.purge_references(&id, true);
        if let Some(m) = &mut self.msx {
            m.remove(true, &id);
        }
        for l in &dead_links {
            self.purge_references(l, false);
            if let Some(m) = &mut self.msx {
                m.remove(false, l);
            }
        }
        self.rebuild_index();
    }

    pub fn delete_link(&mut self, idx: usize) {
        let id = self.links.remove(idx).id;
        self.purge_references(&id, false);
        if let Some(m) = &mut self.msx {
            m.remove(false, &id);
        }
        self.rebuild_index();
    }

    /// Renames an element and updates every reference to it. Returns false if
    /// the new ID is empty, contains a `;` or is already used.
    pub fn rename_node(&mut self, idx: usize, new_id: &str) -> bool {
        let new_id = new_id.trim();
        if new_id.is_empty() || new_id.contains(';') || self.node_lookup.contains_key(new_id) {
            return false;
        }
        let old = std::mem::replace(&mut self.nodes[idx].id, new_id.to_string());
        for l in &mut self.links {
            if l.from == old {
                l.from = new_id.to_string();
            }
            if l.to == old {
                l.to = new_id.to_string();
            }
        }
        self.rename_references(&old, new_id, true);
        if let Some(m) = &mut self.msx {
            m.rename(true, &old, new_id);
        }
        self.rebuild_index();
        true
    }

    pub fn rename_link(&mut self, idx: usize, new_id: &str) -> bool {
        let new_id = new_id.trim();
        if new_id.is_empty() || new_id.contains(';') || self.link_lookup.contains_key(new_id) {
            return false;
        }
        let old = std::mem::replace(&mut self.links[idx].id, new_id.to_string());
        self.rename_references(&old, new_id, false);
        if let Some(m) = &mut self.msx {
            m.rename(false, &old, new_id);
        }
        self.rebuild_index();
        true
    }

    fn purge_references(&mut self, id: &str, is_node: bool) {
        for s in &mut self.raw {
            let name = s.name.clone();
            if name == "RULES" {
                let mut blocks: Vec<Vec<String>> = Vec::new();
                for line in std::mem::take(&mut s.lines) {
                    if blocks.is_empty()
                        || tokenize_line(&line)
                            .0
                            .first()
                            .is_some_and(|t| t.eq_ignore_ascii_case("RULE"))
                    {
                        blocks.push(Vec::new());
                    }
                    blocks.last_mut().unwrap().push(line);
                }
                s.lines = blocks
                    .into_iter()
                    .filter(|block| {
                        !block.iter().any(|line| {
                            let t = tokenize_line(line).0;
                            reference_columns(&name, &t, is_node)
                                .iter()
                                .any(|&c| t.get(c).map(String::as_str) == Some(id))
                        })
                    })
                    .flatten()
                    .collect();
                continue;
            }
            if name == "REPORT" {
                for line in &mut s.lines {
                    let (mut t, comment) = tokenize_line(line);
                    let columns = reference_columns(&name, &t, is_node);
                    let mut changed = false;
                    for c in columns.into_iter().rev() {
                        if t.get(c).map(String::as_str) == Some(id) {
                            t.remove(c);
                            changed = true;
                        }
                    }
                    if !changed {
                        continue;
                    }
                    if t.len() == 1
                        && ((is_node && t[0].eq_ignore_ascii_case("NODES"))
                            || (!is_node && t[0].eq_ignore_ascii_case("LINKS")))
                    {
                        t.push("NONE".into());
                    }
                    *line = if t.is_empty() {
                        line.clone()
                    } else {
                        format!(
                            " {} ;{}",
                            t.iter().map(|v| quote(v)).collect::<Vec<_>>().join("\t"),
                            comment
                        )
                    };
                }
                continue;
            }
            s.lines.retain(|l| {
                let t = tokenize_line(l).0;
                !reference_columns(&name, &t, is_node)
                    .iter()
                    .any(|&c| t.get(c).map(String::as_str) == Some(id))
            });
        }
    }

    fn rename_references(&mut self, old: &str, new: &str, is_node: bool) {
        for s in &mut self.raw {
            let name = s.name.clone();
            for line in &mut s.lines {
                let (mut tokens, comment) = tokenize_line(line);
                let mut changed = false;
                for col in reference_columns(&name, &tokens, is_node) {
                    if tokens.get(col).map(String::as_str) == Some(old) {
                        tokens[col] = new.to_string();
                        changed = true;
                    }
                }
                if changed {
                    *line = join_line(&tokens, &comment);
                }
            }
        }
    }
}

/// True if a keyword-section line starts with `key` (e.g. "DEMAND MODEL").
/// A shorter key never matches a longer keyword: `PRESSURE` is not
/// `PRESSURE EXPONENT`, `GLOBAL` is not `GLOBAL BULK`.
fn keyword_matches(tokens: &[String], key: &str) -> bool {
    const LONGER: &[&str] = &["PRESSURE EXPONENT"];
    let starts = |k: &str| {
        let words: Vec<&str> = k.split_whitespace().collect();
        tokens.len() >= words.len()
            && tokens
                .iter()
                .zip(&words)
                .all(|(t, w)| t.eq_ignore_ascii_case(w))
    };
    starts(key)
        && !LONGER
            .iter()
            .any(|l| l.len() > key.len() && l.starts_with(&key.to_ascii_uppercase()) && starts(l))
}

fn write_record(out: &mut String, head: &[&str], fields: &[String], comment: &str) {
    out.push(' ');
    for h in head {
        let _ = write!(out, "{:<16}\t", quote(h));
    }
    // Trailing empty optional fields are dropped; an interior blank (e.g. a
    // tank with an overflow flag but no volume curve) is written as `*`,
    // which EPANET reads as "not given".
    let last = fields
        .iter()
        .rposition(|f| !f.trim().is_empty())
        .map_or(0, |i| i + 1);
    for f in &fields[..last] {
        let f = if f.trim().is_empty() { "*" } else { f.trim() };
        let _ = write!(out, "{:<12}\t", f);
    }
    if !comment.is_empty() {
        let _ = write!(out, ";{comment}");
    }
    while out.ends_with('\t') {
        out.pop();
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;

    /// A pipe split by a junction becomes two pipes that share its length in
    /// proportion to the map distance, keep its vertices on their side and
    /// its other properties; the junction lies on the pipe with an
    /// elevation between the end nodes'.
    #[test]
    fn split_pipes() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let p = net.link_index("11").unwrap();
        net.links[p].vertices = vec![[40.0, 80.0]];
        let status = net.raw.iter_mut().find(|s| s.name == "STATUS").unwrap();
        status.lines.push(" 11\tClosed".into());
        let j = net.add_node(NodeKind::Junction, [45.0, 76.0]);
        let q = net.split_link(p, 1, j, "P").unwrap();

        let junction = net.nodes[j].clone();
        assert_eq!(junction.pos, Some([44.5, 75.5]));
        assert_eq!(junction.field(0), "702.75");
        let (first, rest) = (&net.links[p], &net.links[q]);
        assert_eq!(
            (first.from.as_str(), first.to.as_str()),
            ("11", junction.id.as_str())
        );
        assert_eq!(
            (rest.from.as_str(), rest.to.as_str()),
            (junction.id.as_str(), "12")
        );
        assert_eq!(rest.id, "P1");
        assert_eq!(first.vertices, [[40.0, 80.0]]);
        assert!(rest.vertices.is_empty());
        assert_eq!((first.field(0), rest.field(0)), ("3828", "1452"));
        assert_eq!(rest.fields[1..], first.fields[1..]);
        let back = Network::from_inp(&net.to_inp()).unwrap();
        let status = back.raw.iter().find(|s| s.name == "STATUS").unwrap();
        assert!(
            status
                .lines
                .iter()
                .any(|l| l.split_whitespace().eq(["P1", "Closed"]))
        );
        // pumps and valves are not split
        let pump = net.link_index("9").unwrap();
        assert!(net.split_link(pump, 0, j, "P").is_none());
    }

    /// A new valve is a fully open throttle control valve (TCV), as EPANET-UI
    /// adds them, and is written so to the INP file.
    #[test]
    fn new_valves_are_tcvs() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let (a, b) = (net.node_index("10").unwrap(), net.node_index("11").unwrap());
        let v = net.add_link(LinkKind::Valve, a, b);
        assert_eq!(net.links[v].field(1), "TCV");
        assert_eq!(net.links[v].num(2), Some(0.0));
        let id = net.links[v].id.clone();
        let back = Network::from_inp(&net.to_inp()).unwrap();
        let w = back.link_index(&id).unwrap();
        assert_eq!(back.links[w].field(1), "TCV");
    }

    /// The coordinate reference system goes in and out of the INP file in a
    /// comment at the head of [COORDINATES].
    #[test]
    fn keeps_the_coordinate_reference_system() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        assert_eq!(net.crs, None);
        assert!(!net.to_inp().contains(";CRS"));
        net.crs = Some("EPSG:2100".into());
        let text = net.to_inp();
        assert!(text.contains("[COORDINATES]\n;CRS: EPSG:2100\n"), "{text}");
        let back = Network::from_inp(&text).unwrap();
        assert_eq!(back.crs.as_deref(), Some("EPSG:2100"));
        assert_eq!(back.nodes.len(), net.nodes.len());
        assert!(back.nodes.iter().all(|n| n.pos.is_some()));
        // written by hand, in any case, with a proj string
        let text = text.replace(
            ";CRS: EPSG:2100",
            ";  crs:  +proj=utm +zone=36 +datum=WGS84",
        );
        let back = Network::from_inp(&text).unwrap();
        assert_eq!(back.crs.as_deref(), Some("+proj=utm +zone=36 +datum=WGS84"));
        // other comments there are not taken for one
        assert_eq!(crs_comment(";Node  X-Coord  Y-Coord"), None);
        assert_eq!(crs_comment(";CRS:"), None);
    }

    #[test]
    fn parse_net1() {
        let net = Network::from_inp(examples::NET1).unwrap();
        assert_eq!(net.count_nodes(NodeKind::Junction), 9);
        assert_eq!(net.count_nodes(NodeKind::Reservoir), 1);
        assert_eq!(net.count_nodes(NodeKind::Tank), 1);
        assert_eq!(net.count_links(LinkKind::Pipe), 12);
        assert_eq!(net.count_links(LinkKind::Pump), 1);
        assert!(net.nodes.iter().all(|n| n.pos.is_some()));
        assert_eq!(net.flow_units(), "GPM");
    }

    #[test]
    fn rename_and_delete_update_controls_rules_trace_and_report() {
        let mut net = Network::from_inp("[JUNCTIONS]\nA 0\nB 0\n[RESERVOIRS]\nR 100\n[PIPES]\nP R A 100 12 100\nQ R B 100 12 100\n[CONTROLS]\nLINK P CLOSED IF NODE A ABOVE 20\n[RULES]\nRULE affected\nIF NODE A PRESSURE ABOVE 20\nTHEN LINK P STATUS IS CLOSED\nRULE retained\nIF SYSTEM TIME ABOVE 2\nTHEN LINK Q STATUS IS OPEN\n[OPTIONS]\nQUALITY TRACE A\n[REPORT]\nNODES A B\nLINKS P Q\n[END]\n").unwrap();
        assert!(net.rename_node(net.node_index("A").unwrap(), "AA"));
        assert!(net.rename_link(net.link_index("P").unwrap(), "PP"));
        let text = net.to_inp();
        assert!(text.contains("LINK\tPP\tCLOSED\tIF\tNODE\tAA"));
        assert!(text.contains("IF\tNODE\tAA\tPRESSURE"));
        assert!(text.contains("QUALITY\tTRACE\tAA"));
        assert!(text.contains("NODES\tAA\tB"));
        net.delete_node(net.node_index("AA").unwrap());
        let text = net.to_inp();
        assert!(!text.contains("RULE affected"));
        assert!(text.contains("RULE retained"));
        assert!(!text.contains("QUALITY\tTRACE"));
        assert!(text.contains("NODES\tB"));
        assert!(text.contains("LINKS\tQ"));
    }

    #[test]
    fn roundtrip_is_stable() {
        for (_, text) in examples::ALL {
            let a = Network::from_inp(text).unwrap();
            let b = Network::from_inp(&a.to_inp()).unwrap();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn comments_before_the_first_section_stay_on_top() {
        let text = format!(";MADE BY EPANET-JS\n{}", examples::NET1);
        let out = Network::from_inp(&text).unwrap().to_inp();
        assert!(out.starts_with(";MADE BY EPANET-JS\n"), "{}", &out[..60]);
        assert!(!out.contains("[]"));
    }

    #[test]
    fn data_tables_follow_the_inp_sections() {
        let net = Network::from_inp(examples::NET1).unwrap();
        let tables = net.data_tables();
        let title = tables.iter().find(|t| t.name == "TITLE").unwrap();
        assert_eq!(title.rows[0][0], "EPANET Example Network 1");
        let j = tables.iter().find(|t| t.name == "JUNCTIONS").unwrap();
        assert_eq!(j.columns[0], "ID");
        assert_eq!(j.rows.len(), 9);
        assert_eq!(j.rows[0][0], "10");
        assert!(tables.iter().all(|t| t.name != "END"));
    }

    #[test]
    fn delete_and_rename() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let i = net.node_index("10").unwrap();
        let links_before = net.links.len();
        net.delete_node(i);
        assert!(net.links.len() < links_before);
        assert!(net.links.iter().all(|l| l.from != "10" && l.to != "10"));

        let j = net.node_index("22").unwrap();
        assert!(net.rename_node(j, "N22"));
        assert!(net.links.iter().any(|l| l.from == "N22" || l.to == "N22"));
        assert!(net.to_inp().contains("N22"));
        assert!(!net.rename_node(j, "23"), "duplicate ID must be rejected");
    }

    #[test]
    fn node_and_link_with_same_id_keep_separate_references() {
        use crate::props::{Element, PropKey};
        let mut net = Network::from_inp(examples::NET1).unwrap();
        let n = Element::Node(net.node_index("12").unwrap());
        let l = Element::Link(net.link_index("12").unwrap());
        net.set_prop(n, PropKey::Tag, "node-tag").unwrap();
        net.set_prop(l, PropKey::Tag, "link-tag").unwrap();
        net.delete_link(net.link_index("12").unwrap());
        let n = Element::Node(net.node_index("12").unwrap());
        assert_eq!(net.get_prop(n, PropKey::Tag), "node-tag");
        assert!(!net.to_inp().contains("link-tag"));
    }

    #[test]
    fn short_keyword_does_not_match_longer_one() {
        let mut net = Network::new();
        net.set_option("OPTIONS", "PRESSURE EXPONENT", "0.5");
        assert_eq!(net.option("OPTIONS", "PRESSURE"), None);
        net.set_option("OPTIONS", "PRESSURE", "METERS");
        assert_eq!(
            net.option("OPTIONS", "PRESSURE EXPONENT").as_deref(),
            Some("0.5")
        );
        assert_eq!(net.option("OPTIONS", "PRESSURE").as_deref(), Some("METERS"));
        net.set_option("OPTIONS", "PRESSURE", "");
        assert_eq!(
            net.option("OPTIONS", "PRESSURE EXPONENT").as_deref(),
            Some("0.5")
        );
    }

    #[test]
    fn options() {
        let mut net = Network::from_inp(examples::NET1).unwrap();
        assert_eq!(net.option("TIMES", "DURATION").as_deref(), Some("24:00"));
        net.set_option("TIMES", "DURATION", "48:00");
        assert_eq!(net.option("TIMES", "DURATION").as_deref(), Some("48:00"));
        assert_eq!(
            net.option("OPTIONS", "QUALITY").as_deref(),
            Some("Chlorine mg/L")
        );
    }
}
