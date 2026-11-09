//! The DXF File Importer (EPANET-UI's `dxfimporter`): network pipes from the
//! lines on chosen layers of a CAD drawing, with nodes where their ends meet.
//! Only the geometry is read; the pipes and nodes get the default properties.

use egui::{RichText, Sense, Ui, vec2};

use crate::app::{Dialog, EpanetApp};
use crate::dxf::Drawing;
use crate::i18n::{tr, trf};
use crate::import::{BuildOptions, InLink, MapUnits};
use crate::io::Inbox;
use crate::wizard::{self, FileAction, Nav};

pub struct DxfImport {
    tab: usize,
    name: String,
    drawing: Option<Drawing>,
    /// The number of lines on each layer of the drawing.
    counts: Vec<usize>,
    /// Counts the drawings read, to tell the preview of one from another's.
    generation: u64,
    /// The layers ticked for import.
    chosen: Vec<bool>,
    units: usize,
    snap: String,
    snap_feet: bool,
    compute_lengths: bool,
    add_to_project: bool,
    /// Add the ticked layers to the map as map layers, in one group, rather
    /// than import their lines as pipes.
    as_map_layers: bool,
    error: Option<String>,
    inbox: Inbox,
}

impl Default for DxfImport {
    fn default() -> Self {
        DxfImport {
            tab: 0,
            name: String::new(),
            drawing: None,
            counts: Vec::new(),
            generation: 0,
            chosen: Vec::new(),
            units: 0,
            snap: "0.00".into(),
            snap_feet: false,
            compute_lengths: true,
            add_to_project: true,
            as_map_layers: false,
            error: None,
            inbox: Inbox::default(),
        }
    }
}

/// Opens the DXF File Importer.
pub fn open(app: &mut EpanetApp) {
    app.dxf_import = Some(DxfImport {
        add_to_project: !app.net.nodes.is_empty(),
        ..Default::default()
    });
    app.dialog = Dialog::ImportDxf;
}

const LAYERS_TEXT: &str = "Select a DXF file containing a network layout and the drawing \
layers that contain network links. Only the lines are imported: pipes get the default \
properties and nodes are added where their ends meet.";
const PREVIEW_TEXT: &str = "A preview of the imported network layout is displayed below. Click \
the Import button to begin importing it into the project.";

/// The lines on the ticked layers.
fn chosen_lines(s: &DxfImport) -> Vec<&[[f64; 2]]> {
    s.drawing.as_ref().map_or_else(Vec::new, |d| {
        d.lines
            .iter()
            .filter(|(l, _)| s.chosen.get(*l).copied().unwrap_or(false))
            .map(|(_, p)| &p[..])
            .collect()
    })
}

/// Whether a ticked layer has lines.
fn any_chosen(s: &DxfImport) -> bool {
    s.chosen.iter().zip(&s.counts).any(|(c, n)| *c && *n > 0)
}

/// Identifies the drawing and the ticked layers, for the preview.
fn preview_key(s: &DxfImport) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (s.generation, &s.chosen).hash(&mut h);
    h.finish()
}

fn layers_page(ui: &mut Ui, s: &mut DxfImport) {
    let ctx = ui.ctx().clone();
    wizard::note(ui, LAYERS_TEXT);
    ui.add_space(14.0);
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(14, 0))
        .show(ui, |ui| {
            match wizard::file_row(ui, "DXF File:", &s.name, "Select a DXF file") {
                FileAction::Open => crate::io::open_file(&s.inbox, &ctx, "DXF", &["dxf"]),
                FileAction::Clear => {
                    s.name.clear();
                    s.drawing = None;
                    s.counts.clear();
                    s.chosen.clear();
                }
                FileAction::None => {}
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new(tr("Select the drawing layers that contain network links:"))
                    .size(14.5),
            );
            ui.add_space(4.0);
            let (rect, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), ui.available_height() - 40.0),
                Sense::hover(),
            );
            ui.painter().rect(
                rect,
                0.0,
                wizard::white(ui),
                wizard::border(ui),
                egui::StrokeKind::Inside,
            );
            let mut list = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(6.0)));
            if let Some(d) = &s.drawing {
                let counts = &s.counts;
                egui::ScrollArea::vertical()
                    .id_salt("dxf_layers")
                    .auto_shrink(false)
                    .show(&mut list, |ui| {
                        for (k, name) in d.layers.iter().enumerate() {
                            let label = format!("{name}  ({})", trf("{} lines", &[&counts[k]]));
                            ui.add_enabled(
                                counts[k] > 0,
                                crate::widgets::Checkbox::new(&mut s.chosen[k], label),
                            );
                        }
                    });
            }
            if s.drawing.is_some() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    if wizard::link_label(ui, "Select All", true) {
                        for (c, n) in s.chosen.iter_mut().zip(s.counts.iter().copied()) {
                            *c = n > 0;
                        }
                    }
                    if wizard::link_label(ui, "Select None", true) {
                        s.chosen.iter_mut().for_each(|c| *c = false);
                    }
                });
            }
        });
}

fn options_page(ui: &mut Ui, s: &mut DxfImport) {
    ui.add_space(56.0);
    ui.spacing_mut().item_spacing.y = 14.0;
    wizard::option_row(ui, "", 0.0, |ui| {
        ui.label(
            RichText::new(tr("Provide values for the import options listed below:")).size(14.5),
        );
    });
    ui.add_space(4.0);
    wizard::option_row(ui, "", 0.0, |ui| {
        crate::widgets::checkbox(
            ui,
            &mut s.as_map_layers,
            "Add as Map Layers (one group, a layer for each drawing layer)",
        );
    });
    // the options below are for pipes
    ui.add_enabled_ui(!s.as_map_layers, |ui| pipe_options(ui, s));
}

fn pipe_options(ui: &mut Ui, s: &mut DxfImport) {
    wizard::option_row(ui, "Drawing Units", 30.0, |ui| {
        egui::ComboBox::from_id_salt("dxf_units")
            .selected_text(tr(MapUnits::ALL[s.units].label()))
            .width(138.0)
            .show_ui(ui, |ui| {
                for (k, u) in MapUnits::ALL.iter().enumerate() {
                    ui.selectable_value(&mut s.units, k, tr(u.label()));
                }
            });
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
    wizard::option_row(ui, "", 0.0, |ui| {
        crate::widgets::checkbox(ui, &mut s.add_to_project, "Add to Existing Project");
    });
}

/// Takes a drawing that was read, with its layers that have lines ticked.
fn choose(s: &mut DxfImport, name: String, d: Drawing) {
    if let Some(u) = d.units {
        s.units = MapUnits::ALL.iter().position(|m| *m == u).unwrap_or(0);
    }
    s.counts = d.counts();
    s.chosen = s.counts.iter().map(|n| *n > 0).collect();
    s.generation += 1;
    s.name = name;
    s.drawing = Some(d);
    s.error = None;
}

/// The DXF File Importer window.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let Some(s) = app.dxf_import.as_mut() else {
        return true;
    };
    if let Some(item) = s.inbox.drain().into_iter().next() {
        match item.and_then(|(name, bytes)| Ok((name, crate::dxf::read(&bytes)?))) {
            Ok((name, d)) => choose(s, name, d),
            Err(e) => s.error = Some(e),
        }
    }
    ui.add_space(8.0);
    let names: &[&str] = if s.tab == 2 {
        &["Layers", "Options", "Preview"]
    } else {
        &["Layers", "Options"]
    };
    wizard::tabs(ui, &mut s.tab, names);
    wizard::page(ui, |ui| match s.tab {
        0 => layers_page(ui, s),
        1 => options_page(ui, s),
        _ => {
            wizard::note(ui, PREVIEW_TEXT);
            ui.add_space(12.0);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(14, 0))
                .show(ui, |ui| {
                    let h = ui.available_height() - 14.0;
                    let key = preview_key(s);
                    let s = &*s;
                    wizard::preview_cached(
                        ui,
                        key,
                        || chosen_lines(s),
                        "No layers were selected",
                        h,
                    );
                });
        }
    });
    wizard::error(ui, &s.error);
    ui.add_space(20.0);
    let can_import = any_chosen(s);
    let nav = wizard::buttons(ui, &mut s.tab, 2, can_import);
    if nav == Nav::Import {
        let as_layers = app.dxf_import.as_ref().is_some_and(|s| s.as_map_layers);
        match if as_layers {
            add_map_layers(app)
        } else {
            run(app)
        } {
            Ok(msg) => {
                app.dxf_import = None;
                app.finish_import(msg);
                return false;
            }
            Err(e) => {
                if let Some(s) = app.dxf_import.as_mut() {
                    s.error = Some(e);
                }
            }
        }
    }
    let close = nav == Nav::Cancel;
    if close {
        app.dxf_import = None;
    }
    close
}

/// Adds the ticked layers to the map as map layers, in a group named after
/// the file, each in a color of its own. The drawing's lines move into them.
fn add_map_layers(app: &mut EpanetApp) -> Result<String, String> {
    use crate::layers::{DXF_COLORS, LineSet, Lines, MapLayer, Service};
    let s = app.dxf_import.as_mut().ok_or("nothing to import")?;
    if !any_chosen(s) {
        return Err("Select one or more layers with lines".into());
    }
    let d = s.drawing.take().ok_or("Select a DXF file")?;
    let group = s
        .name
        .rsplit_once('.')
        .map_or(s.name.as_str(), |(stem, _)| stem)
        .to_string();
    // each layer's lines and their entities
    type Parts = (Vec<Vec<[f64; 2]>>, Vec<crate::dxf::Entity>);
    let mut per_layer: Vec<Parts> = vec![Default::default(); d.layers.len()];
    let mut entities = d.entities.into_iter();
    for (l, pts) in d.lines {
        let e = entities.next().unwrap_or_default();
        if s.chosen.get(l).copied().unwrap_or(false) {
            per_layer[l].0.push(pts);
            per_layer[l].1.push(e);
        }
    }
    let mut added = Vec::new();
    for (k, (name, (lines, entities))) in d.layers.into_iter().zip(per_layer).enumerate() {
        if lines.is_empty() {
            continue;
        }
        let set = LineSet::new(lines, entities);
        added.push(MapLayer {
            name,
            service: Service::Dxf,
            url: s.name.clone(),
            group: group.clone(),
            color: DXF_COLORS[k % DXF_COLORS.len()],
            fill: set.fill_by_default(),
            lines: Lines(std::sync::Arc::new(set)),
            ..Default::default()
        });
    }
    let bounds = added
        .iter()
        .filter_map(|l| l.lines.0.bounds)
        .reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        });
    let msg = trf("Added {} map layers in {}", &[&added.len(), &group]);
    // the panel lists the last layer on top: keep the drawing's order from the top
    added.reverse();
    app.prefs.map_layers.extend(added);
    app.prefs.hide_layers = false;
    app.prefs.docks.activate(crate::dock::Pane::Layers);
    // a map with no network shows the drawing
    if app.net.nodes.is_empty() {
        app.view.fit_to = bounds;
        app.view.fit_pending = true;
    }
    Ok(msg)
}

/// Imports the lines of the chosen layers.
fn run(app: &mut EpanetApp) -> Result<String, String> {
    let s = app.dxf_import.as_ref().ok_or("nothing to import")?;
    let snap = eui_core::num::parse(&s.snap).ok_or("Enter a number for the snap tolerance")?;
    let links: Vec<InLink> = chosen_lines(s)
        .into_iter()
        .map(|p| InLink {
            points: p.to_vec(),
            ..Default::default()
        })
        .collect();
    if links.is_empty() {
        return Err("Select one or more layers with lines".into());
    }
    let units = MapUnits::ALL[s.units];
    let opt = BuildOptions {
        units,
        snap_m: if s.snap_feet { snap * 0.3048 } else { snap },
        compute_lengths: s.compute_lengths || app.prefs.auto_length,
    };
    let add = s.add_to_project;
    let mut net = app.import_target(add);
    let setup = app.setup.clone();
    let sum = crate::import::build(&mut net, &setup, &[], &links, &opt);
    if !add || app.net.nodes.is_empty() {
        net.set_option("BACKDROP", "UNITS", &units.label().to_ascii_uppercase());
    }
    let mut msg = trf(
        "Imported {} nodes and {} pipes",
        &[&sum.nodes_added, &sum.links_added],
    );
    if sum.skipped > 0 {
        msg += &trf("; skipped {} lines", &[&sum.skipped]);
    }
    app.place_import(net, add, msg.clone())?;
    Ok(msg)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The importer on page `tab`, with the sample drawing chosen.
    pub(crate) fn set_tab(app: &mut EpanetApp, tab: usize) {
        let d = crate::dxf::read(crate::dxf::tests::SAMPLE.as_bytes()).unwrap();
        let s = app.dxf_import.as_mut().unwrap();
        choose(s, "network.dxf".into(), d);
        s.tab = tab;
    }

    /// Without "Add to Existing Project" the lines make a new project, after
    /// the question about the unsaved changes of the current one.
    #[test]
    fn imports_into_a_new_project() {
        let mut app = app();
        let setup = app.setup.clone();
        app.checkpoint();
        setup.add_node(&mut app.net, eui_core::NodeKind::Junction, [0.0, 0.0]);
        assert!(app.dirty);
        open(&mut app);
        set_tab(&mut app, 2);
        app.dxf_import.as_mut().unwrap().add_to_project = false;
        let msg = run(&mut app).unwrap();
        assert_eq!(app.dialog, Dialog::ConfirmReplace);
        app.finish_import(msg);
        assert_eq!(app.dialog, Dialog::ConfirmReplace);
        app.resolve_document_change(1);
        assert_eq!(app.net.links.len(), 3);
        assert_eq!(
            app.net.option("BACKDROP", "UNITS").as_deref(),
            Some("METERS")
        );
        assert!(app.dirty && app.view.fit_pending);
    }

    /// With "Add as Map Layers" the ticked layers become map layers in a
    /// group named after the file, the network stays as it was, and a
    /// project file keeps their lines.
    #[test]
    fn adds_the_layers_as_a_group_of_map_layers() {
        use crate::layers::Service;
        let mut app = app();
        open(&mut app);
        set_tab(&mut app, 1);
        app.dxf_import.as_mut().unwrap().as_map_layers = true;
        let msg = add_map_layers(&mut app).unwrap();
        assert_eq!(msg, "Added 2 map layers in network");
        assert!(app.net.links.is_empty());
        let layers = &app.prefs.map_layers;
        // the drawing's first layer on top of the panel, i.e. last
        let names: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["MAINS", "PIPES"]);
        assert!(
            layers
                .iter()
                .all(|l| l.group == "network" && l.service == Service::Dxf)
        );
        assert_eq!(layers[1].lines.0.lines.len(), 2);
        // each line keeps its entity
        assert_eq!(layers[1].lines.0.cell(0, 0), "LINE");
        assert_eq!(layers[1].lines.0.cell(1, 0), "LWPOLYLINE");
        assert_eq!(layers[1].lines.0.bounds, Some([0.0, 0.0, 200.0, 80.0]));
        assert_ne!(layers[0].color, layers[1].color);
        assert!(app.unsaved());

        let bytes = crate::project::write(&app).unwrap();
        let o = crate::project::read(&bytes).unwrap();
        let back = &o.project.map_layers;
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].lines.0.lines, layers[1].lines.0.lines);
        assert_eq!(back[0].lines.0.lines, layers[0].lines.0.lines);
    }

    fn app() -> EpanetApp {
        EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ))
    }

    #[test]
    fn imports_the_ticked_layers() {
        let mut app = app();
        open(&mut app);
        let d = crate::dxf::read(crate::dxf::tests::SAMPLE.as_bytes()).unwrap();
        let s = app.dxf_import.as_mut().unwrap();
        // PIPES only, not MAINS
        s.chosen = vec![true, false, false];
        s.drawing = Some(d);
        s.units = 2;
        s.snap = "0,5".into();
        let msg = run(&mut app).unwrap();
        assert_eq!(msg, "Imported 3 nodes and 2 pipes");
        let net = &app.net;
        assert_eq!(net.links.len(), 2);
        // the polyline starts within the snap tolerance of the line's end
        assert_eq!(net.links[1].from, net.links[0].to);
        assert_eq!(net.links[1].vertices, vec![[100.0, 80.0]]);
        assert_eq!(net.links[0].field(0), "100");
        assert!(app.dirty);
    }
}
