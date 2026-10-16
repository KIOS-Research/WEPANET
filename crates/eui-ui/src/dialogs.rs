//! Dialog windows: Welcome Page, Project Summary, Object Locater, Map Query,
//! Map Display Settings, legend editors, Units, Error Codes, About, Run Status,
//! Program Preferences and Confirm Deletion.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use eui_core::props::Element;
use eui_core::{LinkKind, NodeKind};

use crate::app::{Dialog, EpanetApp};
use crate::style::{Legend, LinkTheme, NodeTheme};

pub fn show(app: &mut EpanetApp, ctx: &egui::Context) {
    let dialog = app.dialog;
    // A dialog with Cancel was left some other way (another dialog took its place).
    if !matches!(dialog, Dialog::MapSettings | Dialog::Preferences) {
        app.prefs_backup = None;
    }
    if dialog == Dialog::ConfirmReplace {
        egui::Modal::new(egui::Id::new("unsaved_project")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(
                RichText::new("Save changes before opening another project?")
                    .font(crate::fonts::bold(14.0)),
            );
            ui.add_space(8.0);
            ui.label("Your current project has unsaved changes.");
            ui.add_space(12.0);
            if app.save_pending() {
                ui.spinner();
                ui.label("Waiting for the save to finish…");
            } else {
                ui.horizontal(|ui| {
                    for (i, label) in ["Save", "Discard", "Cancel"].iter().enumerate() {
                        if ui.button(*label).clicked() {
                            app.resolve_document_change(i);
                        }
                    }
                });
                ui.label(RichText::new(&app.status).weak());
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app.resolve_document_change(2);
                }
            }
        });
        return;
    }
    let title = match dialog {
        Dialog::None => return,
        Dialog::Welcome => "Welcome Page",
        Dialog::Summary => "Project Summary",
        Dialog::Locate => "Object Locater",
        Dialog::Query => "Map Query",
        Dialog::MapSettings => "Map Display Settings",
        Dialog::NodeLegend => "Legend Editor",
        Dialog::LinkLegend => "Legend Editor",
        Dialog::Units => "Units of Measurement",
        Dialog::ErrorCodes => "Error Codes",
        Dialog::About => "About WEPANET",
        Dialog::RunStatus => "Run Status",
        Dialog::Preferences => "Program Preferences",
        Dialog::Basemap => "Select a Basemap Source",
        Dialog::ImportShapefile => "Shapefile Importer",
        Dialog::Georeference => "Map Georeference Tool",
        Dialog::BasemapAlign => "Map Alignment Tool",
        Dialog::Setup => "Project Setup",
        Dialog::PluginMessage => "Plugins",
        Dialog::PluginTable => "Plugin Results",
        Dialog::Elevations => "Elevations",
        Dialog::FireFlowCurve => "Fire Flow Curve",
        Dialog::Benchmarks => "WaterBenchmarkHub Networks",
        Dialog::PluginManager => "Plugin Manager",
        Dialog::Msx => "Multi-Species Quality Editor",
        Dialog::SourceEditor => "Source Quality Editor",
        Dialog::PatternSelector => "Time Pattern Selector",
        Dialog::PatternEditor => "Time Pattern Editor",
        Dialog::CurveSelector => "Data Curve Selector",
        Dialog::CurveEditor => "Data Curve Editor",
        Dialog::QualityEditor => "Single Species Water Quality Editor",
        Dialog::ControlsEditor => "Simple Controls Editor",
        Dialog::RulesEditor => "Rule-Based Controls Editor",
        Dialog::ConfirmDelete => "Confirm Deletion",
        Dialog::ConfirmReplace => "Unsaved Changes",
    };
    // Message boxes are all white and can only be closed with their buttons.
    let message = matches!(
        dialog,
        Dialog::RunStatus | Dialog::ConfirmDelete | Dialog::PluginMessage
    );
    let closable = !message && dialog != Dialog::Preferences;
    // Dialogs that lay out their own white panel (or, for the Welcome Page, the banner).
    let own_panel = matches!(
        dialog,
        Dialog::Welcome
            | Dialog::About
            | Dialog::Preferences
            | Dialog::MapSettings
            | Dialog::ImportShapefile
    );
    let style = ctx.style_of(ctx.theme());
    let dark = style.visuals.dark_mode;
    let fill = if message && !dark {
        Color32::WHITE
    } else if dark {
        style.visuals.panel_fill
    } else {
        Color32::from_rgb(0xf3, 0xf3, 0xf3)
    };
    let mut close = false;
    egui::Window::new(title)
        .id(egui::Id::new(("dialog", title)))
        .title_bar(false)
        .collapsible(false)
        .resizable(dialog == Dialog::ErrorCodes)
        .min_width(match dialog {
            Dialog::Welcome => 735.0,
            Dialog::RunStatus | Dialog::ConfirmDelete => 320.0,
            Dialog::About => 470.0,
            Dialog::Preferences => 330.0,
            Dialog::Benchmarks => 880.0,
            Dialog::PluginManager => 800.0,
            _ => 320.0,
        })
        .max_width(if dialog == Dialog::Welcome {
            735.0
        } else {
            f32::INFINITY
        })
        .frame(egui::Frame::window(&style).fill(fill).inner_margin(0))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // EPANET-UI's window chrome: the title on the left, a red close
            // button on the right (drawn once the width is known).
            let top = ui.cursor().top();
            ui.add_space(TITLE_H);
            let margin = if dialog == Dialog::Welcome {
                egui::Margin::ZERO
            } else {
                egui::Margin {
                    left: 16,
                    right: 16,
                    top: 6,
                    bottom: 16,
                }
            };
            egui::Frame::NONE.inner_margin(margin).show(ui, |ui| {
                let mut content = |ui: &mut Ui| match dialog {
                    Dialog::Welcome => welcome(app, ui),
                    Dialog::Summary => summary(app, ui),
                    Dialog::Locate => locate(app, ui),
                    Dialog::Query => query(app, ui),
                    Dialog::MapSettings => map_settings(app, ui),
                    Dialog::NodeLegend => legend_editor(app, ui, true),
                    Dialog::LinkLegend => legend_editor(app, ui, false),
                    Dialog::Units => units(app, ui),
                    Dialog::ErrorCodes => error_codes(ui),
                    Dialog::About => about(ui),
                    Dialog::ConfirmDelete => confirm_delete(app, ui),
                    Dialog::ConfirmReplace => false,
                    Dialog::RunStatus => run_status(app, ui),
                    Dialog::Preferences => preferences(app, ui),
                    Dialog::Basemap => crate::basemap::dialog(app, ui),
                    Dialog::ImportShapefile => crate::import_shp::dialog(app, ui),
                    Dialog::Georeference => crate::basemap::georef_dialog(app, ui),
                    Dialog::BasemapAlign => crate::basemap::align_dialog(app, ui),
                    Dialog::Setup => crate::setup::dialog(app, ui),
                    Dialog::PluginMessage => crate::plugins::message(app, ui),
                    Dialog::PluginTable => crate::scripting::table_dialog(app, ui),
                    Dialog::Elevations => crate::plugins::elevations(app, ui),
                    Dialog::FireFlowCurve => crate::plugins::curve(app, ui),
                    Dialog::Benchmarks => crate::catalog::dialog(app, ui),
                    Dialog::PluginManager => crate::plugin_manager::dialog(app, ui),
                    Dialog::Msx => crate::msx::dialog(app, ui),
                    Dialog::SourceEditor => crate::source::dialog(app, ui),
                    Dialog::PatternSelector => crate::forms::pattern_selector(app, ui),
                    Dialog::PatternEditor => crate::forms::pattern_editor(app, ui),
                    Dialog::CurveSelector => crate::forms::curve_selector(app, ui),
                    Dialog::CurveEditor => crate::forms::curve_editor(app, ui),
                    Dialog::QualityEditor => crate::forms::quality_editor(app, ui),
                    Dialog::ControlsEditor => crate::forms::controls_editor(app, ui),
                    Dialog::RulesEditor => crate::forms::rules_editor(app, ui),
                    Dialog::None => true,
                };
                close = if own_panel || message {
                    content(ui)
                } else {
                    panel(ui, content)
                };
            });
            let w = ui.min_rect();
            let bar = Rect::from_min_max(pos2(w.left(), top), pos2(w.right(), top + TITLE_H));
            ui.painter().text(
                bar.left_center() + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr(title),
                FontId::proportional(14.5),
                ui.visuals().text_color(),
            );
            if closable {
                let x = Rect::from_min_size(
                    pos2(bar.right() - 46.0, bar.top() + 4.0),
                    vec2(40.0, TITLE_H - 8.0),
                );
                let resp = ui.interact(x, ui.id().with("dialog_close"), Sense::click());
                let red = if resp.hovered() {
                    Color32::from_rgb(0xe8, 0x11, 0x23)
                } else {
                    Color32::from_rgb(0xc4, 0x4b, 0x4b)
                };
                ui.painter().rect_filled(x, 2.0, red);
                let c = x.center();
                let s = Stroke::new(1.6, Color32::WHITE);
                ui.painter()
                    .line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], s);
                ui.painter()
                    .line_segment([c + vec2(5.0, -5.0), c + vec2(-5.0, 5.0)], s);
                if resp.on_hover_text("Close").clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    if dialog == Dialog::MapSettings {
                        cancel_map_settings(app);
                    }
                    close = true;
                }
            }
        });
    if close && app.dialog == dialog {
        app.dialog = Dialog::None;
    }
}

/// Height of the dialog title bar.
const TITLE_H: f32 = 34.0;

/// A Welcome Page link: a small bullet then the text, as in EPANET-UI.
fn welcome_link(ui: &mut Ui, text: &str, icon: crate::icons::Icon) -> bool {
    let font = egui::FontId::proportional(14.0);
    let w = ui.fonts_mut(|f| {
        f.layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    });
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w + 24.0, 30.0), egui::Sense::click());
    let color = ui.visuals().text_color();
    crate::icons::paint(
        ui.painter(),
        icon,
        rect.left_center() + egui::vec2(7.0, 0.0),
        10.0,
        HEADING_LIGHT,
    );
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let pos = rect.left_center() + egui::vec2(18.0, -galley.size().y / 2.0);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter().hline(
            pos.x..=pos.x + galley.size().x,
            pos.y + galley.size().y,
            egui::Stroke::new(1.0, color),
        );
    }
    ui.painter().galley(pos, galley, color);
    resp.clicked()
}

fn welcome(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    use crate::icons::Icon;
    let ctx = ui.ctx().clone();
    let mut close = false;
    welcome_banner(ui);
    let heading = |ui: &mut Ui, t: &str| {
        ui.label(
            RichText::new(t)
                .size(24.0)
                .color(if ui.visuals().dark_mode {
                    HEADING_LIGHT
                } else {
                    HEADING
                }),
        );
        ui.add_space(4.0);
    };
    egui::Frame::NONE
        .inner_margin(egui::Margin { left: 24, right: 20, top: 24, bottom: 22 })
        .show(ui, |ui| {
            egui::Frame::NONE
                .fill(if ui.visuals().dark_mode {
                    ui.visuals().extreme_bg_color
                } else {
                    egui::Color32::WHITE
                })
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(200)))
                .inner_margin(egui::Margin { left: 46, right: 20, top: 22, bottom: 18 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.columns(2, |cols| {
                        heading(&mut cols[0], "Develop");
                        if welcome_link(&mut cols[0], "Create a new project", Icon::Junction) {
                            app.new_project();
                            close = true;
                        }
                        cols[0].add_space(10.0);
                        if welcome_link(&mut cols[0], "Open an existing project", Icon::Junction) {
                            app.open(&ctx);
                            close = true;
                        }
                        cols[0].add_space(34.0);
                        heading(&mut cols[0], "Learn");
                        if welcome_link(&mut cols[0], "Basic Tutorial", Icon::Junction) {
                            crate::help::open_doc(&ctx, crate::help::Doc::Tutorial, None);
                        }
                        cols[0].add_space(10.0);
                        if welcome_link(&mut cols[0], "User Guide", Icon::Junction) {
                            crate::help::open_doc(&ctx, crate::help::Doc::Manual, None);
                        }

                        heading(&mut cols[1], "Open a Recent Project");
                        #[cfg(not(target_arch = "wasm32"))]
                        for r in app.prefs.recent.clone() {
                            let name = std::path::Path::new(&r)
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or(r.clone());
                            if welcome_link(&mut cols[1], &name, Icon::Open) {
                                app.open_recent(&r);
                                close = true;
                            }
                        }
                        if app.prefs.recent.is_empty() || cfg!(target_arch = "wasm32") {
                            cols[1].label(RichText::new("There are no recent projects").weak());
                        }
                        cols[1].add_space(16.0);
                        cols[1].label(RichText::new("Sample projects").size(14.0).strong());
                        for (name, text) in eui_core::examples::ALL {
                            if welcome_link(&mut cols[1], &format!("{name}.inp"), Icon::Open) {
                                app.load_example(name, text);
                                close = true;
                            }
                        }
                    });
                    ui.add_space(18.0);
                    ui.separator();
                    ui.add_space(6.0);
                    heading(ui, "WaterBenchmarkHub");
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(
                                "Benchmark water distribution networks collected by the WaterFutures project.",
                            )
                            .size(13.5),
                        );
                    });
                    ui.add_space(2.0);
                    if welcome_link(ui, "Open a network from the WaterBenchmarkHub catalog", Icon::Open) {
                        app.dialog = Dialog::Benchmarks;
                    }
                    ui.add_space(18.0);
                    crate::widgets::checkbox(
                        ui,
                        &mut app.prefs.show_welcome,
                        RichText::new("Always show at startup").size(14.0),
                    );
                });
        });
    close
}

/// The white, framed content area of EPANET-UI's dialogs.
fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let dark = ui.visuals().dark_mode;
    egui::Frame::NONE
        .fill(if dark {
            ui.visuals().extreme_bg_color
        } else {
            egui::Color32::WHITE
        })
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(190)))
        .inner_margin(egui::Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A row of equal, centered dialog buttons; returns the index of the one clicked.
pub(crate) fn buttons(ui: &mut Ui, labels: &[&str]) -> Option<usize> {
    let w = 96.0;
    let gap = 14.0;
    let total = labels.len() as f32 * w + (labels.len() - 1) as f32 * gap;
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
        ui.spacing_mut().item_spacing.x = gap;
        for (k, l) in labels.iter().enumerate() {
            if ui
                .add_sized([w, 30.0], egui::Button::new(crate::i18n::tr(l)))
                .clicked()
            {
                clicked = Some(k);
            }
        }
    });
    clicked
}

/// While an analysis runs: how far it has got and a Cancel button, as in
/// EPANET-UI; then whether it succeeded, with OK.
fn run_status(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    ui.set_min_width(380.0);
    ui.add_space(18.0);
    if let Some(run) = &app.run {
        let text = match run.progress() {
            Some(p) => format!(
                "Solving {} at hour {}",
                match p.stage {
                    eui_engine::Stage::Hydraulics => "hydraulics",
                    eui_engine::Stage::Quality => "water quality",
                    eui_engine::Stage::Msx => "multi-species quality (EPANET-MSX)",
                },
                p.hour
            ),
            None => "Reading input data…".into(),
        };
        ui.vertical_centered(|ui| ui.label(RichText::new(text).size(14.0)));
        ui.add_space(30.0);
        if buttons(ui, &["Cancel"]).is_some() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            run.cancel();
        }
        ui.add_space(8.0);
        return false;
    }
    ui.vertical_centered(|ui| {
        for line in app.run_message.lines() {
            ui.label(RichText::new(line).size(14.0));
        }
    });
    ui.add_space(30.0);
    let close = buttons(ui, &["OK"]).is_some()
        || ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
    ui.add_space(8.0);
    close
}

fn confirm_delete(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let what = match app.selection {
        Some(Element::Node(i)) => {
            format!("{} {}", app.net.nodes[i].kind.label(), app.net.nodes[i].id)
        }
        Some(Element::Link(i)) => {
            format!("{} {}", app.net.links[i].kind.label(), app.net.links[i].id)
        }
        None => return true,
    };
    ui.add_space(14.0);
    ui.vertical_centered(|ui| ui.label(RichText::new(format!("Delete {what}?")).size(14.0)));
    ui.add_space(24.0);
    let b = buttons(ui, &["Yes", "No"]);
    ui.add_space(6.0);
    if b == Some(0) || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        app.delete_selection();
        return true;
    }
    b == Some(1) || ui.input(|i| i.key_pressed(egui::Key::Escape))
}

fn preferences(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    if app.prefs_backup.is_none() {
        app.prefs_backup = Some(app.prefs.clone());
    }
    let clear_id = egui::Id::new("prefs_clear_recent");
    let mut clear_recent: bool = ctx.data(|d| d.get_temp(clear_id)).unwrap_or(false);
    ui.add_space(6.0);
    panel(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 9.0;
        let p = &mut app.prefs;
        crate::widgets::checkbox(ui, &mut p.blink_marker, "Blinking Map Marker");
        crate::widgets::checkbox(ui, &mut p.map_hints, "Flyover Map Hints");
        crate::widgets::checkbox(ui, &mut p.confirm_delete, "Confirm Deletions");
        crate::widgets::checkbox(ui, &mut p.show_welcome, "Show Welcome Page at Start");
        let desktop = !cfg!(target_arch = "wasm32");
        ui.add_enabled(
            desktop,
            crate::widgets::Checkbox::new(&mut p.open_last, "Open Last File at Start"),
        )
        .on_disabled_hover_text("Browsers cannot reopen files by themselves");
        ui.add_enabled(
            desktop,
            crate::widgets::Checkbox::new(&mut p.auto_backup, "Automatic Backup File"),
        )
        .on_hover_text("Keep the previous version as a .bak file when saving")
        .on_disabled_hover_text("Not available in the browser");
        crate::widgets::checkbox(ui, &mut p.blue_theme, "Change Color Theme")
            .on_hover_text("Blue instead of gray menus and panel headers");
        if crate::widgets::checkbox(ui, &mut p.dark, "Dark Mode").changed() {
            app.apply_theme(&ctx);
        }
        crate::widgets::checkbox(ui, &mut clear_recent, "Clear Recent Projects List");
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Numerical Precision"));
            ui.add_space(30.0);
            ui.add(egui::DragValue::new(&mut app.prefs.decimals).range(0..=6));
        });
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Language"));
            ui.add_space(30.0);
            let current = crate::i18n::all()
                .iter()
                .find(|l| l.code == app.prefs.language)
                .map_or("English", |l| l.name.as_str());
            egui::ComboBox::from_id_salt("prefs_language")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for l in crate::i18n::all() {
                        ui.selectable_value(&mut app.prefs.language, l.code.to_string(), &l.name);
                    }
                });
        });
    });
    ui.add_space(12.0);
    let b = buttons(ui, &["Help", "OK", "Cancel"]);
    ui.add_space(6.0);
    let mut close = false;
    match b {
        Some(0) => crate::help::open(&ctx, "Program Preferences"),
        Some(1) => {
            if clear_recent {
                app.prefs.recent.clear();
            }
            close = true;
        }
        Some(2) => {
            if let Some(old) = app.prefs_backup.take() {
                app.prefs = old;
                app.apply_theme(&ctx);
            }
            close = true;
        }
        _ => {}
    }
    if close {
        app.prefs_backup = None;
        clear_recent = false;
    }
    ctx.data_mut(|d| d.insert_temp(clear_id, clear_recent));
    close
}

/// One expandable line of the Project Summary tree.
fn tree_node(ui: &mut Ui, id: &str, label: String, open: bool, body: impl FnOnce(&mut Ui)) {
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.id().with(("summary", id)),
        open,
    )
    .show_header(ui, |ui| ui.label(label))
    .body(body);
}

/// A leaf of the Project Summary tree, lined up with the expandable lines.
fn tree_leaf(ui: &mut Ui, label: String) {
    ui.horizontal(|ui| {
        ui.add_space(ui.spacing().indent);
        ui.label(label);
    });
}

fn summary(app: &EpanetApp, ui: &mut Ui) -> bool {
    let net = &app.net;
    let opt = |section: &str, key: &str, default: &str| {
        net.option(section, key)
            .unwrap_or_else(|| default.to_string())
    };
    ui.set_min_size(vec2(330.0, 380.0));
    egui::ScrollArea::vertical()
        .max_height(460.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            tree_node(ui, "components", "Network Components".into(), true, |ui| {
                tree_node(
                    ui,
                    "nodes",
                    format!("{} Nodes", net.nodes.len()),
                    false,
                    |ui| {
                        for (k, name) in [
                            (NodeKind::Junction, "Junctions"),
                            (NodeKind::Reservoir, "Reservoirs"),
                            (NodeKind::Tank, "Tanks"),
                        ] {
                            tree_leaf(ui, format!("{} {name}", net.count_nodes(k)));
                        }
                    },
                );
                tree_node(
                    ui,
                    "links",
                    format!("{} Links", net.links.len()),
                    false,
                    |ui| {
                        tree_leaf(ui, format!("{} Pipes", net.count_links(LinkKind::Pipe)));
                        tree_leaf(ui, format!("{} Pumps", net.count_links(LinkKind::Pump)));
                        let valves: Vec<_> = net
                            .links
                            .iter()
                            .filter(|l| l.kind == LinkKind::Valve)
                            .collect();
                        tree_node(
                            ui,
                            "valves",
                            format!("{} Valves", valves.len()),
                            false,
                            |ui| {
                                for t in ["PRV", "PSV", "PBV", "FCV", "TCV", "GPV", "PCV"] {
                                    let n = valves
                                        .iter()
                                        .filter(|l| l.field(1).eq_ignore_ascii_case(t))
                                        .count();
                                    tree_leaf(ui, format!("{n} {t}s"));
                                }
                            },
                        );
                    },
                );
            });
            tree_node(ui, "data", "Data Objects".into(), true, |ui| {
                let curves = net.curves();
                tree_node(
                    ui,
                    "curves",
                    format!("{} Data Curves", curves.len()),
                    false,
                    |ui| {
                        for (t, name) in [
                            ("PUMP", "Pump Curves"),
                            ("EFFICIENCY", "Efficiency Curves"),
                            ("VOLUME", "Volume Curves"),
                            ("HEADLOSS", "Head Loss Curves"),
                            ("VALVE", "Valve Curves"),
                        ] {
                            let n = curves.iter().filter(|c| c.1 == t).count();
                            tree_leaf(ui, format!("{n} {name}"));
                        }
                        let generic = curves
                            .iter()
                            .filter(|c| {
                                !["PUMP", "EFFICIENCY", "VOLUME", "HEADLOSS", "VALVE"]
                                    .contains(&c.1.as_str())
                            })
                            .count();
                        tree_leaf(ui, format!("{generic} Generic Curves"));
                    },
                );
                tree_leaf(ui, format!("{} Time Patterns", net.patterns().len()));
                let simple = net.raw_lines("CONTROLS").count();
                let rules = net
                    .raw_lines("RULES")
                    .filter(|t| t[0].eq_ignore_ascii_case("RULE"))
                    .count();
                tree_node(
                    ui,
                    "controls",
                    format!("{} Link Controls", simple + rules),
                    false,
                    |ui| {
                        tree_leaf(ui, format!("{simple} Simple Controls"));
                        tree_leaf(ui, format!("{rules} Rule-Based Controls"));
                    },
                );
            });
            tree_node(ui, "options", "Analysis Options".into(), true, |ui| {
                let (model, chem, ..) = eui_core::props::quality_parts(net);
                let quality = match model.as_str() {
                    "Chemical" if !chem.is_empty() => chem,
                    "Trace" => "Source Trace".into(),
                    m => m.to_string(),
                };
                tree_leaf(
                    ui,
                    format!("{} Flow Units", net.flow_units().to_lowercase()),
                );
                tree_leaf(
                    ui,
                    format!("{} Head Loss Model", opt("OPTIONS", "HEADLOSS", "H-W")),
                );
                tree_leaf(
                    ui,
                    format!("{} Demand Model", opt("OPTIONS", "DEMAND MODEL", "DDA")),
                );
                tree_leaf(ui, format!("{quality} Quality Model"));
                tree_leaf(
                    ui,
                    format!("{} Hour Duration", opt("TIMES", "DURATION", "0:00")),
                );
            });
        });
    false
}
fn locate(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label("Find");
        crate::widgets::radio_value(ui, &mut app.locate_nodes, true, "Node");
        crate::widgets::radio_value(ui, &mut app.locate_nodes, false, "Link");
    });
    ui.horizontal(|ui| {
        ui.label("Named");
        let r = ui.text_edit_singleline(&mut app.locate_text);
        let go = ui.button("Find").clicked()
            || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        if go {
            let id = app.locate_text.trim().to_string();
            let found = if app.locate_nodes {
                app.net.node_index(&id).map(Element::Node)
            } else {
                app.net.link_index(&id).map(Element::Link)
            };
            match found {
                Some(sel) => {
                    app.select(Some(sel));
                    let pos = match sel {
                        Element::Node(i) => app.net.nodes[i].pos,
                        Element::Link(i) => {
                            app.net.link_ends(&app.net.links[i]).and_then(|(a, b)| {
                                let (pa, pb) = (app.net.nodes[a].pos?, app.net.nodes[b].pos?);
                                Some([(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0])
                            })
                        }
                    };
                    if let Some(p) = pos {
                        app.view.center = p;
                    }
                    app.view_mode = crate::app::ViewMode::Map;
                    app.status = format!("Located {id}");
                    close = true;
                }
                None => {
                    app.status = format!(
                        "{} \"{id}\" not found",
                        if app.locate_nodes { "Node" } else { "Link" }
                    )
                }
            }
        }
    });
    if let Some(Element::Node(i)) = app.selection {
        let id = app.net.nodes[i].id.clone();
        let adj: Vec<String> = app
            .net
            .links
            .iter()
            .filter(|l| l.from == id || l.to == id)
            .map(|l| l.id.clone())
            .collect();
        ui.separator();
        ui.label(RichText::new(format!("Adjacent Links of node {id}")).strong());
        ui.label(adj.join(", "));
    }
    close
}

fn query(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    // every theme but None, with the EPANET-MSX species
    let node_themes: Vec<(NodeTheme, String)> = app
        .node_themes()
        .into_iter()
        .skip(1)
        .map(|t| (t, app.node_theme_name(t)))
        .collect();
    let link_themes: Vec<(LinkTheme, String)> = app
        .link_themes()
        .into_iter()
        .skip(1)
        .map(|t| (t, app.link_theme_name(t)))
        .collect();
    fn name_of<T: PartialEq>(all: &[(T, String)], t: T) -> String {
        all.iter()
            .find(|(k, _)| *k == t)
            .map_or_else(String::new, |(_, n)| n.clone())
    }
    let q = &mut app.query;
    ui.horizontal(|ui| {
        ui.label("Find");
        crate::widgets::radio_value(ui, &mut q.nodes, true, "Nodes");
        crate::widgets::radio_value(ui, &mut q.nodes, false, "Links");
    });
    ui.horizontal(|ui| {
        ui.label("with");
        if q.nodes {
            let t = q.node_theme.unwrap_or(NodeTheme::Pressure);
            egui::ComboBox::from_id_salt("q_node")
                .selected_text(name_of(&node_themes, t))
                .show_ui(ui, |ui| {
                    for (th, name) in node_themes.iter().cloned() {
                        if ui.selectable_label(t == th, name).clicked() {
                            q.node_theme = Some(th);
                        }
                    }
                });
        } else {
            let t = q.link_theme.unwrap_or(LinkTheme::Flow);
            egui::ComboBox::from_id_salt("q_link")
                .selected_text(name_of(&link_themes, t))
                .show_ui(ui, |ui| {
                    for (th, name) in link_themes.iter().cloned() {
                        if ui.selectable_label(t == th, name).clicked() {
                            q.link_theme = Some(th);
                        }
                    }
                });
        }
        egui::ComboBox::from_id_salt("q_op")
            .selected_text(["Below", "Equal To", "Above"][q.op])
            .width(80.0)
            .show_ui(ui, |ui| {
                for (k, s) in ["Below", "Equal To", "Above"].iter().enumerate() {
                    ui.selectable_value(&mut q.op, k, *s);
                }
            });
        ui.add(egui::TextEdit::singleline(&mut q.value).desired_width(70.0));
    });
    let mut run = false;
    let mut clear = false;
    ui.horizontal(|ui| {
        run = ui.button("Find").clicked();
        clear = ui.button("Clear").clicked();
    });
    if clear {
        app.query.active = false;
        app.query.hits.clear();
    }
    if run {
        match app.query.value.trim().parse::<f32>() {
            Err(_) => app.status = "Enter a numeric value to search for".into(),
            Ok(target) => {
                let op = app.query.op;
                let test = |v: f32| match op {
                    0 => v < target,
                    1 => (v - target).abs() <= 1e-6 * target.abs().max(1.0),
                    _ => v > target,
                };
                let hits: Vec<usize> = if app.query.nodes {
                    let t = app.query.node_theme.unwrap_or(NodeTheme::Pressure);
                    (0..app.net.nodes.len())
                        .filter(|&i| app.node_theme_value_of(t, i).is_some_and(test))
                        .collect()
                } else {
                    let t = app.query.link_theme.unwrap_or(LinkTheme::Flow);
                    (0..app.net.links.len())
                        .filter(|&i| app.link_theme_value_of(t, i).is_some_and(test))
                        .collect()
                };
                app.query.hits = hits;
                app.query.active = true;
            }
        }
    }
    if app.query.active {
        ui.label(RichText::new(format!("{} items found", app.query.hits.len())).strong());
        ui.label(RichText::new("Matching items are highlighted on the map.").weak());
    }
    false
}

/// The pages of Map Display Settings, listed on its left.
const MAP_PAGES: [&str; 5] = ["Nodes", "Links", "Annotations", "Flow Arrows", "Background"];

/// EPANET-UI's Map Display Settings: a page list on the left, the page on the
/// right. Changes show on the map at once; Cancel (or the close button) undoes them.
fn map_settings(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    if app.prefs_backup.is_none() {
        app.prefs_backup = Some(app.prefs.clone());
    }
    let page_id = egui::Id::new("map_settings_page");
    let mut page: usize = ui.ctx().data(|d| d.get_temp(page_id)).unwrap_or(0);
    let dark = ui.visuals().dark_mode;
    let white = if dark {
        ui.visuals().extreme_bg_color
    } else {
        Color32::WHITE
    };
    let border = Stroke::new(1.0, Color32::from_gray(if dark { 90 } else { 160 }));
    let height = 410.0;
    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        // page list
        egui::Frame::NONE
            .fill(white)
            .stroke(border)
            .inner_margin(1)
            .show(ui, |ui| {
                ui.set_min_size(vec2(138.0, height));
                ui.set_max_width(138.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                // the frame sits in a row; its contents run top to bottom
                ui.vertical(|ui| {
                    for (k, name) in MAP_PAGES.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(vec2(138.0, 38.0), Sense::click());
                        let selected = k == page;
                        if selected {
                            ui.painter()
                                .rect_filled(r, 0.0, Color32::from_rgb(0x00, 0x78, 0xd7));
                        } else if resp.hovered() {
                            ui.painter()
                                .rect_filled(r, 0.0, ui.visuals().widgets.hovered.bg_fill);
                        }
                        ui.painter().text(
                            r.left_center() + vec2(4.0, 0.0),
                            Align2::LEFT_CENTER,
                            *name,
                            FontId::proportional(14.5),
                            if selected {
                                Color32::WHITE
                            } else {
                                ui.visuals().text_color()
                            },
                        );
                        if resp.clicked() {
                            page = k;
                        }
                    }
                });
            });
        // the page
        egui::Frame::NONE
            .fill(white)
            .stroke(border)
            .inner_margin(egui::Margin {
                left: 28,
                right: 16,
                top: 20,
                bottom: 16,
            })
            .show(ui, |ui| {
                ui.set_min_size(vec2(300.0 - 44.0, height - 36.0));
                ui.set_max_width(300.0 - 44.0);
                ui.vertical(|ui| map_settings_page(app, ui, page));
            });
    });
    ui.ctx().data_mut(|d| d.insert_temp(page_id, page));
    ui.add_space(14.0);
    let mut choice = None;
    ui.horizontal(|ui| {
        let (w, gap) = (88.0, 20.0);
        ui.add_space((ui.available_width() - 2.0 * w - gap - 8.0).max(0.0));
        ui.spacing_mut().item_spacing.x = gap;
        for (k, label) in ["OK", "Cancel"].iter().enumerate() {
            if ui.add_sized([w, 30.0], egui::Button::new(*label)).clicked() {
                choice = Some(k);
            }
        }
    });
    match choice {
        Some(0) => {
            app.prefs_backup = None;
            true
        }
        Some(_) => {
            cancel_map_settings(app);
            true
        }
        None => false,
    }
}

/// Undoes the changes made in Map Display Settings.
fn cancel_map_settings(app: &mut EpanetApp) {
    if let Some(old) = app.prefs_backup.take() {
        app.prefs = old;
    }
}

fn map_settings_page(app: &mut EpanetApp, ui: &mut Ui, page: usize) {
    let p = &mut app.prefs;
    ui.spacing_mut().item_spacing.y = 16.0;
    let row = |ui: &mut Ui, label: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(110.0, 24.0), Sense::hover());
            ui.painter().text(
                r.left_center(),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(14.0),
                ui.visuals().text_color(),
            );
            add(ui);
        })
    };
    // a small red sample of the node or link size, as EPANET-UI shows
    let sample = |ui: &mut Ui, node: bool, size: f32| {
        ui.add_space(24.0);
        let (r, _) = ui.allocate_exact_size(vec2(60.0, 24.0), Sense::hover());
        let red = Color32::from_rgb(0xe0, 0x00, 0x00);
        if node {
            ui.painter()
                .circle(r.center(), size / 2.0 + 0.5, red, Stroke::NONE);
        } else {
            ui.painter()
                .hline(r.x_range(), r.center().y, Stroke::new(size, red));
        }
    };
    match page {
        0 => {
            row(ui, "Size", &mut |ui| {
                spin(ui, &mut p.node_size, 1.0..=10.0);
                sample(ui, true, p.node_size);
            });
            crate::widgets::checkbox(ui, &mut p.node_proportional, "Proportional To Value")
                .on_hover_text("Larger nodes for higher legend intervals");
            crate::widgets::checkbox(ui, &mut p.node_border, "Display Border");
        }
        1 => {
            row(ui, "Size", &mut |ui| {
                spin(ui, &mut p.link_width, 1.0..=10.0);
                sample(ui, false, p.link_width);
            });
            crate::widgets::checkbox(ui, &mut p.link_proportional, "Proportional To Value")
                .on_hover_text("Wider links for higher legend intervals");
            crate::widgets::checkbox(ui, &mut p.link_border, "Display Border");
        }
        2 => {
            ui.spacing_mut().item_spacing.y = 12.0;
            crate::widgets::checkbox(ui, &mut p.show_node_ids, "Show node IDs");
            crate::widgets::checkbox(ui, &mut p.show_node_values, "Show node values");
            crate::widgets::checkbox(ui, &mut p.show_link_ids, "Show link IDs");
            crate::widgets::checkbox(ui, &mut p.show_link_values, "Show link values");
            crate::widgets::checkbox(ui, &mut p.opaque_labels, "Use opaque text");
            row(ui, "Font size", &mut |ui| {
                spin(ui, &mut p.label_font, 6.0..=24.0)
            });
            row(ui, "Zoom level", &mut |ui| {
                spin(ui, &mut p.label_zoom, 0..=10)
            })
            .response
            .on_hover_text("Zoom In steps past the full extent before labels appear");
        }
        3 => {
            crate::widgets::checkbox(ui, &mut p.show_arrows, "Show direction arrows");
            row(ui, "Arrow size", &mut |ui| {
                spin(ui, &mut p.arrow_size, 2.0..=12.0)
            });
            row(ui, "Zoom level", &mut |ui| {
                spin(ui, &mut p.arrow_zoom, 0..=10)
            })
            .response
            .on_hover_text("Zoom In steps past the full extent before arrows appear");
        }
        _ => {
            ui.spacing_mut().item_spacing.y = 6.0;
            for bg in crate::style::MapBackground::ALL {
                let (r, resp) = ui.allocate_exact_size(vec2(160.0, 32.0), Sense::click());
                let selected = p.map_background == bg;
                if selected || resp.hovered() {
                    ui.painter().rect_filled(
                        r,
                        2.0,
                        if selected {
                            ui.visuals().selection.bg_fill.gamma_multiply(0.5)
                        } else {
                            ui.visuals().widgets.hovered.bg_fill
                        },
                    );
                }
                let swatch = Rect::from_min_size(r.left_top() + vec2(2.0, 2.0), vec2(28.0, 28.0));
                ui.painter().rect(
                    swatch,
                    0.0,
                    bg.color(),
                    Stroke::new(1.0, Color32::from_gray(110)),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    swatch.right_center() + vec2(6.0, 0.0),
                    Align2::LEFT_CENTER,
                    bg.label(),
                    FontId::proportional(14.0),
                    ui.visuals().text_color(),
                );
                if resp.clicked() {
                    p.map_background = bg;
                }
            }
        }
    }
}

/// A Windows-style spin box: a number field with up and down arrows.
fn spin<N: egui::emath::Numeric>(ui: &mut Ui, value: &mut N, range: std::ops::RangeInclusive<N>) {
    let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.add_sized(
        [62.0, 24.0],
        egui::DragValue::new(value)
            .range(range)
            .speed(0.1)
            .max_decimals(0),
    );
    let (r, _) = ui.allocate_exact_size(vec2(18.0, 24.0), Sense::hover());
    let color = ui.visuals().text_color();
    for (k, step) in [(0, 1.0), (1, -1.0)] {
        let half = Rect::from_min_size(r.min + vec2(0.0, 12.0 * k as f32), vec2(18.0, 12.0));
        let resp = ui.interact(half, ui.id().with(("spin", k)), Sense::click());
        ui.painter().rect(
            half.shrink(0.5),
            0.0,
            if resp.hovered() {
                ui.visuals().widgets.hovered.bg_fill
            } else {
                ui.visuals().widgets.inactive.bg_fill
            },
            Stroke::new(1.0, Color32::from_gray(150)),
            egui::StrokeKind::Inside,
        );
        let c = half.center();
        let d = if step > 0.0 { -1.5 } else { 1.5 };
        ui.painter().add(egui::Shape::convex_polygon(
            vec![c + vec2(0.0, d), c + vec2(-3.5, -d), c + vec2(3.5, -d)],
            color,
            Stroke::NONE,
        ));
        if resp.clicked() {
            *value = N::from_f64((value.to_f64().round() + step).clamp(lo, hi));
        }
    }
}

/// The Legend Editor's working copy (EPANET-UI's `legendeditor`), applied by OK.
#[derive(Clone, Debug, Default)]
pub struct LegendEdit {
    breaks: [String; 4],
    colors: [Color32; 5],
    framed: bool,
    /// 0 the legend, 1 Interval Scale, 2 Color Palette.
    page: u8,
    min: String,
    max: String,
    palette: usize,
    error: Option<String>,
}

/// Opens the Legend Editor for the node (or link) theme.
pub fn open_legend_editor(app: &mut EpanetApp, nodes: bool) {
    let (legend, framed) = if nodes {
        (app.node_legend(), app.prefs.node_legend_framed)
    } else {
        (app.link_legend(), app.prefs.link_legend_framed)
    };
    app.legend_edit = LegendEdit {
        breaks: legend.breaks.map(crate::style::fmt_break),
        colors: legend.colors,
        framed,
        ..Default::default()
    };
    app.dialog = if nodes {
        Dialog::NodeLegend
    } else {
        Dialog::LinkLegend
    };
}

/// The range of the theme's current values, as EPANET-UI's Interval Scale
/// offers it: tanks and reservoirs left out, flows and demands by magnitude.
fn theme_range(app: &EpanetApp, nodes: bool) -> Option<(f64, f64)> {
    use crate::style::{LinkTheme, NodeTheme};
    let values: Vec<f64> = if nodes {
        let t = app.prefs.node_theme;
        (0..app.net.nodes.len())
            .filter(|&i| app.net.nodes[i].kind == NodeKind::Junction)
            .filter_map(|i| app.node_theme_value(i))
            .map(|v| {
                let v = v as f64;
                match t {
                    NodeTheme::BaseDemand | NodeTheme::Demand => v.abs(),
                    NodeTheme::Pressure => v.max(0.01),
                    _ => v,
                }
            })
            .collect()
    } else {
        let t = app.prefs.link_theme;
        (0..app.net.links.len())
            .filter_map(|i| app.link_theme_value(i))
            .map(|v| {
                let v = v as f64;
                match t {
                    LinkTheme::Flow | LinkTheme::Velocity | LinkTheme::Headloss => v.abs(),
                    _ => v,
                }
            })
            .collect()
    };
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    lo.is_finite().then_some((lo, hi))
}

/// A column of five color boxes; returns the one clicked.
fn color_bar(ui: &mut Ui, colors: &[Color32; 5], size: egui::Vec2) -> (Rect, Option<usize>) {
    let (rect, _) = ui.allocate_exact_size(vec2(size.x, size.y * 5.0), Sense::hover());
    let mut clicked = None;
    for (k, c) in colors.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(0.0, size.y * k as f32), size);
        let resp = ui.interact(r, ui.id().with(("legend_color", k)), Sense::click());
        ui.painter().rect(
            r,
            0.0,
            *c,
            Stroke::new(1.0, Color32::from_gray(60)),
            egui::StrokeKind::Inside,
        );
        if resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            clicked = Some(k);
        }
    }
    (rect, clicked)
}

/// EPANET-UI's Legend Editor: the colors on the left (click one to change
/// it), the four interval boundaries beside them, and buttons for an
/// automatic Interval Scale, a Color Palette, reversing the colors and
/// framing the legend.
fn legend_editor(app: &mut EpanetApp, ui: &mut Ui, nodes: bool) -> bool {
    let ctx = ui.ctx().clone();
    let title = if nodes {
        app.node_theme_name(app.prefs.node_theme)
    } else {
        app.link_theme_name(app.prefs.link_theme)
    };
    let range = theme_range(app, nodes);
    let red = Color32::from_rgb(0xc0, 0x39, 0x2b);
    let e = &mut app.legend_edit;
    let mut close = false;
    let mut apply = false;
    match e.page {
        // Interval Scale
        1 => {
            ui.set_min_width(300.0);
            ui.label(RichText::new(title).size(14.0));
            ui.add_space(10.0);
            egui::Grid::new("legend_scale")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Minimum");
                    ui.add(egui::TextEdit::singleline(&mut e.min).desired_width(110.0));
                    ui.end_row();
                    ui.label("Maximum");
                    ui.add(egui::TextEdit::singleline(&mut e.max).desired_width(110.0));
                    ui.end_row();
                });
            if let Some(err) = &e.error {
                ui.label(RichText::new(err).color(red));
            }
            ui.add_space(12.0);
            match buttons(ui, &["Apply", "Cancel"]) {
                Some(0) => match (e.min.trim().parse::<f64>(), e.max.trim().parse::<f64>()) {
                    (Ok(lo), Ok(hi)) if lo < hi => {
                        e.breaks =
                            crate::style::interval_scale(lo, hi).map(crate::style::fmt_break);
                        e.error = None;
                        e.page = 0;
                    }
                    _ => e.error = Some("Enter a minimum below the maximum".into()),
                },
                Some(_) => {
                    e.error = None;
                    e.page = 0;
                }
                None => {}
            }
        }
        // Color Palette
        2 => {
            ui.set_min_width(300.0);
            for (k, (name, colors)) in crate::style::PALETTES.iter().enumerate() {
                ui.horizontal(|ui| {
                    crate::widgets::radio_value(ui, &mut e.palette, k, "");
                    for c in colors {
                        let (r, resp) = ui.allocate_exact_size(vec2(26.0, 18.0), Sense::click());
                        ui.painter().rect(
                            r,
                            0.0,
                            *c,
                            Stroke::new(1.0, Color32::from_gray(90)),
                            egui::StrokeKind::Inside,
                        );
                        if resp.clicked() {
                            e.palette = k;
                        }
                    }
                    ui.label(RichText::new(*name).weak());
                });
            }
            ui.add_space(10.0);
            match buttons(ui, &["OK", "Cancel"]) {
                Some(0) => {
                    e.colors = crate::style::PALETTES[e.palette].1;
                    e.page = 0;
                }
                Some(_) => e.page = 0,
                None => {}
            }
        }
        // The legend
        _ => {
            ui.set_min_width(380.0);
            let box_size = vec2(18.0, 34.0);
            let mut picked = None;
            ui.horizontal_top(|ui| {
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.add_space(28.0);
                    let (_, clicked) = color_bar(ui, &e.colors, box_size);
                    picked = clicked;
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(14.0));
                    // each boundary sits beside the line between two colors
                    ui.add_space(box_size.y - 12.0 - 6.0);
                    for b in e.breaks.iter_mut() {
                        ui.add_sized([110.0, 24.0], egui::TextEdit::singleline(b));
                        ui.add_space(box_size.y - 24.0 - ui.spacing().item_spacing.y);
                    }
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.add_space(22.0);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    let w = [130.0, 28.0];
                    if ui
                        .add_enabled(range.is_some(), egui::Button::new("Interval Scale"))
                        .on_disabled_hover_text("The theme has no values (run an analysis first)")
                        .clicked()
                        && let Some((lo, hi)) = range
                    {
                        e.min = crate::style::fmt_break(lo as f32);
                        e.max = crate::style::fmt_break(hi as f32);
                        e.error = None;
                        e.page = 1;
                    }
                    if ui
                        .add_sized(w, egui::Button::new("Color Palette"))
                        .clicked()
                    {
                        e.page = 2;
                    }
                    if ui
                        .add_sized(w, egui::Button::new("Reverse Colors"))
                        .clicked()
                    {
                        e.colors.reverse();
                    }
                    crate::widgets::checkbox(ui, &mut e.framed, "Framed");
                });
            });
            // Clicking a color opens a color picker for it.
            let pick_id = egui::Id::new("legend_pick");
            if let Some(k) = picked {
                ctx.data_mut(|d| d.insert_temp(pick_id, k));
            }
            if let Some(k) = ctx.data(|d| d.get_temp::<usize>(pick_id)) {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    egui::widgets::color_picker::color_picker_color32(
                        ui,
                        &mut e.colors[k],
                        egui::widgets::color_picker::Alpha::Opaque,
                    );
                    if ui.button("Done").clicked() {
                        ctx.data_mut(|d| d.remove::<usize>(pick_id));
                    }
                });
            }
            ui.add_space(10.0);
            ui.label("Click on a color to change it.");
            if let Some(err) = &e.error {
                ui.label(RichText::new(err).color(red));
            }
            ui.add_space(10.0);
            match buttons(ui, &["Help", "OK", "Cancel"]) {
                Some(0) => crate::help::open(&ctx, "Map Legend"),
                Some(1) => apply = true,
                Some(_) => close = true,
                None => {}
            }
            if close || apply {
                ctx.data_mut(|d| d.remove::<usize>(pick_id));
            }
        }
    }
    if apply {
        // EPANET-UI: numbers, each no smaller than the one before
        let parsed: Option<Vec<f32>> = e.breaks.iter().map(|s| s.trim().parse().ok()).collect();
        match parsed {
            None => {
                e.error = Some("The intervals must be numbers".into());
                return false;
            }
            Some(v) if v.windows(2).any(|w| w[1] < w[0]) => {
                e.error = Some("The intervals are out of order".into());
                return false;
            }
            Some(v) => {
                let breaks = [v[0], v[1], v[2], v[3]];
                let colors = e.colors.map(crate::app::rgb);
                let framed = e.framed;
                let p = &mut app.prefs;
                if nodes {
                    p.node_legends.insert(p.node_theme, Legend::new(breaks));
                    p.node_colors = colors;
                    p.node_legend_framed = framed;
                } else {
                    p.link_legends.insert(p.link_theme, Legend::new(breaks));
                    p.link_colors = colors;
                    p.link_legend_framed = framed;
                }
                close = true;
            }
        }
    }
    close
}

fn units(app: &EpanetApp, ui: &mut Ui) -> bool {
    let si = app.net.is_si();
    ui.label(format!(
        "This project uses {} units (flow in {}).",
        if si { "SI (metric)" } else { "US customary" },
        app.net.flow_units()
    ));
    ui.add_space(4.0);
    egui::Grid::new("units")
        .striped(true)
        .num_columns(3)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.strong("Parameter");
            ui.strong("US Customary");
            ui.strong("SI Metric");
            ui.end_row();
            for (p, us, m) in [
                ("Concentration", "mg/L or ug/L", "mg/L or ug/L"),
                ("Demand", "flow units", "flow units"),
                ("Diameter (pipes)", "inches", "millimeters"),
                ("Diameter (tanks)", "feet", "meters"),
                ("Efficiency", "percent", "percent"),
                ("Elevation", "feet", "meters"),
                ("Emitter coefficient", "flow / (psi)^½", "flow / (meters)^½"),
                ("Head", "feet", "meters"),
                ("Length", "feet", "meters"),
                ("Minor loss coeff.", "unitless", "unitless"),
                ("Power", "horsepower", "kilowatts"),
                ("Pressure", "psi", "meters"),
                ("Roughness (D-W)", "10⁻³ feet", "millimeters"),
                ("Velocity", "ft/s", "m/s"),
                ("Volume", "cubic feet", "cubic meters"),
                ("Water age", "hours", "hours"),
            ] {
                ui.label(p);
                ui.label(us);
                ui.label(m);
                ui.end_row();
            }
        });
    false
}

/// EPANET's error and warning codes, with a search box: a number finds
/// codes starting with it, other text searches the messages.
fn error_codes(ui: &mut Ui) -> bool {
    const ERRORS: &str = include_str!("../../../vendor/epanet/src/errors.dat");
    let id = egui::Id::new("error_codes_search");
    let mut search: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.add(
            egui::TextEdit::singleline(&mut search)
                .hint_text("Error number, e.g. 200")
                .desired_width(220.0),
        )
        .request_focus();
        if !search.is_empty() && ui.small_button("✖").on_hover_text("Clear").clicked() {
            search.clear();
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, search.clone()));
    let query = search.trim().to_lowercase();
    let matches = |code: &str, msg: &str| {
        query.is_empty()
            || if query.chars().all(|c| c.is_ascii_digit()) {
                code.starts_with(&query)
            } else {
                msg.to_lowercase().contains(&query)
            }
    };
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .show(ui, |ui| {
            egui::Grid::new("errors")
                .striped(true)
                .num_columns(2)
                .show(ui, |ui| {
                    for line in ERRORS.lines().filter(|l| l.starts_with("DAT(")) {
                        let inner = line.trim_start_matches("DAT(").trim_end_matches(')');
                        if let Some((code, msg)) = inner.split_once(',') {
                            let msg = msg.trim().trim_matches('"');
                            if code == "0" || !matches(code, msg) {
                                continue;
                            }
                            ui.monospace(code);
                            ui.label(msg);
                            ui.end_row();
                        }
                    }
                });
        });
    false
}

/// Tabs drawn like EPANET-UI's page control; returns the selected index.
fn page_tabs(ui: &mut Ui, id: &str, names: &[&str]) -> usize {
    let key = egui::Id::new(("page_tabs", id));
    let mut tab: usize = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (k, n) in names.iter().enumerate() {
            let font = egui::FontId::proportional(13.0);
            let w = ui.fonts_mut(|f| {
                f.layout_no_wrap(n.to_string(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            }) + 18.0;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
            let selected = k == tab;
            let v = ui.visuals();
            let fill = if selected {
                if v.dark_mode {
                    v.extreme_bg_color
                } else {
                    egui::Color32::WHITE
                }
            } else if resp.hovered() {
                v.widgets.hovered.weak_bg_fill
            } else {
                v.panel_fill
            };
            let r = if selected {
                rect
            } else {
                rect.shrink2(egui::vec2(0.0, 1.0))
                    .translate(egui::vec2(0.0, 1.0))
            };
            ui.painter().rect(
                r,
                egui::CornerRadius {
                    nw: 3,
                    ne: 3,
                    sw: 0,
                    se: 0,
                },
                fill,
                egui::Stroke::new(1.0, egui::Color32::from_gray(190)),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                *n,
                font,
                v.text_color(),
            );
            if resp.clicked() {
                tab = k;
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, tab));
    tab
}

const MIT_LICENSE: [&str; 4] = [
    "Copyright (c) 2026, the Authors",
    "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:",
    "The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.",
    "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.",
];

fn about(ui: &mut Ui) -> bool {
    use crate::images;
    let blue = egui::Color32::from_rgb(0x00, 0x00, 0xe6);
    ui.add_space(8.0);
    let tab = page_tabs(ui, "about", &["About", "Terms of Use", "Acknowledgements"]);
    ui.add_space(-1.0);
    panel(ui, |ui| {
        ui.set_min_height(330.0);
        // Reserve a scrollbar gutter at the right edge of the full panel width.
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        ui.spacing_mut().scroll.bar_width = 8.0;
        match tab {
            0 => {
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let font = crate::fonts::bold(30.0);
                        let tw = ui.fonts_mut(|f| {
                            f.layout_no_wrap("W E P A N E T".into(), font.clone(), blue)
                                .size()
                                .x
                        });
                        ui.add_space(((ui.available_width() - tw - 44.0) / 2.0).max(0.0));
                        images::show(ui, images::EPANET_UI, 32.0);
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("W E P A N E T")
                                .font(font)
                                .italics()
                                .color(blue),
                        );
                    });
                    ui.add_space(10.0);
                    ui.label(format!("WEPANET Version {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(4.0);
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let v = eui_engine::engine_version();
                        ui.label(format!(
                            "OWA-EPANET Version {}.{}.{}",
                            v / 10000,
                            (v / 100) % 100,
                            v % 100
                        ));
                    }
                    #[cfg(target_arch = "wasm32")]
                    ui.label("OWA-EPANET Version 2.3 (WebAssembly)");
                    ui.add_space(12.0);
                    ui.label("A graphical user interface for the Open Water Analytics");
                    ui.label("version of the EPANET water distribution system simulator.");
                    ui.add_space(12.0);
                    ui.label("Adapted from the EPANET-UI desktop application");
                    ui.label("by Lewis A. Rossman, preserving its interface design and workflows.");
                    ui.hyperlink_to(
                        "Original EPANET-UI project",
                        "https://github.com/OpenWaterAnalytics/EPANET-UI",
                    );
                });
            }
            1 => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(330.0)
                    .show(ui, |ui| {
                        ui.label("MIT License");
                        for p in MIT_LICENSE {
                            ui.add_space(6.0);
                            ui.label(p);
                        }
                    });
            }
            _ => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(330.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "WEPANET builds on the following projects and resources:",
                            )
                            .font(crate::fonts::bold(13.0)),
                        );
                        ui.add_space(6.0);
                        let item = |ui: &mut Ui, name: &str, url: &str, lines: &[&str]| {
                            ui.hyperlink_to(name, url);
                            for l in lines {
                                ui.label(*l);
                            }
                            ui.add_space(8.0);
                        };
                        item(
                            ui,
                            "EPANET-UI desktop application",
                            "https://github.com/OpenWaterAnalytics/EPANET-UI",
                            &[
                                "Desktop application by Lewis A. Rossman",
                                "Interface design, workflows, reports, icon and welcome image",
                                "MIT license",
                                "Copyright (c) 2026 by the Authors",
                            ],
                        );
                        item(
                            ui,
                            "OWA-EPANET 2.3",
                            "https://github.com/OpenWaterAnalytics/EPANET",
                            &["MIT license", "Copyright (c) 2019 by the Authors"],
                        );
                        item(
                            ui,
                            "egui, eframe and egui_plot",
                            "https://github.com/emilk/egui",
                            &["MIT or Apache-2.0 license"],
                        );
                        item(
                            ui,
                            "Icons by icons8",
                            "https://icons8.com",
                            &["Toolbar and menu icons, as in EPANET-UI"],
                        );
                        item(
                            ui,
                            "Selawik font",
                            "https://github.com/microsoft/Selawik",
                            &[
                                "SIL Open Font License 1.1",
                                "Copyright (c) 2015 Microsoft Corporation",
                            ],
                        );
                        item(
                            ui,
                            "WaterBenchmarkHub",
                            "https://github.com/WaterFutures/WaterBenchmarkHub",
                            &["Benchmark networks catalog of the WaterFutures project"],
                        );
                    });
            }
        }
    });
    ui.add_space(6.0);
    false
}

/// Dark blue used for the Welcome Page headings.
const HEADING: egui::Color32 = egui::Color32::from_rgb(0x00, 0x00, 0x8b);
const HEADING_LIGHT: egui::Color32 = egui::Color32::from_rgb(0x4a, 0x8f, 0xe0);

/// EPANET-UI's welcome photo of blue pipes, with the title drawn over it.
fn welcome_banner(ui: &mut Ui) {
    use egui::{Align2, Color32, vec2};
    let img = crate::images::WELCOME;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        vec2(width, width * img.size[1] as f32 / img.size[0] as f32),
        egui::Sense::hover(),
    );
    crate::images::paint(ui, img, rect);
    let font = crate::fonts::bold(36.0);
    let title = "WELCOME TO WEPANET";
    ui.painter().text(
        rect.center() + vec2(2.0, 2.0),
        Align2::CENTER_CENTER,
        title,
        font.clone(),
        Color32::from_black_alpha(140),
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        title,
        font,
        Color32::WHITE,
    );
    // the version, in the bottom right corner
    let version = format!("WEPANET v{}", env!("CARGO_PKG_VERSION"));
    let at = rect.right_bottom() + vec2(-14.0, -10.0);
    let small = crate::fonts::bold(14.0);
    ui.painter().text(
        at + vec2(1.0, 1.0),
        Align2::RIGHT_BOTTOM,
        &version,
        small.clone(),
        Color32::from_black_alpha(140),
    );
    ui.painter()
        .text(at, Align2::RIGHT_BOTTOM, version, small, Color32::WHITE);
}
