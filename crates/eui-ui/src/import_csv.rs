//! The CSV File Importer (EPANET-UI's `csvimporter` and `csvloader`): nodes
//! and pipes from text files whose first line names the columns.
//!
//! A node record with an ID already in the project updates that node (its
//! coordinates are optional); otherwise it needs X and Y and adds a junction.
//! A pipe record with an ID already in the project updates that pipe (its end
//! nodes are optional); otherwise it needs a start and an end node that exist.

use egui::{RichText, Ui};
use eui_core::props::{Element, PropKey};
use eui_core::{LinkKind, Network, NodeKind};

use crate::app::{Dialog, EpanetApp};
use crate::i18n::{tr, trf};
use crate::import::fmt;
use crate::import_shp::{link_ucf, node_ucf};
use crate::io::Inbox;
use crate::wizard::{self, FileAction, Mapping, Nav, ROW_H};

const NODE_PROPS: [&str; 9] = [
    "Node ID",
    "X-coordinate",
    "Y-coordinate",
    "Description",
    "Tag",
    "Elevation",
    "Base Demand",
    "Emitter Coeff.",
    "Initial Quality",
];
const PIPE_PROPS: [&str; 13] = [
    "Pipe ID",
    "Start Node",
    "End Node",
    "Description",
    "Tag",
    "Length",
    "Diameter",
    "Roughness",
    "Loss Coeff.",
    "Bulk Coeff.",
    "Wall Coeff.",
    "Leak Area",
    "Leak Expansion",
];

/// Unit choices of a property ("" = as in the project).
fn unit_choices(nodes: bool, k: usize) -> &'static [&'static str] {
    match (nodes, k) {
        (true, 5) => crate::import_shp::unit_choices(true, 4),
        (true, 6) => crate::import_shp::unit_choices(true, 5),
        (true, 7) => &["", "GPM/PSI", "LPS/M"],
        (false, 5..=7) => crate::import_shp::unit_choices(false, k + 1),
        (false, 9) | (false, 10) => &["", "1/DAYS", "1/HRS"],
        _ => &[],
    }
}

/// Column names that suggest each property, for a first guess.
fn guess(nodes: bool, k: usize, field: &str) -> bool {
    let f = field
        .trim()
        .to_ascii_uppercase()
        .replace([' ', '-', '.'], "_");
    let any = |names: &[&str]| names.iter().any(|n| f == *n || f.starts_with(n));
    match (nodes, k) {
        (_, 0) => any(&["ID", "NAME", "NODE_ID", "PIPE_ID", "LINK_ID", "LABEL"]),
        (true, 1) => any(&["X", "XCOORD", "X_COORD", "EASTING", "LON"]),
        (true, 2) => any(&["Y", "YCOORD", "Y_COORD", "NORTHING", "LAT"]),
        (false, 1) => any(&["FROM", "START", "NODE1", "NODE_1", "N1"]),
        (false, 2) => any(&["TO", "END", "NODE2", "NODE_2", "N2"]),
        (_, 3) => any(&["DESC", "COMMENT"]),
        (_, 4) => any(&["TAG"]),
        (true, 5) => any(&["ELEV", "Z", "ALT", "HEIGHT"]),
        (true, 6) => any(&["DEMAND", "BASEDEM", "BASE_DEM"]),
        (true, 7) => any(&["EMIT"]),
        (true, 8) => any(&["QUAL", "INIT_QUAL"]),
        (false, 5) => any(&["LEN"]),
        (false, 6) => any(&["DIAM", "DN", "DIA"]),
        (false, 7) => any(&["ROUGH", "C_FACTOR", "CFACTOR", "HW_C", "COEF"]),
        (false, 8) => any(&["LOSS", "MINOR", "KLOSS"]),
        (false, 9) => any(&["BULK", "KBULK"]),
        (false, 10) => any(&["WALL", "KWALL"]),
        (false, 11) => any(&["LEAK_AREA", "LEAKAREA"]),
        (false, 12) => any(&["LEAK_EXP", "LEAKEXP"]),
        _ => false,
    }
}

/// A CSV file: its column names and records.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CsvFile {
    pub name: String,
    pub fields: Vec<String>,
    pub records: Vec<Vec<String>>,
}

impl CsvFile {
    fn text(&self, r: usize, col: Option<usize>) -> Option<&str> {
        let v = self.records.get(r)?.get(col?)?.trim();
        (!v.is_empty()).then_some(v)
    }
}

/// Splits one line at the separator, with fields optionally in double quotes.
fn split(line: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => out.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// Reads a CSV file. The separator is a comma, or the semicolon or tab the
/// first line uses instead (as spreadsheets save CSV with a decimal comma).
pub fn read(name: &str, bytes: &[u8]) -> Result<CsvFile, String> {
    let text = crate::io::decode_text(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head = lines.next().ok_or("The file is empty")?;
    let sep = [',', ';', '\t']
        .into_iter()
        .max_by_key(|s| head.matches(*s).count())
        .filter(|s| head.contains(*s))
        .unwrap_or(',');
    let fields = split(head, sep);
    let records = lines.map(|l| split(l, sep)).collect();
    Ok(CsvFile {
        name: name.to_string(),
        fields,
        records,
    })
}

#[derive(Default)]
pub struct CsvImport {
    tab: usize,
    nodes: Option<CsvFile>,
    pipes: Option<CsvFile>,
    node_map: [Option<usize>; 9],
    node_units: [usize; 9],
    pipe_map: [Option<usize>; 13],
    pipe_units: [usize; 13],
    /// The records shown instead of the properties (of the nodes file?).
    table: Option<bool>,
    error: Option<String>,
    node_inbox: Inbox,
    pipe_inbox: Inbox,
}

/// Opens the CSV File Importer.
pub fn open(app: &mut EpanetApp) {
    app.csv_import = Some(CsvImport::default());
    app.dialog = Dialog::ImportCsv;
}

const NODES_TEXT: &str = "Select a CSV text file containing network node data and assign its \
column headings to their corresponding EPANET node properties. Specify units if they differ from \
the project's units. Skip this page if no node data will be imported.";
const PIPES_TEXT: &str = "Select a CSV text file containing network pipe data and assign its \
column headings to their corresponding EPANET pipe properties. Specify units if they differ from \
the project's units. Skip this page if no pipe data will be imported.";

/// The Nodes or Pipes page.
fn file_page(ui: &mut Ui, s: &mut CsvImport, nodes: bool) {
    let ctx = ui.ctx().clone();
    wizard::note(ui, if nodes { NODES_TEXT } else { PIPES_TEXT });
    ui.add_space(14.0);
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(14, 0))
        .show(ui, |ui| {
            let file = if nodes { &s.nodes } else { &s.pipes };
            let name = file.as_ref().map(|f| f.name.clone()).unwrap_or_default();
            match wizard::file_row(
                ui,
                if nodes { "Nodes File:" } else { "Pipes File:" },
                &name,
                if nodes {
                    "Select a Nodes CSV File"
                } else {
                    "Select a Pipes CSV File"
                },
            ) {
                FileAction::Open => {
                    let inbox = if nodes { &s.node_inbox } else { &s.pipe_inbox };
                    crate::io::open_file(inbox, &ctx, "CSV", &["csv", "txt"]);
                }
                FileAction::Clear => {
                    if nodes {
                        s.nodes = None;
                        s.node_map = [None; 9];
                    } else {
                        s.pipes = None;
                        s.pipe_map = [None; 13];
                    }
                    s.table = None;
                }
                FileAction::None => {}
            }
            ui.add_space(4.0);
            let file = if nodes {
                s.nodes.as_ref()
            } else {
                s.pipes.as_ref()
            };
            let showing = file.is_some() && s.table == Some(nodes);
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() - 160.0);
                if file.is_some()
                    && wizard::link_label(
                        ui,
                        if showing {
                            "View Properties"
                        } else {
                            "View CSV File"
                        },
                        true,
                    )
                {
                    s.table = if showing { None } else { Some(nodes) };
                }
            });
            ui.add_space(4.0);
            let height = (ui.available_height() - 10.0).max(ROW_H * 8.0);
            if showing {
                if let Some(f) = file {
                    wizard::records(ui, "csv_table", &f.fields, &f.records, height);
                }
                return;
            }
            let fields = file.map(|f| &f.fields[..]);
            egui::ScrollArea::vertical()
                .id_salt(("csv_props", nodes))
                .max_height(height)
                .show(ui, |ui| {
                    let m = if nodes {
                        Mapping {
                            id: "csv_nodes",
                            title: "EPANET Node Property",
                            column: "CSV File Column",
                            props: &NODE_PROPS,
                            fields,
                            map: &mut s.node_map,
                            units: &mut s.node_units,
                            choices: &|k| unit_choices(true, k),
                            height: 0.0,
                        }
                    } else {
                        Mapping {
                            id: "csv_pipes",
                            title: "EPANET Pipe Property",
                            column: "CSV File Column",
                            props: &PIPE_PROPS,
                            fields,
                            map: &mut s.pipe_map,
                            units: &mut s.pipe_units,
                            choices: &|k| unit_choices(false, k),
                            height: 0.0,
                        }
                    };
                    wizard::mapping(ui, m);
                });
        });
}

/// The assigned (property, column) pairs of one file.
fn assigned<'a>(
    file: Option<&'a CsvFile>,
    props: &[&'a str],
    map: &[Option<usize>],
) -> Vec<(&'a str, &'a str)> {
    let Some(f) = file else { return Vec::new() };
    props
        .iter()
        .zip(map)
        .filter_map(|(p, c)| Some((*p, f.fields.get((*c)?)?.as_str())))
        .collect()
}

/// The Review page: what will be read from which column.
fn review_page(ui: &mut Ui, s: &CsvImport) {
    ui.add_space(20.0);
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(40, 0))
        .show(ui, |ui| {
            ui.label(RichText::new(tr("Import Data Summary")).size(16.0).strong());
            ui.add_space(10.0);
            let nodes = assigned(s.nodes.as_ref(), &NODE_PROPS, &s.node_map);
            let pipes = assigned(s.pipes.as_ref(), &PIPE_PROPS, &s.pipe_map);
            if nodes.is_empty() && pipes.is_empty() {
                ui.label(RichText::new(tr("No data were selected for importing.")).size(14.5));
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("csv_review")
                .max_height(ui.available_height() - 10.0)
                .show(ui, |ui| {
                    egui::Grid::new("csv_review_grid")
                        .num_columns(2)
                        .spacing([40.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (title, rows) in [
                                ("EPANET Node Property", &nodes),
                                ("EPANET Pipe Property", &pipes),
                            ] {
                                if rows.is_empty() {
                                    continue;
                                }
                                ui.label(RichText::new(tr(title)).underline().size(14.5));
                                ui.label(
                                    RichText::new(tr("CSV File Column")).underline().size(14.5),
                                );
                                ui.end_row();
                                for (p, c) in rows.iter() {
                                    ui.label(RichText::new(tr(p)).size(14.5));
                                    ui.label(RichText::new(*c).size(14.5));
                                    ui.end_row();
                                }
                                ui.label("");
                                ui.end_row();
                            }
                        });
                });
        });
}

/// The CSV File Importer window.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let Some(s) = app.csv_import.as_mut() else {
        return true;
    };
    for nodes in [true, false] {
        let inbox = if nodes { &s.node_inbox } else { &s.pipe_inbox };
        let Some(item) = inbox.drain().into_iter().next() else {
            continue;
        };
        match item.and_then(|(name, bytes)| read(&name, &bytes)) {
            Ok(f) => {
                if nodes {
                    for k in 0..NODE_PROPS.len() {
                        s.node_map[k] = f.fields.iter().position(|c| guess(true, k, c));
                    }
                    s.nodes = Some(f);
                } else {
                    for k in 0..PIPE_PROPS.len() {
                        s.pipe_map[k] = f.fields.iter().position(|c| guess(false, k, c));
                    }
                    s.pipes = Some(f);
                }
                s.error = None;
            }
            Err(e) => s.error = Some(e),
        }
    }
    ui.add_space(8.0);
    let before = s.tab;
    wizard::tabs(ui, &mut s.tab, &["Nodes", "Pipes", "Review"]);
    wizard::page(ui, |ui| match s.tab {
        0 | 1 => file_page(ui, s, s.tab == 0),
        _ => review_page(ui, s),
    });
    wizard::error(ui, &s.error);
    ui.add_space(20.0);
    let can_import = !assigned(s.nodes.as_ref(), &NODE_PROPS, &s.node_map).is_empty()
        || !assigned(s.pipes.as_ref(), &PIPE_PROPS, &s.pipe_map).is_empty();
    let nav = wizard::buttons(ui, &mut s.tab, 2, can_import);
    if s.tab != before {
        s.table = None;
    }
    if nav == Nav::Import {
        match run(app) {
            Ok(msg) => {
                app.csv_import = None;
                app.finish_import(msg);
                return false;
            }
            Err(e) => {
                if let Some(s) = app.csv_import.as_mut() {
                    s.error = Some(e);
                }
            }
        }
    }
    let close = nav == Nav::Cancel;
    if close {
        app.csv_import = None;
    }
    close
}

/// What an import did.
#[derive(Default)]
struct Counts {
    added: [usize; 2],
    updated: [usize; 2],
    skipped: [usize; 2],
}

fn set(net: &mut Network, e: Element, key: PropKey, v: &str) {
    let _ = net.set_prop(e, key, v);
}

/// Loads the files' records into `net`.
fn load(
    net: &mut Network,
    s: &CsvImport,
    setup: &crate::setup::SetupDefaults,
    auto_length: bool,
) -> Counts {
    let mut c = Counts::default();
    let si = net.is_si();
    let flow = net.flow_units();
    let dw = net
        .option("OPTIONS", "HEADLOSS")
        .is_some_and(|h| h.eq_ignore_ascii_case("D-W"));
    let exponent = net
        .option("OPTIONS", "EMITTER EXPONENT")
        .and_then(|v| eui_core::num::parse(&v))
        .unwrap_or(0.5);
    if let Some(f) = &s.nodes {
        let unit = |k: usize| {
            unit_choices(true, k)
                .get(s.node_units[k])
                .copied()
                .unwrap_or("")
        };
        for r in 0..f.records.len() {
            let text = |k: usize| f.text(r, s.node_map[k]);
            let num = |k: usize| text(k).and_then(eui_core::num::parse);
            let xy = num(1).zip(num(2)).map(|(x, y)| [x, y]);
            let id = text(0);
            let i = match (id.and_then(|id| net.node_index(id)), xy) {
                (Some(i), _) => {
                    if xy.is_some() {
                        net.nodes[i].pos = xy;
                    }
                    c.updated[0] += 1;
                    i
                }
                (None, Some(xy)) => {
                    let i = setup.add_node(net, NodeKind::Junction, xy);
                    if let Some(id) = id {
                        net.rename_node(i, id);
                    }
                    c.added[0] += 1;
                    i
                }
                (None, None) => {
                    c.skipped[0] += 1;
                    continue;
                }
            };
            let e = Element::Node(i);
            let junction = net.nodes[i].kind == NodeKind::Junction;
            if let Some(v) = text(3) {
                set(net, e, PropKey::Description, v);
            }
            if let Some(v) = text(4) {
                set(net, e, PropKey::Tag, v);
            }
            if let Some(v) = num(5) {
                set(
                    net,
                    e,
                    PropKey::Field(0),
                    &fmt(v * node_ucf(4, unit(5), si, &flow)),
                );
            }
            if let Some(v) = num(6).filter(|_| junction) {
                set(
                    net,
                    e,
                    PropKey::Field(1),
                    &fmt(v * node_ucf(5, unit(6), si, &flow)),
                );
            }
            if let Some(v) = num(7).filter(|_| junction) {
                let q = match unit(7) {
                    "GPM/PSI" => {
                        node_ucf(5, "GPM", si, &flow)
                            * if si { 1.4219_f64.powf(exponent) } else { 1.0 }
                    }
                    "LPS/M" => {
                        node_ucf(5, "LPS", si, &flow)
                            / if si { 1.0 } else { 1.4219_f64.powf(exponent) }
                    }
                    _ => 1.0,
                };
                set(net, e, PropKey::Keyed("EMITTERS", 1), &fmt(v * q));
            }
            if let Some(v) = num(8) {
                set(net, e, PropKey::Keyed("QUALITY", 1), &fmt(v));
            }
        }
    }
    if let Some(f) = &s.pipes {
        let unit = |k: usize| {
            unit_choices(false, k)
                .get(s.pipe_units[k])
                .copied()
                .unwrap_or("")
        };
        for r in 0..f.records.len() {
            let text = |k: usize| f.text(r, s.pipe_map[k]);
            let num = |k: usize| text(k).and_then(eui_core::num::parse);
            let ends = text(1)
                .and_then(|a| net.node_index(a))
                .zip(text(2).and_then(|b| net.node_index(b)))
                .filter(|(a, b)| a != b);
            let id = text(0);
            let i = match (id.and_then(|id| net.link_index(id)), ends) {
                (Some(i), _) => {
                    if let Some((a, b)) = ends {
                        net.links[i].from = net.nodes[a].id.clone();
                        net.links[i].to = net.nodes[b].id.clone();
                    }
                    c.updated[1] += 1;
                    i
                }
                (None, Some((a, b))) => {
                    let i = setup.add_link(net, LinkKind::Pipe, a, b, auto_length);
                    if let Some(id) = id {
                        net.rename_link(i, id);
                    }
                    c.added[1] += 1;
                    i
                }
                (None, None) => {
                    c.skipped[1] += 1;
                    continue;
                }
            };
            let e = Element::Link(i);
            if let Some(v) = text(3) {
                set(net, e, PropKey::Description, v);
            }
            if let Some(v) = text(4) {
                set(net, e, PropKey::Tag, v);
            }
            if net.links[i].kind != LinkKind::Pipe {
                continue;
            }
            // length, diameter, roughness: positive; the other coefficients
            // as EPANET-UI checks them
            for (k, key) in [
                (5, PropKey::Field(0)),
                (6, PropKey::Field(1)),
                (7, PropKey::Field(2)),
            ] {
                if let Some(v) = num(k).filter(|v| *v > 0.0) {
                    set(net, e, key, &fmt(v * link_ucf(k + 1, unit(k), si, dw)));
                }
            }
            if let Some(v) = num(8).filter(|v| *v >= 0.0) {
                set(net, e, PropKey::Field(3), &fmt(v));
            }
            for (k, kw) in [(9, "BULK"), (10, "WALL")] {
                if let Some(v) = num(k) {
                    let per_day = if unit(k) == "1/HRS" { 24.0 } else { 1.0 };
                    set(net, e, PropKey::Reaction(kw), &fmt(v * per_day));
                }
            }
            for (k, col) in [(11, 1), (12, 2)] {
                if let Some(v) = num(k).filter(|v| *v >= 0.0) {
                    set(net, e, PropKey::Keyed("LEAKAGE", col), &fmt(v));
                }
            }
        }
    }
    c
}

/// Imports the files into the project.
fn run(app: &mut EpanetApp) -> Result<String, String> {
    let s = app.csv_import.as_ref().ok_or("nothing to import")?;
    if s.nodes.is_none() && s.pipes.is_none() {
        return Err("Select a nodes or a pipes CSV file".into());
    }
    let mut net = app.net.clone();
    let setup = app.setup.clone();
    let c = load(&mut net, s, &setup, app.prefs.auto_length);
    if c.added == [0, 0] && c.updated == [0, 0] {
        return Err(
            "No records could be imported: new nodes need X and Y coordinates, new pipes existing start and end nodes".into(),
        );
    }
    let mut msg = trf(
        "Imported {} nodes and {} pipes",
        &[&c.added[0], &c.added[1]],
    );
    if c.updated != [0, 0] {
        msg += &trf(
            " (updated {} nodes and {} pipes)",
            &[&c.updated[0], &c.updated[1]],
        );
    }
    if c.skipped != [0, 0] {
        msg += &trf("; skipped {} records", &[&(c.skipped[0] + c.skipped[1])]);
    }
    app.checkpoint();
    app.place_import(net, true, msg.clone())?;
    Ok(msg)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The importer on page `tab`, with files chosen.
    pub(crate) fn set_tab(app: &mut EpanetApp, tab: usize) {
        let s = app.csv_import.as_mut().unwrap();
        s.nodes = Some(read("nodes.csv", b"ID,X,Y,Elevation\nJ1,0,0,10\n").unwrap());
        s.pipes = Some(read("pipes.csv", b"ID,From,To,Diameter\nP1,J1,J2,150\n").unwrap());
        s.node_map[0] = Some(0);
        s.tab = tab;
    }

    #[test]
    fn reads_quoted_fields_and_semicolons() {
        let f = read(
            "n.csv",
            b"\xef\xbb\xbfID,X,\"Note\"\nJ1,1.5,\"a, \"\"b\"\"\"\n\n",
        )
        .unwrap();
        assert_eq!(f.fields, ["ID", "X", "Note"]);
        assert_eq!(f.records, [vec!["J1", "1.5", "a, \"b\""]]);
        let f = read("n.csv", "ID;X;Y\nJ1;1,5;2,25\n".as_bytes()).unwrap();
        assert_eq!(f.records[0], ["J1", "1,5", "2,25"]);
        assert!(guess(true, 1, "X") && guess(false, 1, "From Node"));
    }

    #[test]
    fn adds_and_updates_nodes_and_pipes() {
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        open(&mut app);
        let nodes = read(
            "nodes.csv",
            "ID;X;Y;Elev\nA;0;0;10,5\nB;100;0;12\nC;;;7\n".as_bytes(),
        )
        .unwrap();
        let pipes = read(
            "pipes.csv",
            b"Pipe,From,To,Diameter,Bulk\nP1,A,B,6,-0.5\nP2,A,X,8,0\n",
        )
        .unwrap();
        let s = app.csv_import.as_mut().unwrap();
        s.node_map = [
            Some(0),
            Some(1),
            Some(2),
            None,
            None,
            Some(3),
            None,
            None,
            None,
        ];
        s.pipe_map = [
            Some(0),
            Some(1),
            Some(2),
            None,
            None,
            None,
            Some(3),
            None,
            None,
            Some(4),
            None,
            None,
            None,
        ];
        // diameters in inches into the SI project; bulk coefficients per hour
        s.pipe_units[6] = 1;
        s.pipe_units[9] = 2;
        s.nodes = Some(nodes);
        s.pipes = Some(pipes);
        let msg = run(&mut app).unwrap();
        assert_eq!(msg, "Imported 2 nodes and 1 pipes; skipped 2 records");
        let net = &app.net;
        let a = net.node_index("A").unwrap();
        assert_eq!(net.nodes[a].field(0), "10.5");
        let p1 = &net.links[net.link_index("P1").unwrap()];
        assert_eq!((p1.from.as_str(), p1.to.as_str()), ("A", "B"));
        assert_eq!(p1.field(1), "152.4");
        assert_eq!(
            net.get_prop(
                Element::Link(net.link_index("P1").unwrap()),
                PropKey::Reaction("BULK")
            ),
            "-12"
        );
        assert!(app.dirty);
        // a second file updates what is there: C has no coordinates, A moves
        let nodes = read("n2.csv", b"ID,X,Y,Elev\nA,5,5,11\n").unwrap();
        let s = app.csv_import.get_or_insert_with(Default::default);
        s.nodes = Some(nodes);
        s.pipes = None;
        s.node_map = [
            Some(0),
            Some(1),
            Some(2),
            None,
            None,
            Some(3),
            None,
            None,
            None,
        ];
        let msg = run(&mut app).unwrap();
        assert_eq!(
            msg,
            "Imported 0 nodes and 0 pipes (updated 1 nodes and 0 pipes)"
        );
        assert_eq!(app.net.nodes[a].pos, Some([5.0, 5.0]));
    }
}
