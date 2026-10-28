//! Dialog windows: Welcome Page, Project Summary, Map Display Settings, legend
//! editors, Units, Error Codes, About, Run Status, Program Preferences and
//! Confirm Deletion.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use eui_core::props::Element;
use eui_core::{LinkKind, NodeKind};

use crate::app::{Dialog, EpanetApp};
use crate::style::Legend;

pub fn show(app: &mut EpanetApp, ctx: &egui::Context) {
    crate::basemap::project_crs_window(app, ctx);
    let dialog = app.dialog;
    // Dialogs can be moved (dragged by any free part of them). Each opens in
    // the middle of the screen and stays there while the screen changes size
    // (in the browser the first frames do not know it yet), until moved.
    let shown = egui::Id::new("dialog_shown");
    // (the dialog, the screen center it was put at, whether it was moved)
    let state: Option<(Dialog, egui::Pos2, bool)> = ctx.data(|d| d.get_temp(shown));
    let opened = state.map(|s| s.0) != Some(dialog);
    let center = ctx.content_rect().center();
    let recenter = match state {
        Some((d, c, moved)) if d == dialog => !moved && c != center,
        _ => true,
    };
    if recenter {
        ctx.data_mut(|d| d.insert_temp(shown, (dialog, center, false)));
    }
    // A dialog with Cancel was left some other way (another dialog took its place).
    if !matches!(dialog, Dialog::MapSettings | Dialog::Preferences) {
        app.prefs_backup = None;
    }
    if dialog == Dialog::ConfirmReload {
        // F5 / Ctrl+R in the browser with changes not saved
        egui::Modal::new(egui::Id::new("reload_page")).show(ctx, |ui| {
            use crate::i18n::tr;
            ui.set_width(360.0);
            ui.label(RichText::new(tr("Reload the page?")).font(crate::fonts::bold(14.0)));
            ui.add_space(8.0);
            ui.label(tr(
                "The project has changes that are not saved: reloading the page loses them.",
            ));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Save Project")).clicked() {
                    app.dialog = Dialog::None;
                    app.save_project();
                }
                if ui.button(tr("Reload")).clicked() {
                    crate::io::reload_page();
                }
                if ui.button(tr("Cancel")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    app.dialog = Dialog::None;
                }
            });
        });
        return;
    }
    if dialog == Dialog::ConfirmReplace {
        egui::Modal::new(egui::Id::new("unsaved_project")).show(ctx, |ui| {
            use crate::i18n::tr;
            ui.set_width(340.0);
            ui.label(
                RichText::new(tr("Save changes before opening another project?"))
                    .font(crate::fonts::bold(14.0)),
            );
            ui.add_space(8.0);
            ui.label(tr("Your current project has unsaved changes."));
            ui.add_space(12.0);
            if app.save_pending() {
                ui.spinner();
                ui.label(tr("Waiting for the save to finish…"));
            } else {
                ui.horizontal(|ui| {
                    for (i, label) in ["Save", "Discard", "Cancel"].iter().enumerate() {
                        if ui.button(tr(label)).clicked() {
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
        Dialog::MapSettings => "Map Display Settings",
        Dialog::NodeLegend => "Legend Editor",
        Dialog::LinkLegend => "Legend Editor",
        Dialog::Units => "Units of Measurement",
        Dialog::ErrorCodes => "Error Codes",
        Dialog::About => "About WEPANET",
        Dialog::RunStatus => "Run Status",
        Dialog::Preferences => "Program Preferences",
        Dialog::Basemap => "Select a Basemap Source",
        Dialog::AddLayer => "Add a Map Layer",
        Dialog::ImportShapefile => "Shapefile Importer",
        Dialog::ImportDxf => "DXF File Importer",
        Dialog::ImportCsv => "CSV File Importer",
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
        Dialog::ConfirmDelete => "Confirm Deletion",
        Dialog::ConfirmReplace => "Unsaved Changes",
        Dialog::ConfirmReload => "Unsaved Changes",
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
            | Dialog::ImportDxf
            | Dialog::ImportCsv
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
    // On small screens no window is wider than the screen.
    let screen = ctx.content_rect();
    let fit = (screen.width() - 16.0).max(200.0);
    // Maximized (with the button beside the close button, or a double click
    // on the title bar) until the dialog closes.
    let max_id = egui::Id::new(("dialog_maximized", title));
    if opened {
        ctx.data_mut(|d| d.remove::<bool>(max_id));
    }
    let maximized = ctx.data(|d| d.get_temp::<bool>(max_id)).unwrap_or(false);
    let resizable = !message;
    let width = match dialog {
        Dialog::Welcome => 735.0_f32,
        Dialog::RunStatus | Dialog::ConfirmDelete => 320.0,
        Dialog::About => 470.0,
        Dialog::Preferences => 330.0,
        Dialog::Benchmarks => 880.0,
        Dialog::PluginManager => 800.0,
        _ => 320.0,
    }
    .min(fit);
    let mut window = egui::Window::new(title)
        // the version makes egui forget sizes remembered by earlier builds
        // (the Basemap source window had grown wide); maximized, it is
        // another window, so the normal one keeps its place and size
        .id(egui::Id::new(("dialog", title, 2, maximized)))
        .title_bar(false)
        .collapsible(false)
        // resizable by the edges and corners, down to the size of its content
        // (at which it opens)
        .resizable(resizable)
        .min_width(width)
        .default_size([width, 40.0])
        .frame(egui::Frame::window(&style).fill(fill).inner_margin(0))
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(screen.center());
    if maximized {
        window = window
            .pivot(egui::Align2::LEFT_TOP)
            .fixed_rect(screen.shrink(4.0));
    } else if recenter {
        window = window.current_pos(screen.center());
    }
    let shown_window = window.show(ctx, |ui| {
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
        let mut body = |ui: &mut Ui| {
            egui::Frame::NONE.inner_margin(margin).show(ui, |ui| {
                let mut content = |ui: &mut Ui| match dialog {
                    Dialog::Welcome => welcome(app, ui),
                    Dialog::Summary => summary(app, ui),
                    Dialog::MapSettings => map_settings(app, ui),
                    Dialog::NodeLegend => legend_editor(app, ui, true),
                    Dialog::LinkLegend => legend_editor(app, ui, false),
                    Dialog::Units => units(app, ui),
                    Dialog::ErrorCodes => error_codes(ui),
                    Dialog::About => about(ui),
                    Dialog::ConfirmDelete => confirm_delete(app, ui),
                    Dialog::ConfirmReplace | Dialog::ConfirmReload => false,
                    Dialog::RunStatus => run_status(app, ui),
                    Dialog::Preferences => preferences(app, ui),
                    Dialog::Basemap => crate::basemap::dialog(app, ui),
                    Dialog::AddLayer => crate::layers::dialog(app, ui),
                    Dialog::ImportShapefile => crate::import_shp::dialog(app, ui),
                    Dialog::ImportDxf => crate::import_dxf::dialog(app, ui),
                    Dialog::ImportCsv => crate::import_csv::dialog(app, ui),
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
                    Dialog::None => true,
                };
                // A resized window is as tall as the user made it: the
                // content (its white panel, if any) fills the height left.
                let fill = |ui: &mut Ui| {
                    if resizable {
                        ui.allocate_space(vec2(0.0, ui.available_height().max(0.0)));
                    }
                };
                close = if own_panel || message {
                    let c = content(ui);
                    fill(ui);
                    c
                } else {
                    panel(ui, |ui| {
                        let c = content(ui);
                        fill(ui);
                        c
                    })
                };
            });
        };
        body(ui);
        let w = if resizable {
            ui.max_rect()
        } else {
            ui.min_rect()
        };
        let bar = Rect::from_min_max(pos2(w.left(), top), pos2(w.right(), top + TITLE_H));
        let clicked = title_bar(
            ui,
            bar,
            title,
            fill,
            resizable.then_some(maximized),
            closable,
        );
        if clicked.maximize {
            ctx.data_mut(|d| d.insert_temp(max_id, !maximized));
        }
        // (Escape is for the CRS selector while it is open over its dialog)
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape)) && !app.basemap.selector_open();
        if closable && (clicked.close || escape) {
            if dialog == Dialog::MapSettings {
                cancel_map_settings(app);
            }
            close = true;
        }
    });
    if shown_window.is_some_and(|w| w.response.dragged()) {
        ctx.data_mut(|d| d.insert_temp(shown, (dialog, center, true)));
    }
    if close && app.dialog == dialog {
        app.dialog = Dialog::None;
    }
}

/// Height of the dialog title bar.
pub(crate) const TITLE_H: f32 = 34.0;

/// What was clicked on a window's title bar.
#[derive(Default)]
pub(crate) struct TitleClicks {
    /// The maximize (or restore) button, or a double click on the title.
    pub maximize: bool,
    pub close: bool,
}

/// EPANET-UI's window chrome over `bar`: the title on the left and on the
/// right a maximize button (when `maximized` is given: whether the window
/// is maximized now) and a red close button.
pub(crate) fn title_bar(
    ui: &mut Ui,
    bar: Rect,
    title: &str,
    fill: Color32,
    maximized: Option<bool>,
    closable: bool,
) -> TitleClicks {
    let mut clicks = TitleClicks::default();
    ui.painter().text(
        bar.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr(title),
        FontId::proportional(14.5),
        ui.visuals().text_color(),
    );
    let button = |k: f32| {
        Rect::from_min_size(
            pos2(bar.right() - 46.0 - 40.0 * k, bar.top() + 4.0),
            vec2(40.0, TITLE_H - 8.0),
        )
    };
    if let Some(maximized) = maximized {
        // left of the close button
        let m = button(if closable { 1.0 } else { 0.0 });
        let resp = ui.interact(m, ui.id().with("title_maximize"), Sense::click());
        if resp.hovered() {
            ui.painter()
                .rect_filled(m, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let s = Stroke::new(1.2, ui.visuals().text_color());
        let c = m.center();
        if maximized {
            // two overlapping squares: restore
            let back = Rect::from_center_size(c + vec2(2.0, -2.0), vec2(9.0, 9.0));
            let front = Rect::from_center_size(c + vec2(-1.0, 1.0), vec2(9.0, 9.0));
            ui.painter()
                .rect_stroke(back, 0.0, s, egui::StrokeKind::Middle);
            ui.painter().rect_filled(front, 0.0, fill);
            ui.painter()
                .rect_stroke(front, 0.0, s, egui::StrokeKind::Middle);
        } else {
            let sq = Rect::from_center_size(c, vec2(10.0, 10.0));
            ui.painter()
                .rect_stroke(sq, 0.0, s, egui::StrokeKind::Middle);
        }
        // a double click on the title does the same
        let title_area = Rect::from_min_max(bar.min, pos2(m.left(), bar.bottom()));
        let title_click = ui.interact(title_area, ui.id().with("title_bar"), Sense::click());
        clicks.maximize = resp
            .on_hover_text(if maximized { "Restore" } else { "Maximize" })
            .clicked()
            || title_click.double_clicked();
    }
    if closable {
        let x = button(0.0);
        let resp = ui.interact(x, ui.id().with("title_close"), Sense::click());
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
        clicks.close = resp.on_hover_text("Close").clicked();
    }
    clicks
}

/// A Welcome Page link: a small bullet then the text, as in EPANET-UI.
fn welcome_link(ui: &mut Ui, text: &str, icon: crate::icons::Icon) -> bool {
    let text = crate::i18n::tr(text);
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
    // On a low screen the part under the banner scrolls; on a narrow one the
    // two columns go one under the other.
    let max_h = (ui.ctx().content_rect().height() - ui.min_rect().bottom() - 20.0).max(120.0);
    let narrow = ui.available_width() < 560.0;
    let heading =
        |ui: &mut Ui, t: &str| {
            ui.label(RichText::new(crate::i18n::tr(t)).size(24.0).color(
                if ui.visuals().dark_mode {
                    HEADING_LIGHT
                } else {
                    HEADING
                },
            ));
            ui.add_space(4.0);
        };
    // the Develop / Learn column (0) and the projects column (1)
    let column = |app: &mut EpanetApp, close: &mut bool, ui: &mut Ui, k: usize| {
        if k == 0 {
            heading(ui, "Develop");
            if welcome_link(ui, "Create a new project", Icon::Junction) {
                app.new_project();
                *close = true;
            }
            ui.add_space(10.0);
            if welcome_link(ui, "Open an existing project", Icon::Junction) {
                app.open(&ctx);
                *close = true;
            }
            ui.add_space(34.0);
            heading(ui, "Learn");
            if welcome_link(ui, "Basic Tutorial", Icon::Junction) {
                crate::help::open_doc(&ctx, crate::help::Doc::Tutorial, None);
            }
            ui.add_space(10.0);
            if welcome_link(ui, "User Guide", Icon::Junction) {
                crate::help::open_doc(&ctx, crate::help::Doc::Manual, None);
            }
            ui.add_space(10.0);
            if welcome_link(ui, "Report an issue or suggest a feature", Icon::Junction) {
                crate::help::report_issue(&ctx);
            }
        } else {
            heading(ui, "Open a Recent Project");
            for r in app.prefs.recent.clone() {
                let name = std::path::Path::new(&r)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(r.clone());
                if welcome_link(ui, &name, Icon::Open) {
                    app.open_recent(&r);
                    *close = true;
                }
            }
            if app.prefs.recent.is_empty() {
                ui.label(RichText::new(crate::i18n::tr("There are no recent projects")).weak());
            }
            ui.add_space(16.0);
            ui.label(
                RichText::new(crate::i18n::tr("Sample projects"))
                    .size(14.0)
                    .strong(),
            );
            for (name, text) in eui_core::examples::ALL {
                if welcome_link(ui, &format!("{name}.inp"), Icon::Open) {
                    app.load_example(name, text);
                    *close = true;
                }
            }
        }
    };
    let (outer, inner) = if narrow { (12, 20) } else { (24, 46) };
    egui::ScrollArea::vertical()
        .max_height(max_h)
        // as tall as the screen allows when it scrolls (the window would
        // otherwise keep the height it had on a smaller screen)
        .min_scrolled_height(max_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
    egui::Frame::NONE
        .inner_margin(egui::Margin { left: outer, right: outer.min(20), top: 24, bottom: 22 })
        .show(ui, |ui| {
            egui::Frame::NONE
                .fill(if ui.visuals().dark_mode {
                    ui.visuals().extreme_bg_color
                } else {
                    egui::Color32::WHITE
                })
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(200)))
                .inner_margin(egui::Margin { left: inner, right: 20, top: 22, bottom: 18 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if narrow {
                        column(app, &mut close, ui, 0);
                        ui.add_space(24.0);
                        column(app, &mut close, ui, 1);
                    } else {
                        ui.columns(2, |cols| {
                            column(app, &mut close, &mut cols[0], 0);
                            column(app, &mut close, &mut cols[1], 1);
                        });
                    }
                    ui.add_space(18.0);
                    ui.separator();
                    ui.add_space(6.0);
                    heading(ui, "WaterBenchmarkHub");
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(crate::i18n::tr(
                                "Benchmark water distribution networks collected by the WaterFutures project.",
                            ))
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
                        RichText::new(crate::i18n::tr("Always show at startup")).size(14.0),
                    );
                });
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
    let many = !app.multi.is_empty();
    let question = if many {
        let (n, l) = app.multi.counts();
        crate::i18n::trf(
            "Delete the {} nodes (with their links) and {} links selected?",
            &[&n, &l],
        )
    } else {
        let what = match app.selection {
            Some(Element::Node(i)) => {
                format!("{} {}", app.net.nodes[i].kind.label(), app.net.nodes[i].id)
            }
            Some(Element::Link(i)) => {
                format!("{} {}", app.net.links[i].kind.label(), app.net.links[i].id)
            }
            None => return true,
        };
        format!("Delete {what}?")
    };
    ui.add_space(14.0);
    ui.vertical_centered(|ui| ui.label(RichText::new(question).size(14.0)));
    ui.add_space(24.0);
    let b = buttons(ui, &["Yes", "No"]);
    ui.add_space(6.0);
    if b == Some(0) || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if many {
            crate::multi::delete(app);
        } else {
            app.delete_selection();
        }
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
            ui.label(crate::i18n::tr("Decimal Separator"));
            ui.add_space(30.0);
            let sep = &mut app.prefs.decimal_separator;
            egui::ComboBox::from_id_salt("prefs_decimal_separator")
                .selected_text(crate::i18n::tr(sep.label()))
                .show_ui(ui, |ui| {
                    for s in crate::style::DecimalSeparator::ALL {
                        ui.selectable_value(sep, s, crate::i18n::tr(s.label()));
                    }
                });
        })
        .response
        .on_hover_text(crate::i18n::tr(
            "Numbers may be typed with either a point or a comma; files always use a point",
        ));
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
                #[cfg(not(target_arch = "wasm32"))]
                app.prefs.recent.clear();
                #[cfg(target_arch = "wasm32")]
                crate::recent::clear(app);
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
                Some(0) => match (eui_core::num::parse(&e.min), eui_core::num::parse(&e.max)) {
                    (Some(lo), Some(hi)) if lo < hi => {
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
        let parsed: Option<Vec<f32>> = e
            .breaks
            .iter()
            .map(|s| eui_core::num::parse(s).map(|v| v as f32))
            .collect();
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
    // smaller when the title (in some languages longer) would not fit
    let title = crate::i18n::tr("WELCOME TO WEPANET");
    let full = ui.fonts_mut(|f| {
        f.layout_no_wrap(title.to_string(), crate::fonts::bold(36.0), Color32::WHITE)
            .size()
            .x
    });
    let font = crate::fonts::bold(36.0 * ((width - 60.0) / full.max(1.0)).min(1.0));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a frame of the dialogs on a screen of `size`, with `events`;
    /// returns where each piece of text can be seen.
    fn frame(
        app: &mut EpanetApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<(String, Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            // a 60 Hz clock, for double clicks
            time: Some(ctx.input(|i| i.time) + 1.0 / 60.0),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| show(app, ui.ctx()));
        out.textures_delta.clear();
        out.shapes
            .iter()
            .filter_map(|c| match &c.shape {
                // the visible part of the text
                egui::Shape::Text(t) => Some((
                    t.galley.text().to_string(),
                    t.visual_bounding_rect().intersect(c.clip_rect),
                ))
                .filter(|(_, r)| r.is_positive()),
                _ => None,
            })
            .collect()
    }

    fn find<'a>(texts: &'a [(String, Rect)], text: &str) -> &'a Rect {
        &texts
            .iter()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("{text} not drawn"))
            .1
    }

    /// The Welcome Page is in the interface language.
    #[test]
    fn welcome_is_translated() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.dialog = Dialog::Welcome;
        let el = crate::i18n::all().iter().find(|l| l.code == "el").unwrap();
        for english in [
            "Develop",
            "Create a new project",
            "Learn",
            "Open a Recent Project",
            "Sample projects",
            "Always show at startup",
            "WELCOME TO WEPANET",
        ] {
            assert_ne!(el.text(english), english, "{english} has no Greek");
        }
        // the page's texts all go through the language table (only checked
        // in English here: switching the language would change the other
        // tests' text)
        let mut texts = Vec::new();
        for _ in 0..4 {
            texts = frame(&mut app, &ctx, vec2(1400.0, 900.0), Vec::new());
        }
        find(&texts, "Develop");
        find(&texts, "WELCOME TO WEPANET");
    }
    #[test]
    fn welcome_fits_a_small_screen() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.dialog = Dialog::Welcome;
        let size = vec2(480.0, 520.0);
        let mut texts = Vec::new();
        for _ in 0..4 {
            texts = frame(&mut app, &ctx, size, Vec::new());
        }
        // all on the screen (what is lower scrolls)
        let screen = Rect::from_min_size(egui::Pos2::ZERO, size);
        find(&texts, "Develop");
        for (t, r) in &texts {
            assert!(screen.expand(1.0).contains_rect(*r), "{t} at {r:?}");
        }
        // narrow but tall: one column, the projects under Develop
        let size = vec2(480.0, 1200.0);
        for _ in 0..4 {
            texts = frame(&mut app, &ctx, size, Vec::new());
        }
        assert!(find(&texts, "Open a Recent Project").top() > find(&texts, "Develop").bottom());
    }

    /// Program Preferences fits the Decimal Separator choice above its buttons.
    #[test]
    fn preferences_show_the_decimal_separator() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.dialog = Dialog::Preferences;
        let mut texts = Vec::new();
        for _ in 0..4 {
            texts = frame(&mut app, &ctx, vec2(1400.0, 900.0), Vec::new());
        }
        let window = window_rect(&ctx, "Program Preferences", false);
        let sep = *find(&texts, "Decimal Separator");
        let ok = *find(&texts, "OK");
        assert!(window.contains_rect(sep) && window.contains_rect(ok));
        assert!(sep.bottom() < ok.top());
    }

    /// Every page of the DXF and CSV importers fits its window, buttons included.
    #[test]
    fn importer_pages_fit_their_windows() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        let size = vec2(1400.0, 900.0);
        for (title, pages) in [("DXF File Importer", 3), ("CSV File Importer", 3)] {
            for page in 0..pages {
                if title.starts_with("DXF") {
                    crate::import_dxf::open(&mut app);
                    crate::import_dxf::tests::set_tab(&mut app, page);
                } else {
                    crate::import_csv::open(&mut app);
                    crate::import_csv::tests::set_tab(&mut app, page);
                }
                let mut texts = Vec::new();
                for _ in 0..4 {
                    texts = frame(&mut app, &ctx, size, Vec::new());
                }
                let window = window_rect(&ctx, title, false);
                for t in ["Back", "Cancel"] {
                    let r = *find(&texts, t);
                    assert!(window.contains_rect(r), "{title} page {page}: {t} at {r:?}");
                }
            }
        }
    }

    /// The rectangle of a dialog's window.
    fn window_rect(ctx: &egui::Context, title: &str, maximized: bool) -> Rect {
        let id = egui::Id::new(("dialog", title, 2, maximized));
        ctx.memory(|m| m.area_rect(id)).expect("no window")
    }

    /// A press, moves and a release of the mouse from `from` to `to`.
    fn drag(
        app: &mut EpanetApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        from: egui::Pos2,
        to: egui::Pos2,
    ) {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(app, ctx, size, vec![egui::Event::PointerMoved(from)]);
        frame(app, ctx, size, vec![button(from, true)]);
        for k in 1..=10 {
            let p = from + (to - from) * (k as f32 / 10.0);
            frame(app, ctx, size, vec![egui::Event::PointerMoved(p)]);
        }
        frame(app, ctx, size, vec![button(to, false)]);
        frame(app, ctx, size, Vec::new());
    }

    /// In the browser the first frames come before the screen size is
    /// known; the Welcome Page still opens in the middle.
    #[test]
    fn welcome_opens_centered_after_unknown_screen() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.dialog = Dialog::Welcome;
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(&mut app, ui.ctx()));
            out.textures_delta.clear();
        }
        let size = vec2(1600.0, 865.0);
        for _ in 0..10 {
            frame(&mut app, &ctx, size, Vec::new());
        }
        let r = window_rect(&ctx, "Welcome Page", false);
        let c = Rect::from_min_size(egui::Pos2::ZERO, size).center();
        assert!((r.center() - c).length() < 3.0, "{r:?}");
    }
    #[test]
    fn dialogs_resize_and_maximize() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.dialog = Dialog::Units;
        let size = vec2(1400.0, 900.0);
        let title = "Units of Measurement";
        for _ in 0..10 {
            frame(&mut app, &ctx, size, Vec::new());
        }
        let before = window_rect(&ctx, title, false);
        // drag the bottom right corner 150 px right and 80 px down
        let corner = before.right_bottom() - vec2(2.0, 2.0);
        drag(&mut app, &ctx, size, corner, corner + vec2(150.0, 80.0));
        let after = window_rect(&ctx, title, false);
        assert!(
            (after.width() - before.width() - 150.0).abs() < 3.0,
            "{before:?} {after:?}"
        );
        assert!(
            (after.height() - before.height() - 80.0).abs() < 3.0,
            "{before:?} {after:?}"
        );

        // a double click on the title bar maximizes the window...
        let texts = frame(&mut app, &ctx, size, Vec::new());
        let at = find(&texts, title).center();
        let click = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&mut app, &ctx, size, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false, true, false] {
            frame(&mut app, &ctx, size, vec![click(pressed)]);
        }
        for _ in 0..3 {
            frame(&mut app, &ctx, size, Vec::new());
        }
        let max = window_rect(&ctx, title, true);
        assert!(max.width() > 1380.0 && max.height() > 880.0, "{max:?}");
        // ...and another one gives back the size and place it had
        let texts = frame(&mut app, &ctx, size, Vec::new());
        let at = find(&texts, title).center();
        let click = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&mut app, &ctx, size, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false, true, false] {
            frame(&mut app, &ctx, size, vec![click(pressed)]);
        }
        for _ in 0..3 {
            frame(&mut app, &ctx, size, Vec::new());
        }
        let back = window_rect(&ctx, title, false);
        assert!(
            (back.size() - after.size()).length() < 3.0,
            "{after:?} {back:?}"
        );
    }
    #[test]
    fn dialogs_can_be_moved_and_open_centered() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.dialog = Dialog::Units;
        let size = vec2(1400.0, 900.0);
        let title = "Units of Measurement";
        let mut texts = Vec::new();
        for _ in 0..10 {
            texts = frame(&mut app, &ctx, size, Vec::new());
        }
        let before = *find(&texts, title);
        // drag the title bar 200 px right and 100 px down
        let at = before.center();
        let to = at + vec2(200.0, 100.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: if pressed { at } else { to },
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&mut app, &ctx, size, vec![egui::Event::PointerMoved(at)]);
        frame(&mut app, &ctx, size, vec![press(true)]);
        for k in 1..=10 {
            let p = at + (to - at) * (k as f32 / 10.0);
            frame(&mut app, &ctx, size, vec![egui::Event::PointerMoved(p)]);
        }
        frame(&mut app, &ctx, size, vec![press(false)]);
        let texts = frame(&mut app, &ctx, size, Vec::new());
        let after = *find(&texts, title);
        let moved = after.center() - before.center();
        assert!((moved - vec2(200.0, 100.0)).length() < 2.0, "{moved:?}");
        // closed and opened again: back in the middle
        app.dialog = Dialog::None;
        frame(&mut app, &ctx, size, Vec::new());
        app.dialog = Dialog::Units;
        let mut texts = Vec::new();
        for _ in 0..10 {
            texts = frame(&mut app, &ctx, size, Vec::new());
        }
        assert!((find(&texts, title).center() - before.center()).length() < 2.0);
    }
}
