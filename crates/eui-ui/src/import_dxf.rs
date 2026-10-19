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
    /// The layers ticked for import.
    chosen: Vec<bool>,
    units: usize,
    snap: String,
    snap_feet: bool,
    compute_lengths: bool,
    add_to_project: bool,
    error: Option<String>,
    inbox: Inbox,
}

impl Default for DxfImport {
    fn default() -> Self {
        DxfImport {
            tab: 0,
            name: String::new(),
            drawing: None,
            chosen: Vec::new(),
            units: 0,
            snap: "0.00".into(),
            snap_feet: false,
            compute_lengths: true,
            add_to_project: true,
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
                let counts = d.counts();
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
                        let counts = s.drawing.as_ref().map(Drawing::counts).unwrap_or_default();
                        for (c, n) in s.chosen.iter_mut().zip(counts) {
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
    s.chosen = d.counts().iter().map(|n| *n > 0).collect();
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
                    let lines = chosen_lines(s);
                    wizard::preview(ui, &lines, &[], "No layers were selected", h);
                });
        }
    });
    wizard::error(ui, &s.error);
    ui.add_space(20.0);
    let can_import = !chosen_lines(s).is_empty();
    let nav = wizard::buttons(ui, &mut s.tab, 2, can_import);
    if nav == Nav::Import {
        match run(app) {
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
