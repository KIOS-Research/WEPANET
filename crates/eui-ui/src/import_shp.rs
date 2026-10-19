//! The Shapefile Importer (EPANET-UI's `shpimporter`): network links from a
//! polyline shapefile and, optionally, nodes from a point shapefile, with
//! their attributes mapped to EPANET properties.
//!
//! A shapefile is several files (.shp, .dbf, .prj); they can be chosen
//! together or as one .zip archive.

use egui::{RichText, Ui};
use eui_core::{LinkKind, NodeKind};

use crate::app::{Dialog, EpanetApp};
use crate::i18n::{tr, trf};
use crate::import::{BuildOptions, InLink, InNode, MapUnits};
use crate::io::Inbox;
use crate::shapefile::{Dbf, Shape, read_dbf, read_prj, read_shp};
use crate::wizard::{self, FileAction, Mapping, Nav, ROW_H};

/// The link attributes to map, as EPANET-UI's (ID … Roughness).
const LINK_PROPS: [&str; 9] = [
    "Link ID",
    "Link Type",
    "Start Node",
    "End Node",
    "Description",
    "Tag",
    "Length",
    "Diameter",
    "Roughness",
];
const NODE_PROPS: [&str; 6] = [
    "Node ID",
    "Node Type",
    "Description",
    "Tag",
    "Elevation",
    "Base Demand",
];
const FLOW_UNITS: [&str; 11] = [
    "CFS", "GPM", "MGD", "IMGD", "AFD", "LPS", "LPM", "MLD", "CMH", "CMD", "CMS",
];

/// Unit choices of an attribute ("" = as in the project).
pub(crate) fn unit_choices(nodes: bool, k: usize) -> &'static [&'static str] {
    match (nodes, k) {
        (false, 6) | (true, 4) => &["", "FEET", "METERS"],
        (false, 7) | (false, 8) => &["", "INCHES", "MILLIMETERS"],
        (true, 5) => &[
            "", "CFS", "GPM", "MGD", "IMGD", "AFD", "LPS", "LPM", "MLD", "CMH", "CMD", "CMS",
        ],
        _ => &[],
    }
}

/// Field names that suggest each property, for a first guess.
fn guess(nodes: bool, k: usize, field: &str) -> bool {
    let f = field.to_ascii_uppercase();
    let any = |names: &[&str]| names.iter().any(|n| f == *n || f.starts_with(n));
    match (nodes, k) {
        (_, 0) => any(&["ID", "NAME", "DC_ID", "LABEL"]),
        (_, 1) => any(&["TYPE"]),
        (false, 2) => any(&["FROM", "START", "NODE1", "NODE_1", "N1"]),
        (false, 3) => any(&["TO", "END", "NODE2", "NODE_2", "N2"]),
        (false, 4) | (true, 2) => any(&["DESC", "COMMENT"]),
        (false, 5) | (true, 3) => any(&["TAG"]),
        (false, 6) => any(&["LEN", "SHAPE_LEN"]),
        (false, 7) => any(&["DIAM", "DN", "DIA"]),
        (false, 8) => any(&["ROUGH", "C_FACTOR", "CFACTOR", "HW_C", "COEF"]),
        (true, 4) => any(&["ELEV", "Z", "ALT", "HEIGHT"]),
        (true, 5) => any(&["DEMAND", "BASEDEM"]),
        _ => false,
    }
}

/// One shapefile: its shapes, attribute table and .prj text.
#[derive(Clone, Debug, Default)]
pub struct Layer {
    pub name: String,
    pub shapes: Vec<Shape>,
    pub dbf: Option<Dbf>,
    pub prj: Option<String>,
}

pub struct ShpImport {
    tab: usize,
    links: Option<Layer>,
    nodes: Option<Layer>,
    link_map: [Option<usize>; 9],
    link_units: [usize; 9],
    node_map: [Option<usize>; 6],
    node_units: [usize; 6],
    epsg: String,
    units: usize,
    snap: String,
    snap_feet: bool,
    compute_lengths: bool,
    table: Option<bool>,
    error: Option<String>,
    link_inbox: Inbox,
    node_inbox: Inbox,
}

impl Default for ShpImport {
    fn default() -> Self {
        ShpImport {
            tab: 0,
            links: None,
            nodes: None,
            link_map: [None; 9],
            link_units: [0; 9],
            node_map: [None; 6],
            node_units: [0; 6],
            epsg: String::new(),
            units: 2,
            snap: "0.00".into(),
            snap_feet: false,
            compute_lengths: false,
            table: None,
            error: None,
            link_inbox: Inbox::default(),
            node_inbox: Inbox::default(),
        }
    }
}

/// Opens the Shapefile Importer.
pub fn open(app: &mut EpanetApp) {
    // the project's map units, as EPANET-UI
    let units = match app
        .net
        .option("BACKDROP", "UNITS")
        .map(|u| u.to_ascii_uppercase())
        .as_deref()
    {
        Some("FEET") => 1,
        Some("METERS") => 2,
        Some("DEGREES") => 3,
        _ => 0,
    };
    let epsg = app
        .project_crs()
        .and_then(|c| c.strip_prefix("EPSG:").map(str::to_string))
        .unwrap_or_default();
    app.shp_import = Some(ShpImport {
        units,
        epsg,
        ..Default::default()
    });
    app.dialog = Dialog::ImportShapefile;
}

/// Gathers a shapefile from the chosen files: .shp, .dbf and .prj of the same
/// name, or the same inside a .zip.
fn assemble(files: Vec<(String, Vec<u8>)>) -> Result<Layer, String> {
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, bytes) in files {
        if name.to_ascii_lowercase().ends_with(".zip") {
            let mut z =
                zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
            for k in 0..z.len() {
                let mut f = z.by_index(k).map_err(|e| e.to_string())?;
                let full = f.name().map_err(|e| e.to_string())?.to_string();
                let inner = full.rsplit('/').next().unwrap_or("").to_string();
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut f, &mut buf).map_err(|e| e.to_string())?;
                parts.push((inner, buf));
            }
        } else {
            parts.push((name, bytes));
        }
    }
    let ext = |n: &str| n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let stem = |n: &str| n.rsplit_once('.').map_or(n, |s| s.0).to_string();
    let (shp_name, shp) = parts
        .iter()
        .find(|(n, _)| ext(n) == "shp")
        .ok_or("choose the .shp file (with its .dbf and .prj), or a .zip")?;
    let base = stem(shp_name);
    let sibling = |e: &str| {
        parts
            .iter()
            .find(|(n, _)| ext(n) == e && stem(n) == base)
            .or_else(|| parts.iter().find(|(n, _)| ext(n) == e))
    };
    Ok(Layer {
        name: shp_name.clone(),
        shapes: read_shp(shp)?,
        dbf: sibling("dbf").map(|(_, b)| read_dbf(b)).transpose()?,
        prj: sibling("prj").map(|(_, b)| crate::io::decode_text(b)),
    })
}

/// Reads the CRS and units from a layer's .prj (EPANET-UI's "Read CRS code
/// and units from .prj file").
fn use_prj(s: &mut ShpImport, prj: &str) {
    let (epsg, units) = read_prj(prj);
    if let Some(e) = epsg {
        s.epsg = e.to_string();
    }
    if let Some(u) = units {
        s.units = MapUnits::ALL
            .iter()
            .position(|m| m.label() == u)
            .unwrap_or(s.units);
    }
}

fn take_files(inbox: &Inbox) -> Option<Result<Layer, String>> {
    let items = inbox.drain();
    if items.is_empty() {
        return None;
    }
    let files: Result<Vec<_>, String> = items.into_iter().collect();
    Some(files.and_then(assemble))
}

const LINKS_TEXT: &str = "Select a shapefile containing georeferenced link data and assign its \
attributes to their corresponding EPANET link properties. Properties with no assigned attribute \
will have default values. Skip this page if no link data will be imported.";
const NODES_TEXT: &str = "Select a shapefile containing georeferenced node data and assign its \
attributes to their corresponding EPANET node properties. Properties with no assigned attribute \
will have default values. Skip this page if no node data will be imported.";
const PREVIEW_TEXT: &str = "A preview of the imported network layout is displayed below. Click \
the Import button to begin importing data from the selected shapefiles into the project.";

/// The Links or Nodes page.
fn layer_page(ui: &mut Ui, s: &mut ShpImport, nodes: bool) {
    let ctx = ui.ctx().clone();
    wizard::note(ui, if nodes { NODES_TEXT } else { LINKS_TEXT });
    ui.add_space(14.0);
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(14, 0))
        .show(ui, |ui| {
            let layer = if nodes { &s.nodes } else { &s.links };
            let name = layer.as_ref().map(|l| l.name.clone()).unwrap_or_default();
            match wizard::file_row(
                ui,
                if nodes { "Nodes File:" } else { "Links File:" },
                &name,
                "Select the .shp file (with its .dbf and .prj), or a .zip",
            ) {
                FileAction::Open => {
                    let inbox = if nodes { &s.node_inbox } else { &s.link_inbox };
                    crate::io::open_files(inbox, &ctx, "Shapefile", &["shp", "dbf", "prj", "zip"]);
                }
                FileAction::Clear => {
                    if nodes {
                        s.nodes = None;
                        s.node_map = [None; 6];
                    } else {
                        s.links = None;
                        s.link_map = [None; 9];
                    }
                    s.table = None;
                }
                FileAction::None => {}
            }
            ui.add_space(4.0);
            let layer = if nodes {
                s.nodes.as_ref()
            } else {
                s.links.as_ref()
            };
            let has_dbf = layer.is_some_and(|l| l.dbf.is_some());
            let showing = s.table == Some(nodes) && has_dbf;
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() - 160.0);
                if has_dbf
                    && wizard::link_label(
                        ui,
                        if showing {
                            "View Properties"
                        } else {
                            "View Attribute Table"
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
                if let Some(dbf) = layer.and_then(|l| l.dbf.as_ref()) {
                    wizard::records(ui, "shp_table", &dbf.fields, &dbf.records, height);
                }
            } else if nodes {
                let fields = s
                    .nodes
                    .as_ref()
                    .and_then(|l| l.dbf.as_ref())
                    .map(|d| &d.fields[..]);
                wizard::mapping(
                    ui,
                    Mapping {
                        id: "shp_nodes",
                        title: "Node Property",
                        column: "Shapefile Attribute",
                        props: &NODE_PROPS,
                        fields,
                        map: &mut s.node_map,
                        units: &mut s.node_units,
                        choices: &|k| unit_choices(true, k),
                        height: ROW_H * 9.0,
                    },
                );
            } else {
                let fields = s
                    .links
                    .as_ref()
                    .and_then(|l| l.dbf.as_ref())
                    .map(|d| &d.fields[..]);
                wizard::mapping(
                    ui,
                    Mapping {
                        id: "shp_links",
                        title: "Link Property",
                        column: "Shapefile Attribute",
                        props: &LINK_PROPS,
                        fields,
                        map: &mut s.link_map,
                        units: &mut s.link_units,
                        choices: &|k| unit_choices(false, k),
                        height: ROW_H * 12.5,
                    },
                );
            }
        });
}

/// The Options page.
fn options_page(ui: &mut Ui, s: &mut ShpImport) {
    ui.add_space(56.0);
    ui.spacing_mut().item_spacing.y = 14.0;
    wizard::option_row(ui, "", 0.0, |ui| {
        ui.label(
            RichText::new(tr("Provide values for the import options listed below:")).size(14.5),
        );
    });
    ui.add_space(4.0);
    wizard::option_row(ui, "EPSG CRS Code\n(if known)", 40.0, |ui| {
        ui.add(egui::TextEdit::singleline(&mut s.epsg).desired_width(150.0));
    });
    wizard::option_row(ui, "Coordinate Units", 30.0, |ui| {
        egui::ComboBox::from_id_salt("shp_units")
            .selected_text(tr(MapUnits::ALL[s.units].label()))
            .width(138.0)
            .show_ui(ui, |ui| {
                for (k, u) in MapUnits::ALL.iter().enumerate() {
                    ui.selectable_value(&mut s.units, k, tr(u.label()));
                }
            });
    });
    let prj = s
        .links
        .as_ref()
        .and_then(|l| l.prj.clone())
        .or_else(|| s.nodes.as_ref().and_then(|l| l.prj.clone()));
    wizard::option_row(ui, "", 0.0, |ui| {
        if wizard::link_label(ui, "Read CRS code and units from .prj file", true) {
            match &prj {
                Some(p) => {
                    use_prj(s, p);
                    s.error = None;
                }
                None => {
                    s.error = Some(
                        "No projection data were found: select the shapefile together with its .prj file.".into(),
                    )
                }
            }
        }
    });
    wizard::option_row(ui, "Node Snap Tolerance", 30.0, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut s.snap)
                .desired_width(150.0)
                .horizontal_align(egui::Align::Max),
        );
    });
    wizard::option_row(ui, "", 0.0, |ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        crate::widgets::radio_value(ui, &mut s.snap_feet, true, "Feet");
        crate::widgets::radio_value(ui, &mut s.snap_feet, false, "Meters");
    });
    wizard::option_row(ui, "", 0.0, |ui| {
        ui.label(RichText::new(tr("Nodes within snap tolerance will be merged.")).size(14.5));
    });
    wizard::option_row(ui, "", 0.0, wizard::snap_picture);
    wizard::option_row(ui, "", 0.0, |ui| {
        crate::widgets::checkbox(ui, &mut s.compute_lengths, "Recompute Pipe Lengths");
    });
}

/// The Shapefile Importer window.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let Some(s) = app.shp_import.as_mut() else {
        return true;
    };
    // files arriving from the pickers
    for nodes in [false, true] {
        let inbox = if nodes { &s.node_inbox } else { &s.link_inbox };
        match take_files(inbox) {
            Some(Ok(layer)) => {
                if s.epsg.is_empty()
                    && let Some(prj) = layer.prj.clone()
                {
                    use_prj(s, &prj);
                }
                let fields = layer
                    .dbf
                    .as_ref()
                    .map(|d| d.fields.clone())
                    .unwrap_or_default();
                if nodes {
                    for k in 0..NODE_PROPS.len() {
                        s.node_map[k] = fields.iter().position(|f| guess(true, k, f));
                    }
                    s.nodes = Some(layer);
                } else {
                    for k in 0..LINK_PROPS.len() {
                        s.link_map[k] = fields.iter().position(|f| guess(false, k, f));
                    }
                    s.links = Some(layer);
                }
                s.error = None;
            }
            Some(Err(e)) => s.error = Some(e),
            None => {}
        }
    }
    ui.add_space(8.0);
    let names: &[&str] = if s.tab == 3 {
        &["Links", "Nodes", "Options", "Preview"]
    } else {
        &["Links", "Nodes", "Options"]
    };
    let before = s.tab;
    wizard::tabs(ui, &mut s.tab, names);
    wizard::page(ui, |ui| match s.tab {
        0 | 1 => layer_page(ui, s, s.tab == 1),
        2 => options_page(ui, s),
        _ => {
            wizard::note(ui, PREVIEW_TEXT);
            ui.add_space(12.0);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(14, 0))
                .show(ui, |ui| {
                    let h = ui.available_height() - 14.0;
                    let mut lines = Vec::new();
                    let mut points = Vec::new();
                    for l in [s.links.as_ref(), s.nodes.as_ref()].into_iter().flatten() {
                        for sh in &l.shapes {
                            match sh {
                                Shape::Line(v) => lines.push(&v[..]),
                                Shape::Point(p) => points.push(*p),
                                Shape::Null => {}
                            }
                        }
                    }
                    wizard::preview(ui, &lines, &points, "No shapefile was selected", h);
                });
        }
    });
    wizard::error(ui, &s.error);
    ui.add_space(20.0);
    let can_import = s.links.is_some() || s.nodes.is_some();
    let nav = wizard::buttons(ui, &mut s.tab, 3, can_import);
    if s.tab != before {
        s.table = None;
    }
    if nav == Nav::Import {
        match run(app) {
            Ok(msg) => {
                app.status = msg.clone();
                app.plugins.message = msg;
                app.shp_import = None;
                app.dialog = Dialog::PluginMessage;
                return false;
            }
            Err(e) => {
                if let Some(s) = app.shp_import.as_mut() {
                    s.error = Some(e);
                }
            }
        }
    }
    let close = nav == Nav::Cancel;
    if close {
        app.shp_import = None;
    }
    close
}

/// Scale factor of an attribute's units to the project's.
pub(crate) fn link_ucf(k: usize, unit: &str, si: bool, dw: bool) -> f64 {
    match (k, unit, si) {
        (6, "METERS", false) => 1.0 / 0.3048,
        (6, "FEET", true) => 0.3048,
        (7, "MILLIMETERS", false) => 1.0 / 25.4,
        (7, "INCHES", true) => 25.4,
        // D-W roughness is in millifeet (US) or millimeters (SI)
        (8, "MILLIMETERS", false) if dw => 1.0 / 0.3048,
        (8, "INCHES", true) if dw => 25.4,
        (8, "INCHES", false) if dw => 1000.0 / 12.0,
        _ => 1.0,
    }
}

pub(crate) fn node_ucf(k: usize, unit: &str, si: bool, project_flow: &str) -> f64 {
    match (k, unit) {
        (4, "METERS") if !si => 1.0 / 0.3048,
        (4, "FEET") if si => 0.3048,
        (5, u) if !u.is_empty() => {
            let i = FLOW_UNITS.iter().position(|f| *f == u);
            let j = FLOW_UNITS
                .iter()
                .position(|f| f.eq_ignore_ascii_case(project_flow));
            match (i, j) {
                (Some(i), Some(j)) => {
                    eui_core::analysis::FLOW_UCF[j] / eui_core::analysis::FLOW_UCF[i]
                }
                _ => 1.0,
            }
        }
        _ => 1.0,
    }
}

/// Imports the chosen layers into the project.
fn run(app: &mut EpanetApp) -> Result<String, String> {
    let s = app.shp_import.as_ref().ok_or("nothing to import")?;
    let snap: f64 = s
        .snap
        .trim()
        .parse()
        .map_err(|_| "Enter a number for the snap tolerance")?;
    let snap_m = if s.snap_feet { snap * 0.3048 } else { snap };
    let src_units = MapUnits::ALL[s.units];
    let src_epsg: Option<u32> = s.epsg.trim().parse().ok().filter(|e| *e > 0);
    let si = app.net.is_si();
    let dw = app
        .net
        .option("OPTIONS", "HEADLOSS")
        .is_some_and(|h| h.eq_ignore_ascii_case("D-W"));
    let flow = app.net.flow_units();

    // Into an empty project the coordinates come as they are and set its CRS;
    // into one with a known CRS they are reprojected when they differ.
    let empty = app.net.nodes.is_empty();
    let mut reproject: Option<(crate::basemap::Geo, crate::basemap::Geo)> = None;
    let mut units = src_units;
    if !empty
        && let Some(code) = src_epsg
        && let Some(dst) = app.project_crs()
        && dst != format!("EPSG:{code}")
    {
        let src = crate::basemap::Geo::parse(&format!("EPSG:{code}"))?;
        let dstg = crate::basemap::Geo::parse(&dst)?;
        reproject = Some((src, dstg));
        units = match app
            .net
            .option("BACKDROP", "UNITS")
            .map(|u| u.to_ascii_uppercase())
            .as_deref()
        {
            Some("DEGREES") => MapUnits::Degrees,
            Some("FEET") => MapUnits::Feet,
            Some("METERS") => MapUnits::Meters,
            _ if dst == "EPSG:4326" => MapUnits::Degrees,
            _ => MapUnits::Meters,
        };
    }
    let xy = |p: [f64; 2]| -> [f64; 2] {
        match &reproject {
            Some((a, b)) => a.to_lonlat(p).and_then(|ll| b.lonlat_to(ll)).unwrap_or(p),
            None => p,
        }
    };

    let mut nodes = Vec::new();
    if let Some(l) = &s.nodes {
        for (r, sh) in l.shapes.iter().enumerate() {
            let Shape::Point(p) = sh else { continue };
            let d = l.dbf.as_ref();
            let field = |k: usize| d.and_then(|d| d.text(r, s.node_map[k])).map(str::to_string);
            let num = |k: usize| {
                let u = unit_choices(true, k)
                    .get(s.node_units[k])
                    .copied()
                    .unwrap_or("");
                d.and_then(|d| d.number(r, s.node_map[k]))
                    .map(|v| v * node_ucf(k, u, si, &flow))
            };
            let kind = field(1).map(|t| {
                let t = t.to_ascii_uppercase();
                if t.starts_with("RES") || t == "1" {
                    NodeKind::Reservoir
                } else if t.starts_with("TANK") || t == "2" {
                    NodeKind::Tank
                } else {
                    NodeKind::Junction
                }
            });
            nodes.push(InNode {
                id: field(0),
                kind,
                xy: xy(*p),
                elevation: num(4),
                demand: num(5),
                description: field(2),
                tag: field(3),
            });
        }
    }
    let mut links = Vec::new();
    if let Some(l) = &s.links {
        for (r, sh) in l.shapes.iter().enumerate() {
            let Shape::Line(pts) = sh else { continue };
            let d = l.dbf.as_ref();
            let field = |k: usize| d.and_then(|d| d.text(r, s.link_map[k])).map(str::to_string);
            let num = |k: usize| {
                let u = unit_choices(false, k)
                    .get(s.link_units[k])
                    .copied()
                    .unwrap_or("");
                d.and_then(|d| d.number(r, s.link_map[k]))
                    .map(|v| v * link_ucf(k, u, si, dw))
            };
            let kind = field(1).map(|t| {
                let t = t.to_ascii_uppercase();
                if t.starts_with("PUMP") || t == "2" {
                    LinkKind::Pump
                } else if t.starts_with("VALVE") || t == "3" {
                    LinkKind::Valve
                } else {
                    LinkKind::Pipe
                }
            });
            links.push(InLink {
                id: field(0),
                kind,
                points: pts.iter().map(|&p| xy(p)).collect(),
                start: field(2),
                end: field(3),
                length: num(6),
                diameter: num(7),
                roughness: num(8),
                description: field(4),
                tag: field(5),
            });
        }
    }
    if nodes.is_empty() && links.is_empty() {
        return Err("The shapefiles have no points or lines to import".into());
    }
    let opt = BuildOptions {
        units,
        snap_m,
        compute_lengths: s.compute_lengths || app.prefs.auto_length,
    };
    app.checkpoint();
    let setup = app.setup.clone();
    let sum = crate::import::build(&mut app.net, &setup, &nodes, &links, &opt);
    if empty {
        if let Some(code) = src_epsg {
            app.set_project_crs(format!("EPSG:{code}"));
        }
        app.net
            .set_option("BACKDROP", "UNITS", &src_units.label().to_ascii_uppercase());
    }
    app.results = None;
    app.view.fit_pending = true;
    let mut msg = trf(
        "Imported {} nodes and {} links",
        &[&sum.nodes_added, &sum.links_added],
    );
    if sum.nodes_updated + sum.links_updated > 0 {
        msg += &trf(
            " (updated {} existing objects)",
            &[&(sum.nodes_updated + sum.links_updated)],
        );
    }
    if sum.skipped > 0 {
        msg += &trf("; skipped {} shapes", &[&sum.skipped]);
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapefile::tests::{write_dbf, write_shp};

    #[test]
    fn assembles_a_zipped_shapefile() {
        let shp = write_shp(&[Shape::Line(vec![[0.0, 0.0], [10.0, 0.0]])]);
        let dbf = write_dbf(&[("PIPE_ID", 'C')], &[vec!["P1"]]);
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (n, b) in [("pipes.shp", &shp), ("pipes.dbf", &dbf)] {
                z.start_file(n, o).unwrap();
                std::io::Write::write_all(&mut z, b).unwrap();
            }
            z.finish().unwrap();
        }
        let l = assemble(vec![("network.zip".into(), buf.into_inner())]).unwrap();
        assert_eq!(l.shapes.len(), 1);
        assert_eq!(l.dbf.unwrap().fields, ["PIPE_ID"]);
        assert!(guess(false, 7, "DIAMETER") && guess(true, 4, "ELEVATION"));
    }

    #[test]
    fn imports_pipes_and_nodes_into_a_new_project() {
        let mut app = crate::app::EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        let links = Layer {
            name: "pipes.shp".into(),
            shapes: vec![
                Shape::Line(vec![[0.0, 0.0], [100.0, 0.0]]),
                Shape::Line(vec![[100.2, 0.0], [100.0, 80.0], [200.0, 80.0]]),
            ],
            dbf: Some(
                read_dbf(&write_dbf(
                    &[("ID", 'C'), ("DIAMETER", 'N')],
                    &[vec!["P1", "150"], vec!["P2", "200"]],
                ))
                .unwrap(),
            ),
            prj: None,
        };
        let nodes = Layer {
            name: "nodes.shp".into(),
            shapes: vec![Shape::Point([0.0, 0.0])],
            dbf: Some(
                read_dbf(&write_dbf(
                    &[("ID", 'C'), ("TYPE", 'C'), ("ELEV", 'N')],
                    &[vec!["R1", "RESERVOIR", "50"]],
                ))
                .unwrap(),
            ),
            prj: None,
        };
        let mut s = ShpImport {
            snap: "0.5".into(),
            epsg: "2100".into(),
            ..Default::default()
        };
        s.link_map = [Some(0), None, None, None, None, None, None, Some(1), None];
        s.node_map = [Some(0), Some(1), None, None, Some(2), None];
        s.links = Some(links);
        s.nodes = Some(nodes);
        app.shp_import = Some(s);
        let msg = run(&mut app).unwrap();
        assert_eq!(msg, "Imported 3 nodes and 2 links");
        let net = &app.net;
        let r = net.node_index("R1").unwrap();
        assert_eq!(net.nodes[r].kind, NodeKind::Reservoir);
        assert_eq!(net.links[net.link_index("P1").unwrap()].from, "R1");
        // P2 starts within the snap tolerance of P1's end, bends once
        let p2 = &net.links[net.link_index("P2").unwrap()];
        assert_eq!(p2.from, net.links[net.link_index("P1").unwrap()].to);
        assert_eq!(p2.vertices, vec![[100.0, 80.0]]);
        assert_eq!(p2.field(1), "200");
        assert_eq!(
            app.prefs
                .basemap_crs
                .get(&app.file_name)
                .map(String::as_str),
            Some("EPSG:2100")
        );
    }

    #[test]
    fn unit_factors() {
        assert!((link_ucf(7, "MILLIMETERS", false, false) - 1.0 / 25.4).abs() < 1e-12);
        assert_eq!(link_ucf(7, "INCHES", true, false), 25.4);
        assert!((node_ucf(5, "LPS", false, "GPM") - 15.850).abs() < 0.01);
    }
}
