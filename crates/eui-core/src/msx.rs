//! EPANET-MSX, the multi-species extension of EPANET: the species named by its
//! input (`.msx`) file, and a reader for its binary output file.
//!
//! Output file layout (all little-endian 4-byte values):
//!
//! * prolog  – magic number, version, nodes, links, species, reporting step,
//!   then per species: ID length, ID, mass units (16 bytes)
//! * results – per reporting period: per species every node, then per species
//!   every link
//! * epilog  – results offset, period count, error code, magic number

use crate::results::OutError;

const MAGIC: i32 = 516_114_521;
const UNITS_LEN: usize = 16; // MAXUNITS
const EPILOG_LEN: usize = 16;

/// A chemical species of an MSX model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MsxSpecies {
    pub id: String,
    /// Mass units, e.g. `MG`, `UG`, `MOL`, `MMOL`.
    pub units: String,
    /// A wall species (attached to pipe walls), rather than a bulk one
    /// (dissolved in the water). Wall species have no values at nodes.
    pub wall: bool,
}

impl MsxSpecies {
    /// Concentration units: mass per liter for a bulk species, mass per pipe
    /// wall area (`area`, e.g. `M2`) for a wall species.
    pub fn concentration_units(&self, area: &str) -> String {
        if self.wall {
            format!("{}/{}", self.units, area)
        } else {
            format!("{}/L", self.units)
        }
    }
}

/// A reaction coefficient of an MSX model: a constant, or a parameter whose
/// value can differ per pipe and tank (`[PARAMETERS]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MsxCoefficient {
    pub id: String,
    pub value: f64,
    pub parameter: bool,
}

/// What an MSX input file declares: its title, species, coefficients,
/// patterns and the area units of wall species.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MsxModel {
    pub title: String,
    pub species: Vec<MsxSpecies>,
    pub coefficients: Vec<MsxCoefficient>,
    /// IDs of the MSX file's own time patterns (`[PATTERNS]`), for sources.
    pub patterns: Vec<String>,
    /// `FT2` (the default), `M2` or `CM2`.
    pub area_units: String,
}

/// The key EPANET-MSX knows a section by: its first four letters, e.g.
/// `COEF` for `[COEFFICIENTS]` (it accepts any abbreviation that long).
fn section_key(header: &str) -> String {
    header
        .trim()
        .trim_start_matches('[')
        .chars()
        .take(4)
        .collect::<String>()
        .to_ascii_uppercase()
}

/// A line's tokens, without its comment.
fn tokens(line: &str) -> Vec<&str> {
    line.split(';')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect()
}

impl MsxModel {
    /// Reads the declarations of an MSX input file; the reactions are left to
    /// EPANET-MSX.
    pub fn parse(text: &str) -> MsxModel {
        let mut model = MsxModel {
            area_units: "FT2".into(),
            ..MsxModel::default()
        };
        let mut section = String::new();
        for raw in text.lines() {
            let line = raw.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                section = section_key(line);
                continue;
            }
            let tok = tokens(line);
            let is = |k: &str| tok[0].eq_ignore_ascii_case(k);
            match section.as_str() {
                "TITL" if model.title.is_empty() => model.title = line.to_string(),
                "OPTI" if tok.len() >= 2 && is("AREA_UNITS") => {
                    model.area_units = tok[1].to_ascii_uppercase();
                }
                "SPEC" if tok.len() >= 3 && (is("BULK") || is("WALL")) => {
                    model.species.push(MsxSpecies {
                        id: tok[1].to_string(),
                        units: tok[2].to_ascii_uppercase(),
                        wall: is("WALL"),
                    });
                }
                "COEF" if tok.len() >= 2 && (is("CONSTANT") || is("PARAMETER")) => {
                    model.coefficients.push(MsxCoefficient {
                        id: tok[1].to_string(),
                        value: tok.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                        parameter: is("PARAMETER"),
                    });
                }
                "PATT"
                    if !model
                        .patterns
                        .iter()
                        .any(|p| p.eq_ignore_ascii_case(tok[0])) =>
                {
                    model.patterns.push(tok[0].to_string());
                }
                _ => {}
            }
        }
        model
    }

    /// The concentration units of species `s`.
    pub fn units(&self, s: usize) -> String {
        self.species
            .get(s)
            .map(|sp| sp.concentration_units(&self.area_units))
            .unwrap_or_default()
    }

    /// The parameters (coefficients that can differ per pipe and tank).
    pub fn parameters(&self) -> impl Iterator<Item = &MsxCoefficient> {
        self.coefficients.iter().filter(|c| c.parameter)
    }
}

/// A section of the MSX file as a table (EPANET-UI's Multi-Species Quality
/// Editor): its columns, the last of which takes the rest of the line when
/// `rest` is set (expressions, pattern multipliers), and the choices of the
/// first column, if it is a keyword.
pub struct MsxSection {
    /// As written in the file, e.g. `PIPES`.
    pub name: &'static str,
    /// As shown, e.g. "Pipe Reactions".
    pub title: &'static str,
    pub columns: &'static [&'static str],
    pub rest: bool,
    pub keywords: &'static [&'static str],
}

/// The sections of an MSX file other than its title, in the order EPANET-MSX
/// writes them.
pub const SECTIONS: [MsxSection; 12] = [
    MsxSection {
        name: "OPTIONS",
        title: "Options",
        columns: &["Option", "Value"],
        rest: true,
        keywords: &[
            "AREA_UNITS",
            "RATE_UNITS",
            "SOLVER",
            "COUPLING",
            "TIMESTEP",
            "ATOL",
            "RTOL",
            "SEGMENTS",
            "PECLET",
        ],
    },
    MsxSection {
        name: "SPECIES",
        title: "Species",
        columns: &["Type", "Name", "Units", "Abs. Tolerance", "Rel. Tolerance"],
        rest: false,
        keywords: &["BULK", "WALL"],
    },
    MsxSection {
        name: "COEFFICIENTS",
        title: "Coefficients",
        columns: &["Type", "Name", "Value"],
        rest: false,
        keywords: &["CONSTANT", "PARAMETER"],
    },
    MsxSection {
        name: "TERMS",
        title: "Terms",
        columns: &["Name", "Expression"],
        rest: true,
        keywords: &[],
    },
    MsxSection {
        name: "PIPES",
        title: "Pipe Reactions",
        columns: &["Type", "Species", "Expression"],
        rest: true,
        keywords: &["RATE", "EQUIL", "FORMULA"],
    },
    MsxSection {
        name: "TANKS",
        title: "Tank Reactions",
        columns: &["Type", "Species", "Expression"],
        rest: true,
        keywords: &["RATE", "EQUIL", "FORMULA"],
    },
    MsxSection {
        name: "DIFFUSIVITY",
        title: "Diffusivity",
        columns: &["Species", "Diffusivity"],
        rest: false,
        keywords: &[],
    },
    MsxSection {
        name: "QUALITY",
        title: "Initial Quality",
        columns: &["Type", "Node or Link", "Species", "Value"],
        rest: false,
        keywords: &["GLOBAL", "NODE", "LINK"],
    },
    MsxSection {
        name: "SOURCES",
        title: "Sources",
        columns: &["Type", "Node", "Species", "Strength", "Pattern"],
        rest: false,
        keywords: &SOURCE_TYPES,
    },
    MsxSection {
        name: "PARAMETERS",
        title: "Parameters",
        columns: &["Type", "Pipe or Tank", "Parameter", "Value"],
        rest: false,
        keywords: &["PIPE", "TANK"],
    },
    MsxSection {
        name: "PATTERNS",
        title: "Patterns",
        columns: &["Name", "Multipliers"],
        rest: true,
        keywords: &[],
    },
    MsxSection {
        name: "REPORT",
        title: "Report",
        columns: &["Keyword", "Values"],
        rest: true,
        keywords: &["NODES", "LINKS", "SPECIES", "FILE", "PAGESIZE"],
    },
];

/// How a source adds a species at a node (EPANET-MSX's source types).
pub const SOURCE_TYPES: [&str; 4] = ["CONCEN", "MASS", "SETPOINT", "FLOWPACED"];

/// A project's EPANET-MSX input file, kept as text (as EPANET-MSX reads it)
/// with typed access to what the Property Editor shows: initial
/// concentrations, sources and parameters of nodes and links.
#[derive(Clone, Debug, PartialEq)]
pub struct MsxFile {
    /// File name, e.g. `example.msx`.
    pub name: String,
    pub text: String,
}

/// Which tokens a line must have: (position, value, compared exactly).
/// Element IDs are compared exactly, keywords and species in any case.
type Matcher<'a> = [(usize, &'a str, bool)];

fn matches(tok: &[&str], m: &Matcher) -> bool {
    m.iter().all(|&(k, v, exact)| {
        tok.get(k).is_some_and(|t| {
            if exact {
                *t == v
            } else {
                t.eq_ignore_ascii_case(v)
            }
        })
    })
}

/// A line's comment (from `;`), to keep it when the line is rewritten.
fn comment_of(raw: &str) -> String {
    raw.find(';')
        .map(|i| format!("  {}", &raw[i..]))
        .unwrap_or_default()
}

/// What is attached to an element ID: in `section`, lines whose token `pos`
/// is the ID and, if given, whose first token is the keyword.
struct IdRule {
    section: &'static str,
    pos: usize,
    keyword: Option<&'static str>,
}

const NODE_RULES: [IdRule; 3] = [
    IdRule {
        section: "QUALITY",
        pos: 1,
        keyword: Some("NODE"),
    },
    IdRule {
        section: "SOURCES",
        pos: 1,
        keyword: None,
    },
    IdRule {
        section: "PARAMETERS",
        pos: 1,
        keyword: Some("TANK"),
    },
];

const LINK_RULES: [IdRule; 2] = [
    IdRule {
        section: "QUALITY",
        pos: 1,
        keyword: Some("LINK"),
    },
    IdRule {
        section: "PARAMETERS",
        pos: 1,
        keyword: Some("PIPE"),
    },
];

impl MsxFile {
    pub fn new(name: &str, text: &str) -> MsxFile {
        MsxFile {
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    pub fn model(&self) -> MsxModel {
        MsxModel::parse(&self.text)
    }

    /// The rows of a section as a table (see [`MsxSection`]), each with a
    /// last cell for its comment: the one on its line, or else the comment
    /// line above it (as the EPANET-MSX examples write them). `GLOBAL`
    /// initial qualities have no node or link.
    pub fn rows(&self, s: &MsxSection) -> Vec<Vec<String>> {
        let n = s.columns.len();
        let key = section_key(s.name);
        let mut current = String::new();
        let mut above = String::new();
        let mut out = Vec::new();
        for line in self.text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                current = section_key(t);
                above.clear();
                continue;
            }
            if current != key {
                continue;
            }
            let comment = t.find(';').map(|i| t[i + 1..].trim().to_string());
            let mut tok: Vec<String> = tokens(t).iter().map(|s| s.to_string()).collect();
            if tok.is_empty() {
                above = comment.unwrap_or_default();
                continue;
            }
            if s.name == "QUALITY" && tok[0].eq_ignore_ascii_case("GLOBAL") {
                tok.insert(1, String::new());
            }
            if s.rest && tok.len() > n {
                let last = tok.split_off(n - 1).join(" ");
                tok.push(last);
            }
            tok.resize(n, String::new());
            tok.push(
                comment
                    .filter(|c| !c.is_empty())
                    .unwrap_or_else(|| std::mem::take(&mut above)),
            );
            above.clear();
            out.push(tok);
        }
        out
    }

    /// Replaces the contents of a section by table rows (as [`MsxFile::rows`]
    /// gives them, the comment last; empty rows are left out).
    pub fn set_rows(&mut self, s: &MsxSection, rows: &[Vec<String>]) {
        let n = s.columns.len();
        let lines: Vec<String> = rows
            .iter()
            .filter(|r| r.iter().take(n).any(|c| !c.trim().is_empty()))
            .map(|r| {
                let data = &r[..n.min(r.len())];
                let cells: Vec<&str> = data
                    .iter()
                    .map(|c| c.trim())
                    .enumerate()
                    .filter(|&(k, c)| {
                        // a GLOBAL quality has no node or link; trailing
                        // optional cells (tolerances, pattern) may be blank
                        !(c.is_empty()
                            && (k + 1 == data.len()
                                || (s.name == "QUALITY" && k == 1)
                                || data[k + 1..].iter().all(|x| x.trim().is_empty())))
                    })
                    .map(|(_, c)| c)
                    .collect();
                let comment = r.get(n).map(|c| c.trim()).unwrap_or("");
                if comment.is_empty() {
                    format!("  {}", cells.join("  "))
                } else {
                    format!("  {}  ;{comment}", cells.join("  "))
                }
            })
            .collect();
        let key = section_key(s.name);
        let mut out: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut found = false;
        for raw in self.text.lines() {
            let t = raw.trim();
            if t.starts_with('[') {
                if current == key {
                    out.push(String::new());
                }
                current = section_key(t);
                out.push(raw.to_string());
                if current == key && !found {
                    found = true;
                    out.extend(lines.iter().cloned());
                }
                continue;
            }
            if current != key {
                out.push(raw.to_string());
            }
        }
        if !found && !lines.is_empty() {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(format!("[{}]", s.name));
            out.extend(lines);
        }
        self.text = out.join("\n") + "\n";
    }

    /// The model's title (the first line of `[TITLE]`).
    pub fn title(&self) -> String {
        self.model().title
    }

    pub fn set_title(&mut self, title: &str) {
        let title = title.trim();
        let mut out: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut done = false;
        for raw in self.text.lines() {
            let t = raw.trim();
            if t.starts_with('[') {
                current = section_key(t);
                out.push(raw.to_string());
                if current == "TITL" && !done {
                    out.push(title.to_string());
                    done = true;
                }
            } else if current != "TITL" || t.is_empty() {
                out.push(raw.to_string());
            }
        }
        if !done {
            out.insert(0, String::new());
            out.insert(0, title.to_string());
            out.insert(0, "[TITLE]".to_string());
        }
        self.text = out.join("\n") + "\n";
    }

    /// The entries of a section, as tokens (e.g. `"SOURCES"`).
    pub fn entries(&self, section: &str) -> Vec<Vec<String>> {
        let key = section_key(section);
        let mut current = String::new();
        let mut out = Vec::new();
        for line in self.text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                current = section_key(t);
            } else if current == key {
                let tok = tokens(t);
                if !tok.is_empty() {
                    out.push(tok.iter().map(|s| s.to_string()).collect());
                }
            }
        }
        out
    }

    /// The tokens of the first line of `section` that `m` matches.
    fn find(&self, section: &str, m: &Matcher) -> Option<Vec<String>> {
        let key = section_key(section);
        let mut current = String::new();
        for line in self.text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                current = section_key(t);
            } else if current == key {
                let tok = tokens(t);
                if !tok.is_empty() && matches(&tok, m) {
                    return Some(tok.iter().map(|s| s.to_string()).collect());
                }
            }
        }
        None
    }

    /// Replaces the lines of `section` that `m` matches by `line`, or removes
    /// them (None). A new line goes at the end of the section, which is added
    /// to the file if it has none.
    fn set(&mut self, section: &str, m: &Matcher, line: Option<String>) {
        let key = section_key(section);
        let mut out: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut placed = line.is_none();
        // where the section's last entry ends in `out`
        let mut section_end: Option<usize> = None;
        for raw in self.text.lines() {
            let t = raw.trim();
            if t.starts_with('[') {
                current = section_key(t);
                out.push(raw.to_string());
                if current == key {
                    section_end = Some(out.len());
                }
                continue;
            }
            if current == key {
                let tok = tokens(t);
                if !tok.is_empty() && matches(&tok, m) {
                    if !placed && let Some(l) = &line {
                        out.push(format!("  {l}{}", comment_of(raw)));
                        placed = true;
                        section_end = Some(out.len());
                    }
                    continue;
                }
                out.push(raw.to_string());
                if !t.is_empty() {
                    section_end = Some(out.len());
                }
                continue;
            }
            out.push(raw.to_string());
        }
        if !placed && let Some(l) = line {
            match section_end {
                Some(i) => out.insert(i, format!("  {l}")),
                None => {
                    if out.last().is_some_and(|l| !l.trim().is_empty()) {
                        out.push(String::new());
                    }
                    out.push(format!("[{}]", section.to_ascii_uppercase()));
                    out.push(format!("  {l}"));
                }
            }
        }
        self.text = out.join("\n") + "\n";
    }

    /// The initial concentration of species `sp` at a node (`[QUALITY]`).
    pub fn node_initial(&self, node: &str, sp: &str) -> Option<String> {
        let m = [(0, "NODE", false), (1, node, true), (2, sp, false)];
        self.find("QUALITY", &m)?.get(3).cloned()
    }

    pub fn set_node_initial(&mut self, node: &str, sp: &str, value: Option<f64>) {
        let m = [(0, "NODE", false), (1, node, true), (2, sp, false)];
        let line = value.map(|v| format!("NODE  {node}  {sp}  {v}"));
        self.set("QUALITY", &m, line);
    }

    /// The initial concentration of species `sp` in a pipe (`[QUALITY]`).
    pub fn link_initial(&self, link: &str, sp: &str) -> Option<String> {
        let m = [(0, "LINK", false), (1, link, true), (2, sp, false)];
        self.find("QUALITY", &m)?.get(3).cloned()
    }

    pub fn set_link_initial(&mut self, link: &str, sp: &str, value: Option<f64>) {
        let m = [(0, "LINK", false), (1, link, true), (2, sp, false)];
        let line = value.map(|v| format!("LINK  {link}  {sp}  {v}"));
        self.set("QUALITY", &m, line);
    }

    /// The initial concentration of species `sp` wherever a node or pipe does
    /// not give its own (`GLOBAL` in `[QUALITY]`).
    pub fn global_initial(&self, sp: &str) -> Option<String> {
        let m = [(0, "GLOBAL", false), (1, sp, false)];
        self.find("QUALITY", &m)?.get(2).cloned()
    }

    pub fn set_global_initial(&mut self, sp: &str, value: Option<f64>) {
        let m = [(0, "GLOBAL", false), (1, sp, false)];
        self.set("QUALITY", &m, value.map(|v| format!("GLOBAL  {sp}  {v}")));
    }

    /// The source of species `sp` at a node, as `TYPE strength [pattern]`.
    pub fn source(&self, node: &str, sp: &str) -> Option<String> {
        let tok = self.find("SOURCES", &[(1, node, true), (2, sp, false)])?;
        let mut s = vec![tok[0].to_ascii_uppercase()];
        s.extend(tok.iter().skip(3).cloned());
        Some(s.join(" "))
    }

    /// Sets the source of species `sp` at a node from `TYPE strength
    /// [pattern]` text; empty text removes it. The type may be left out
    /// (CONCEN) or shortened to three letters or more (e.g. `SET`).
    pub fn set_source(&mut self, node: &str, sp: &str, text: &str) -> Result<(), String> {
        let tok: Vec<&str> = text.split_whitespace().collect();
        let m = [(1, node, true), (2, sp, false)];
        if tok.is_empty() {
            self.set("SOURCES", &m, None);
            return Ok(());
        }
        let (kind, rest) = if tok[0].parse::<f64>().is_ok() {
            ("CONCEN", &tok[..])
        } else {
            let t = tok[0].to_ascii_uppercase();
            let kind = SOURCE_TYPES
                .iter()
                .find(|k| t.len() >= 3 && k.starts_with(t.as_str()))
                .ok_or_else(|| {
                    format!(
                        "\"{}\" is not a source type ({})",
                        tok[0],
                        SOURCE_TYPES.join(", ")
                    )
                })?;
            (*kind, &tok[1..])
        };
        let strength = rest
            .first()
            .ok_or("give the source strength after its type")?;
        strength
            .parse::<f64>()
            .map_err(|_| format!("\"{strength}\" is not a valid source strength"))?;
        if rest.len() > 2 {
            return Err("a source is a type, a strength and an optional pattern".into());
        }
        let mut line = format!("{kind}  {node}  {sp}  {strength}");
        if let Some(p) = rest.get(1) {
            if !self
                .model()
                .patterns
                .iter()
                .any(|q| q.eq_ignore_ascii_case(p))
            {
                return Err(format!(
                    "the MSX file has no pattern \"{p}\" (add it to its [PATTERNS])"
                ));
            }
            line.push_str(&format!("  {p}"));
        }
        self.set("SOURCES", &m, Some(line));
        Ok(())
    }

    /// The value of parameter `p` for a pipe or a tank (`[PARAMETERS]`).
    pub fn parameter(&self, tank: bool, id: &str, p: &str) -> Option<String> {
        let kind = if tank { "TANK" } else { "PIPE" };
        let m = [(0, kind, false), (1, id, true), (2, p, false)];
        self.find("PARAMETERS", &m)?.get(3).cloned()
    }

    pub fn set_parameter(&mut self, tank: bool, id: &str, p: &str, value: Option<f64>) {
        let kind = if tank { "TANK" } else { "PIPE" };
        let m = [(0, kind, false), (1, id, true), (2, p, false)];
        let line = value.map(|v| format!("{kind}  {id}  {p}  {v}"));
        self.set("PARAMETERS", &m, line);
    }

    /// Sets the value of a constant or parameter in `[COEFFICIENTS]`.
    pub fn set_coefficient(&mut self, id: &str, value: f64) {
        let Some(c) = self
            .model()
            .coefficients
            .into_iter()
            .find(|c| c.id.eq_ignore_ascii_case(id))
        else {
            return;
        };
        let kind = if c.parameter { "PARAMETER" } else { "CONSTANT" };
        let line = format!("{kind}  {}  {value}", c.id);
        self.set("COEFFICIENTS", &[(1, id, false)], Some(line));
    }

    /// Follows a renamed node or link.
    pub fn rename(&mut self, node: bool, old: &str, new: &str) {
        self.edit_ids(node, old, Some(new));
    }

    /// Forgets a deleted node or link.
    pub fn remove(&mut self, node: bool, id: &str) {
        self.edit_ids(node, id, None);
    }

    /// Renames (or removes, if `new` is None) an element in every line that
    /// names it, including the `[REPORT]` lists.
    fn edit_ids(&mut self, node: bool, old: &str, new: Option<&str>) {
        let rules: &[IdRule] = if node { &NODE_RULES } else { &LINK_RULES };
        let report = if node { "NODES" } else { "LINKS" };
        let mut current = String::new();
        let mut out: Vec<String> = Vec::new();
        for raw in self.text.lines() {
            let t = raw.trim();
            if t.starts_with('[') {
                current = section_key(t);
                out.push(raw.to_string());
                continue;
            }
            let tok = tokens(t);
            let rule = rules.iter().find(|r| {
                current == section_key(r.section)
                    && tok.get(r.pos) == Some(&old)
                    && r.keyword
                        .is_none_or(|k| tok.first().is_some_and(|f| f.eq_ignore_ascii_case(k)))
            });
            let in_report = current == "REPO"
                && tok.first().is_some_and(|f| f.eq_ignore_ascii_case(report))
                && tok[1..].contains(&old);
            if rule.is_none() && !in_report {
                out.push(raw.to_string());
                continue;
            }
            let swap = |k: usize, x: &'_ str, at: usize| -> Option<String> {
                if k == at && x == old {
                    new.map(str::to_string)
                } else {
                    Some(x.to_string())
                }
            };
            let comment = comment_of(raw);
            if let Some(r) = rule {
                if new.is_some() {
                    let toks: Vec<String> = tok
                        .iter()
                        .enumerate()
                        .filter_map(|(k, x)| swap(k, x, r.pos))
                        .collect();
                    out.push(format!("  {}{comment}", toks.join("  ")));
                }
            } else {
                let ids: Vec<String> = tok
                    .iter()
                    .enumerate()
                    .skip(1)
                    .filter_map(|(k, x)| swap(k, x, k))
                    .collect();
                if !ids.is_empty() {
                    out.push(format!("  {}  {}{comment}", tok[0], ids.join(" ")));
                }
            }
        }
        self.text = out.join("\n") + "\n";
    }
}

/// Species concentrations computed by EPANET-MSX, for every node and link at
/// each reporting period.
#[derive(Clone, Debug, Default)]
pub struct MsxResults {
    /// The species, with their types and the area units taken from the input.
    pub model: MsxModel,
    pub n_periods: usize,
    /// Seconds.
    pub report_step: u32,
    n_nodes: usize,
    n_links: usize,
    values: Vec<f32>,
    /// Per species, the smallest and largest value at any node and in any
    /// link over all periods.
    node_ranges: Vec<Option<(f32, f32)>>,
    link_ranges: Vec<Option<(f32, f32)>>,
}

impl MsxResults {
    /// Reads EPANET-MSX's binary output file; `input` is the MSX input text
    /// the results came from (for the species types and units).
    pub fn from_out(buf: &[u8], input: &str) -> Result<Self, OutError> {
        let int = |pos: usize| -> Result<i32, OutError> {
            buf.get(pos..pos + 4)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
                .ok_or(OutError::Truncated)
        };
        if buf.len() < 24 + EPILOG_LEN {
            return Err(OutError::Truncated);
        }
        if int(0)? != MAGIC {
            return Err(OutError::BadMagic);
        }
        let end = buf.len() - EPILOG_LEN;
        if int(end + 12)? != MAGIC {
            return Err(OutError::Truncated);
        }
        let n_periods = int(end + 4)?.max(0) as usize;
        if n_periods == 0 {
            return Err(OutError::NoResults);
        }
        let n_nodes = int(8)?.max(0) as usize;
        let n_links = int(12)?.max(0) as usize;
        let n_species = int(16)?.max(0) as usize;
        let report_step = int(20)?.max(0) as u32;

        let parsed = MsxModel::parse(input);
        let mut model = MsxModel {
            species: Vec::with_capacity(n_species),
            ..parsed.clone()
        };
        let mut pos = 24;
        for _ in 0..n_species {
            let len = int(pos)?.max(0) as usize;
            pos += 4;
            let text = |n: usize, at: usize| -> Result<String, OutError> {
                let raw = buf.get(at..at + n).ok_or(OutError::Truncated)?;
                let stop = raw.iter().position(|&b| b == 0).unwrap_or(n);
                Ok(String::from_utf8_lossy(&raw[..stop]).trim().to_string())
            };
            let id = text(len, pos)?;
            let units = text(UNITS_LEN, pos + len)?;
            pos += len + UNITS_LEN;
            let wall = parsed
                .species
                .iter()
                .find(|s| s.id.eq_ignore_ascii_case(&id))
                .is_some_and(|s| s.wall);
            model.species.push(MsxSpecies { id, units, wall });
        }

        let count = n_periods * n_species * (n_nodes + n_links);
        let raw = buf.get(pos..pos + count * 4).ok_or(OutError::Truncated)?;
        let values = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        let mut res = MsxResults {
            model,
            n_periods,
            report_step,
            n_nodes,
            n_links,
            values,
            node_ranges: Vec::new(),
            link_ranges: Vec::new(),
        };
        let range = |v: &mut dyn Iterator<Item = f32>| {
            v.filter(|x| x.is_finite())
                .fold(None, |r: Option<(f32, f32)>, x| {
                    Some(r.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))))
                })
        };
        let n_species = res.species().len();
        let r = &res;
        let node_ranges = (0..n_species)
            .map(|s| {
                let mut all =
                    (0..n_periods).flat_map(|p| (0..n_nodes).filter_map(move |k| r.node(p, s, k)));
                range(&mut all)
            })
            .collect();
        let link_ranges = (0..n_species)
            .map(|s| {
                let mut all =
                    (0..n_periods).flat_map(|p| (0..n_links).filter_map(move |k| r.link(p, s, k)));
                range(&mut all)
            })
            .collect();
        res.node_ranges = node_ranges;
        res.link_ranges = link_ranges;
        Ok(res)
    }

    pub fn species(&self) -> &[MsxSpecies] {
        &self.model.species
    }

    /// The index of the species named `id` (any case).
    pub fn species_index(&self, id: &str) -> Option<usize> {
        self.species()
            .iter()
            .position(|s| s.id.eq_ignore_ascii_case(id))
    }

    /// The concentration units of species `s`.
    pub fn units(&self, s: usize) -> String {
        self.model.units(s)
    }

    /// The smallest and largest value of species `s` at any node, over all
    /// periods (None for a wall species).
    pub fn node_range(&self, s: usize) -> Option<(f32, f32)> {
        self.node_ranges.get(s).copied().flatten()
    }

    /// The smallest and largest value of species `s` in any link.
    pub fn link_range(&self, s: usize) -> Option<(f32, f32)> {
        self.link_ranges.get(s).copied().flatten()
    }

    fn period_len(&self) -> usize {
        self.species().len() * (self.n_nodes + self.n_links)
    }

    /// Concentration of species `s` at node `r` (result row) at period `p`.
    /// None for a wall species, which only exists in pipes.
    pub fn node(&self, p: usize, s: usize, r: usize) -> Option<f32> {
        if p >= self.n_periods || r >= self.n_nodes || self.species().get(s)?.wall {
            return None;
        }
        self.values
            .get(p * self.period_len() + s * self.n_nodes + r)
            .copied()
    }

    /// Concentration of species `s` in link `r` (result row) at period `p`.
    pub fn link(&self, p: usize, s: usize, r: usize) -> Option<f32> {
        if p >= self.n_periods || r >= self.n_links || s >= self.species().len() {
            return None;
        }
        let nodes = self.species().len() * self.n_nodes;
        self.values
            .get(p * self.period_len() + nodes + s * self.n_links + r)
            .copied()
    }

    /// Species `s` at node `r` for every period.
    pub fn node_series(&self, r: usize, s: usize) -> Vec<f32> {
        (0..self.n_periods)
            .map_while(|p| self.node(p, s, r))
            .collect()
    }

    /// Species `s` in link `r` for every period.
    pub fn link_series(&self, r: usize, s: usize) -> Vec<f32> {
        (0..self.n_periods)
            .map_while(|p| self.link(p, s, r))
            .collect()
    }
}

/// The arsenic oxidation/adsorption example of the EPANET-MSX manual: its
/// network and its MSX input file.
pub mod examples {
    pub const INP: &str = include_str!("../../../vendor/epanet-msx/examples/example.inp");
    pub const MSX: &str = include_str!("../../../vendor/epanet-msx/examples/example.msx");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_initial_quality_sources_and_parameters() {
        let mut f = MsxFile::new("a.msx", examples::MSX);
        assert_eq!(f.node_initial("Source", "AS3").as_deref(), Some("10.0"));
        f.set_node_initial("Source", "as3", Some(12.5));
        assert_eq!(f.node_initial("Source", "AS3").as_deref(), Some("12.5"));
        f.set_node_initial("A", "NH2CL", Some(1.0));
        f.set_link_initial("5", "AS5s", Some(3.0));
        f.set_global_initial("AS5", Some(0.5));
        assert_eq!(f.global_initial("as5").as_deref(), Some("0.5"));
        f.set_source("Source", "NH2CL", "SET 2.2").unwrap();
        assert_eq!(f.source("Source", "NH2CL").as_deref(), Some("SETPOINT 2.2"));
        assert!(f.set_source("Source", "NH2CL", "CONCEN 1 NOPAT").is_err());
        assert!(f.set_source("Source", "NH2CL", "CONCEN x").is_err());
        f.set_parameter(false, "5", "K1", Some(4.0));
        assert_eq!(f.parameter(false, "5", "K1").as_deref(), Some("4"));
        // everything still parses, and the [QUALITY] comment line survives
        let m = f.model();
        assert_eq!(m.species.len(), 5);
        assert!(f.text.contains(";Initial conditions"));
        // renaming and deleting follow the network
        f.rename(true, "Source", "S0");
        assert_eq!(f.node_initial("S0", "AS3").as_deref(), Some("12.5"));
        assert_eq!(f.source("S0", "NH2CL").as_deref(), Some("SETPOINT 2.2"));
        f.rename(true, "C", "CC");
        assert!(f.text.contains("NODES  CC D"), "{}", f.text);
        f.remove(true, "S0");
        assert_eq!(f.node_initial("S0", "AS3"), None);
        assert_eq!(f.source("S0", "NH2CL"), None);
        f.remove(false, "5");
        assert_eq!(f.link_initial("5", "AS5s"), None);
        assert!(!f.text.contains("LINKS"), "{}", f.text);
        // removing values
        f.set_global_initial("AS5", None);
        assert_eq!(f.global_initial("AS5"), None);
        f.set_coefficient("Ka", 7.5);
        let ka = f
            .model()
            .coefficients
            .into_iter()
            .find(|c| c.id == "Ka")
            .unwrap();
        assert_eq!(ka.value, 7.5);
    }

    #[test]
    fn sections_as_tables() {
        let mut f = MsxFile::new("a.msx", examples::MSX);
        let pipes = SECTIONS.iter().find(|s| s.name == "PIPES").unwrap();
        let rows = f.rows(pipes);
        assert_eq!(
            rows[0],
            ["RATE", "AS3", "-Ka*AS3*NH2CL", "Arsenite oxidation"]
        );
        assert_eq!(rows.len(), 5);
        let quality = SECTIONS.iter().find(|s| s.name == "QUALITY").unwrap();
        let mut q = f.rows(quality);
        // the comment line above a row is its comment
        assert_eq!(
            q[0],
            [
                "NODE",
                "Source",
                "AS3",
                "10.0",
                "Initial conditions (= 0 if not specified here)"
            ]
        );
        q.push(vec![
            "GLOBAL".into(),
            String::new(),
            "AS5".into(),
            "0.1".into(),
            "everywhere".into(),
        ]);
        q.push(vec![String::new(); 5]);
        f.set_rows(quality, &q);
        assert_eq!(f.global_initial("AS5").as_deref(), Some("0.1"));
        assert!(
            f.text.contains("GLOBAL  AS5  0.1  ;everywhere"),
            "{}",
            f.text
        );
        // a species keeps its comment
        assert_eq!(
            f.rows(SECTIONS.iter().find(|s| s.name == "SPECIES").unwrap())[0][5],
            "Dissolved arsenite"
        );
        assert_eq!(f.rows(quality).len(), 3);
        let species = SECTIONS.iter().find(|s| s.name == "SPECIES").unwrap();
        let mut sp = f.rows(species);
        sp.push(vec![
            "BULK".into(),
            "CL2".into(),
            "MG".into(),
            String::new(),
            String::new(),
            String::new(),
        ]);
        f.set_rows(species, &sp);
        assert!(f.text.contains("  BULK  CL2  MG\n"), "{}", f.text);
        assert_eq!(f.model().species.len(), 6);
        // a section the file lacks is added
        let diff = SECTIONS.iter().find(|s| s.name == "DIFFUSIVITY").unwrap();
        f.set_rows(diff, &[vec!["CL2".into(), "1.2e-9".into(), String::new()]]);
        assert!(f.text.contains("[DIFFUSIVITY]\n  CL2  1.2e-9"));
        f.set_title("New title");
        assert_eq!(f.title(), "New title");
        assert_eq!(f.model().species.len(), 6);
    }

    #[test]
    fn reads_the_species_of_an_msx_file() {
        let m = MsxModel::parse(examples::MSX);
        assert_eq!(m.title, "Arsenic Oxidation/Adsorption Example");
        assert_eq!(m.area_units, "M2");
        let ids: Vec<&str> = m.species.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["AS3", "AS5", "AStot", "AS5s", "NH2CL"]);
        assert!(m.species[3].wall && !m.species[0].wall);
        assert_eq!(m.units(0), "UG/L");
        assert_eq!(m.units(3), "UG/M2");
        assert_eq!(m.units(4), "MG/L");
    }
}
