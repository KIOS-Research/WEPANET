//! Main menu, as in EPANET-UI (`mainmenu.lfm`): a colored menu bar with
//! File / Edit / Map / Project / Help, a toolbar for the selected menu, the
//! quick-access speed bar on the right, and the drop-down File menu.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use eui_core::{LinkKind, NodeKind};

use crate::app::{Dialog, EpanetApp, MenuTab, ReportKind, Tool, ViewMode};
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::style;

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    let pal = app.palette();
    egui::Panel::top("menubar")
        .exact_size(34.0)
        .frame(egui::Frame::NONE.fill(pal.menu))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let tabs: [(Option<MenuTab>, &str); 6] = [
                    (None, "File"),
                    (Some(MenuTab::Edit), "Edit"),
                    (Some(MenuTab::Map), "Map"),
                    (Some(MenuTab::Project), "Project"),
                    (Some(MenuTab::Plugins), "Plugins"),
                    (Some(MenuTab::Help), "Help"),
                ];
                for (tab, name) in tabs {
                    let selected = match tab {
                        None => app.backstage,
                        Some(t) => !app.backstage && app.prefs.menu_tab == t,
                    };
                    if menu_tab(ui, name, selected, pal.form).clicked() {
                        match tab {
                            None => app.backstage = !app.backstage,
                            Some(t) => {
                                app.prefs.menu_tab = t;
                                app.backstage = false;
                            }
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(6.0);
                    speed_bar(app, ui);
                });
            });
        });
    egui::Panel::top("toolbar")
        .exact_size(60.0)
        .frame(
            egui::Frame::NONE
                .fill(toolbar_fill(ui))
                .inner_margin(egui::Margin::symmetric(6, 2)),
        )
        .show(ui, |ui| {
            if app.backstage {
                return;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                match app.prefs.menu_tab {
                    MenuTab::Edit => edit_toolbar(app, ui),
                    MenuTab::Map => map_toolbar(app, ui),
                    MenuTab::Project => project_toolbar(app, ui),
                    MenuTab::Plugins => plugins_toolbar(app, ui),
                    MenuTab::Help => help_toolbar(app, ui),
                }
            });
        });
}

/// Background of the toolbar and of the selected menu tab (white, as in EPANET-UI).
pub(crate) fn toolbar_fill(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        ui.visuals().panel_fill
    } else {
        Color32::WHITE
    }
}

fn menu_tab(ui: &mut Ui, name: &str, selected: bool, form: Color32) -> Response {
    let name = tr(name);
    let font = crate::fonts::bold(12.5);
    let w = ui.fonts_mut(|f| {
        f.layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
    }) + 40.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(76.0), 34.0), Sense::click());
    let p = ui.painter();
    let fg = if selected {
        p.rect_filled(rect.with_min_y(rect.top() + 2.0), 0.0, toolbar_fill(ui));
        ui.visuals().strong_text_color()
    } else {
        if resp.hovered() {
            p.rect_filled(
                rect.with_min_y(rect.top() + 2.0),
                0.0,
                form.gamma_multiply(0.25),
            );
        }
        Color32::WHITE
    };
    p.text(
        rect.center() + vec2(0.0, 1.0),
        Align2::CENTER_CENTER,
        name,
        font,
        fg,
    );
    resp
}

fn speed_bar(app: &mut EpanetApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    egui::Frame::NONE
        .fill(Color32::from_rgb(0xdf, 0xe8, 0xf3))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0xa9, 0xb8, 0xc8)))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(egui::Margin::symmetric(3, 1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let dark = Color32::from_rgb(0x18, 0x18, 0x18);
            let small = |ui: &mut Ui, icon: Icon, tip: &str| -> bool {
                let (rect, resp) = ui.allocate_exact_size(vec2(23.0, 23.0), Sense::click());
                if resp.hovered() {
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(3), Color32::WHITE);
                }
                icons::paint(ui.painter(), icon, rect.center(), 16.0, dark);
                resp.on_hover_text(tr(tip)).clicked()
            };
            // Right-to-left layout: listed in reverse visual order.
            // Last, apart and in red: report an issue on GitHub.
            let (rect, resp) = ui.allocate_exact_size(vec2(23.0, 23.0), Sense::click());
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(3), Color32::WHITE);
            }
            issue_icon(
                ui.painter(),
                rect.center() + vec2(0.0, 1.0),
                0.72,
                ISSUE_RED,
                Some(ISSUE_RED),
            );
            if resp
                .on_hover_text(tr(
                    "Report a problem or suggest a feature on WEPANET's GitHub",
                ))
                .clicked()
            {
                crate::help::report_issue(&ctx);
            }
            let (sep, _) = ui.allocate_exact_size(vec2(5.0, 23.0), Sense::hover());
            ui.painter().vline(
                sep.center().x,
                sep.y_range().shrink(3.0),
                Stroke::new(1.0, Color32::from_rgb(0xa9, 0xb8, 0xc8)),
            );
            if small(ui, Icon::Help, "Help") {
                app.dialog = Dialog::About;
            }
            if small(ui, Icon::Report, "Report") {
                let kind = app.report_kind;
                crate::reports::open(app, kind);
            }
            if small(ui, Icon::Analyze, "Analyze (F5)") {
                app.run_simulation(&ctx);
            }
            if small(ui, Icon::FullExtent, "Full Extent") {
                app.view_mode = ViewMode::Map;
                app.view.fit_pending = true;
            }
            if small(ui, Icon::ZoomOut, "Zoom Out") {
                app.view.zoom_steps(-1.0);
            }
            if small(ui, Icon::ZoomIn, "Zoom In") {
                app.view.zoom_steps(1.0);
            }
            if small(ui, Icon::Save, "Save") {
                app.save(false);
            }
            if small(ui, Icon::Open, "Open") {
                app.open(&ctx);
            }
            if small(ui, Icon::New, "New") {
                app.new_project();
            }
        });
}

/// A toolbar button: icon above caption, as in EPANET-UI. A caption ending
/// in " ▾" gets a drop-down arrow beside the icon.
fn tool_button(
    ui: &mut Ui,
    icon: Icon,
    caption: &str,
    enabled: bool,
    active: bool,
    tip: &str,
) -> Response {
    tool_button_with(
        ui,
        |p, c, fg| icons::paint(p, icon, c, 24.0, fg),
        caption,
        enabled,
        active,
        tip,
    )
}

/// The Report Issue icon: a speech bubble with an exclamation mark.
fn paint_issue(p: &egui::Painter, c: egui::Pos2, fg: Color32) {
    issue_icon(p, c, 1.0, fg, None);
}

/// The red Report Issue button of the speed bar.
const ISSUE_RED: Color32 = Color32::from_rgb(0xd3, 0x2f, 0x2f);

/// A speech bubble with an exclamation mark, `s` times the toolbar size;
/// with `fill`, a filled bubble and a white mark.
fn issue_icon(p: &egui::Painter, c: egui::Pos2, s: f32, fg: Color32, fill: Option<Color32>) {
    let stroke = Stroke::new(1.6 * s.max(0.8), fg);
    let r = Rect::from_center_size(c - vec2(0.0, 2.0 * s), vec2(22.0, 16.0) * s);
    // the tail, under the bubble's left part
    let a = r.left_bottom() + vec2(5.0, 0.0) * s;
    let tail = [a, a + vec2(-1.0, 6.0) * s, a + vec2(6.0, 0.0) * s];
    let mark = match fill {
        Some(f) => {
            p.rect_filled(r, 3.0 * s, f);
            p.add(egui::Shape::convex_polygon(
                vec![tail[0] - vec2(0.0, 1.0), tail[1], tail[2] - vec2(0.0, 1.0)],
                f,
                Stroke::NONE,
            ));
            Color32::WHITE
        }
        None => {
            p.rect_stroke(r, 3.0 * s, stroke, egui::StrokeKind::Middle);
            p.line_segment([tail[0], tail[1]], stroke);
            p.line_segment([tail[1], tail[2]], stroke);
            fg
        }
    };
    p.line_segment(
        [
            r.center_top() + vec2(0.0, 3.5 * s),
            r.center() + vec2(0.0, 2.0 * s),
        ],
        Stroke::new(2.0 * s.max(0.8), mark),
    );
    p.circle_filled(
        r.center_bottom() - vec2(0.0, 3.5 * s),
        1.2 * s.max(0.8),
        mark,
    );
}

/// A toolbar button whose icon `paint` draws (centred at the given point).
fn tool_button_with(
    ui: &mut Ui,
    paint: impl FnOnce(&egui::Painter, egui::Pos2, Color32),
    caption: &str,
    enabled: bool,
    active: bool,
    tip: &str,
) -> Response {
    let (caption, dropdown) = match caption.strip_suffix(" ▾") {
        Some(c) => (tr(c), true),
        None => (tr(caption), false),
    };
    let tip = tr(tip);
    let font = FontId::proportional(12.0);
    let text_w = ui.fonts_mut(|f| {
        f.layout_no_wrap(caption.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
    });
    let size = vec2(
        (text_w + 30.0).max(96.0) + if dropdown { 18.0 } else { 0.0 },
        56.0,
    );
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let v = ui.visuals();
    let fg = if enabled {
        v.text_color()
    } else {
        v.weak_text_color().gamma_multiply(0.55)
    };
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 0.0, style::ACCENT.gamma_multiply(0.14));
    } else if enabled && resp.hovered() {
        p.rect_filled(rect, 0.0, Color32::from_rgb(0xe3, 0xf0, 0xfc));
    }
    let body_x = rect.center().x - if dropdown { 9.0 } else { 0.0 };
    paint(p, pos2(body_x, rect.top() + 20.0), fg);
    p.text(
        pos2(body_x, rect.bottom() - 11.0),
        Align2::CENTER_CENTER,
        caption,
        font,
        fg,
    );
    if dropdown {
        let c = pos2(rect.right() - 12.0, rect.top() + 26.0);
        p.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-4.5, -2.5),
                c + vec2(4.5, -2.5),
                c + vec2(0.0, 2.5),
            ],
            fg,
            Stroke::NONE,
        ));
    }
    if enabled {
        resp.on_hover_text(tip)
    } else {
        resp.on_hover_text(format!("{tip}\n({})", tr("not available yet")))
    }
}

fn separator(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(2.0, 56.0), Sense::hover());
    ui.painter().vline(
        r.center().x,
        r.top()..=r.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

/// Opens a drop-down menu under a toolbar button.
fn dropdown(resp: &Response, add: impl FnOnce(&mut Ui)) {
    egui::Popup::menu(resp).gap(0.0).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.set_min_width(200.0);
        add(ui);
    });
}

fn edit_toolbar(app: &mut EpanetApp, ui: &mut Ui) {
    let has_sel = app.selection.is_some();
    let link = app.selected_link();
    if tool_button(
        ui,
        Icon::Copy,
        "Copy",
        has_sel,
        false,
        "Copy the selected object's properties (Ctrl+C)",
    )
    .clicked()
    {
        app.copy_selection();
    }
    if tool_button(
        ui,
        Icon::Paste,
        "Paste",
        app.can_paste(),
        false,
        "Paste copied properties into the selected object (Ctrl+V)",
    )
    .clicked()
    {
        app.paste_selection();
    }
    if tool_button(
        ui,
        Icon::Reverse,
        "Reverse Link",
        link.is_some(),
        false,
        "Reverse the direction of a link",
    )
    .clicked()
        && let Some(i) = link
    {
        app.checkpoint();
        app.net.reverse_link(i);
        app.status = format!("Reversed link {}", app.net.links[i].id);
    }
    let reshape = app.tool == Tool::Reshape;
    if tool_button(
        ui,
        Icon::Reshape,
        "Reshape Link",
        link.is_some(),
        reshape,
        "Edit vertices used to shape a link",
    )
    .clicked()
    {
        app.set_tool(if reshape { Tool::Select } else { Tool::Reshape });
    }
    separator(ui);
    if tool_button(
        ui,
        Icon::Undo,
        "Undo",
        app.can_undo(),
        false,
        "Undo (Ctrl+Z)",
    )
    .clicked()
    {
        app.undo();
    }
    if tool_button(
        ui,
        Icon::Redo,
        "Redo",
        app.can_redo(),
        false,
        "Redo (Ctrl+Y)",
    )
    .clicked()
    {
        app.redo();
    }
    separator(ui);
    tool_button(
        ui,
        Icon::GroupEdit,
        "Group Edit",
        false,
        false,
        "Edit a property for a group of objects",
    );
    tool_button(
        ui,
        Icon::GroupDelete,
        "Group Delete",
        false,
        false,
        "Delete a group of objects",
    );
    let conv = tool_button(
        ui,
        Icon::Convert,
        "Convert Link ▾",
        link.is_some(),
        false,
        "Convert a link to another type",
    );
    dropdown(&conv, |ui| {
        for (k, icon) in [
            (LinkKind::Pipe, Icon::Pipe),
            (LinkKind::Pump, Icon::Pump),
            (LinkKind::Valve, Icon::Valve),
        ] {
            if icons::menu_item(ui, icon, &format!("to {}", k.label()), true).clicked()
                && let Some(i) = link
                && app.net.links[i].kind != k
            {
                app.checkpoint();
                app.net.convert_link(i, k);
                app.buffer_owner = None;
                app.status = format!(
                    "Converted link {} to a {}",
                    app.net.links[i].id,
                    k.label().to_lowercase()
                );
                ui.close();
            }
        }
    });
}

fn map_toolbar(app: &mut EpanetApp, ui: &mut Ui) {
    let basemap = tool_button(
        ui,
        Icon::Basemap,
        "Basemap ▾",
        true,
        app.prefs.basemap != crate::basemap::BasemapKind::None,
        "Add/remove/adjust a basemap layer",
    );
    dropdown(&basemap, |ui| crate::basemap::menu(app, ui));
    let layers = tool_button(
        ui,
        Icon::Layers,
        "Layers ▾",
        true,
        app.prefs.map_layers.iter().any(|l| l.visible),
        "Add layers from WMS or ArcGIS REST map services",
    );
    dropdown(&layers, |ui| {
        if icons::menu_item(ui, Icon::Add, "Add Layer", true).clicked() {
            crate::layers::open_add(app);
            ui.close();
        }
        if icons::check_item(ui, "Map Layers Panel", !app.prefs.hide_layers).clicked() {
            app.prefs.hide_layers = !app.prefs.hide_layers;
            ui.close();
        }
    });
    let export = tool_button(
        ui,
        Icon::Export,
        "Export ▾",
        true,
        false,
        "Export the map as an image",
    );
    dropdown(&export, |ui| {
        if icons::menu_item(ui, Icon::Copy, "To Clipboard", true).clicked() {
            crate::export::request(app, crate::export::ExportTarget::Clipboard);
            ui.close();
        }
        if icons::menu_item(ui, Icon::Save, "To File", true).clicked() {
            crate::export::request(app, crate::export::ExportTarget::File);
            ui.close();
        }
    });
    if tool_button(
        ui,
        Icon::Query,
        "Query",
        true,
        app.query.active,
        "Find items meeting some criterion",
    )
    .clicked()
    {
        app.dialog = Dialog::Query;
    }
    separator(ui);
    if tool_button(
        ui,
        Icon::ZoomIn,
        "Zoom In",
        true,
        false,
        "Zoom in on the map",
    )
    .clicked()
    {
        app.view.zoom_steps(1.0);
    }
    if tool_button(
        ui,
        Icon::ZoomOut,
        "Zoom Out",
        true,
        false,
        "Zoom out on the map",
    )
    .clicked()
    {
        app.view.zoom_steps(-1.0);
    }
    if tool_button(
        ui,
        Icon::FullExtent,
        "Full Extent",
        true,
        false,
        "View the map at full extent",
    )
    .clicked()
    {
        app.view_mode = ViewMode::Map;
        app.view.fit_pending = true;
    }
    separator(ui);
    if tool_button(
        ui,
        Icon::Settings,
        "Settings",
        true,
        false,
        "Change the map display settings",
    )
    .clicked()
    {
        app.dialog = Dialog::MapSettings;
    }
}

fn project_toolbar(app: &mut EpanetApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if tool_button(
        ui,
        Icon::Setup,
        "Setup",
        true,
        false,
        "Set project options, including flow units and head loss model",
    )
    .clicked()
    {
        crate::setup::open(app);
    }
    let adding = matches!(
        app.tool,
        Tool::AddNode(_) | Tool::AddLink(_) | Tool::AddLabel
    );
    let add = tool_button(
        ui,
        Icon::Add,
        "Add ▾",
        true,
        adding,
        "Add a new object to the project",
    );
    dropdown(&add, |ui| {
        for (k, icon) in [
            (NodeKind::Junction, Icon::Junction),
            (NodeKind::Reservoir, Icon::Reservoir),
            (NodeKind::Tank, Icon::Tank),
        ] {
            if icons::menu_item(ui, icon, &format!("{} Node", k.label()), true).clicked() {
                app.set_tool(Tool::AddNode(k));
                ui.close();
            }
        }
        ui.separator();
        for (k, icon) in [
            (LinkKind::Pipe, Icon::Pipe),
            (LinkKind::Pump, Icon::Pump),
            (LinkKind::Valve, Icon::Valve),
        ] {
            if icons::menu_item(ui, icon, &format!("{} Link", k.label()), true).clicked() {
                app.set_tool(Tool::AddLink(k));
                ui.close();
            }
        }
        ui.separator();
        if icons::menu_item(ui, Icon::Label, "Map Label", true).clicked() {
            app.set_tool(Tool::AddLabel);
            ui.close();
        }
    });
    if tool_button(
        ui,
        Icon::Delete,
        "Delete",
        app.selection.is_some(),
        false,
        "Delete the selected object from the project (Del / Backspace)",
    )
    .clicked()
    {
        app.request_delete();
    }
    if tool_button(ui, Icon::Locate, "Locate", true, false, "Find an object").clicked() {
        app.dialog = Dialog::Locate;
    }
    separator(ui);
    if tool_button(
        ui,
        Icon::Summary,
        "Summary",
        true,
        false,
        "Summarize the project's contents",
    )
    .clicked()
    {
        app.dialog = Dialog::Summary;
    }
    if tool_button(
        ui,
        Icon::Details,
        "Details",
        true,
        false,
        "View all project data",
    )
    .clicked()
    {
        app.data_view.open = !app.data_view.open;
    }
    separator(ui);
    let running = app.is_running();
    if tool_button(
        ui,
        Icon::Analyze,
        if running { "Analyzing…" } else { "Analyze" },
        !running,
        false,
        "Run a hydraulic / water quality analysis (F5)",
    )
    .clicked()
    {
        app.run_simulation(&ctx);
    }
    let msx_on = app.msx.enabled && app.net.msx.is_some();
    if tool_button_with(
        ui,
        paint_molecule,
        "EPANET-MSX",
        true,
        msx_on,
        if msx_on {
            "Multi-species water quality: runs with each analysis (open to edit or remove)"
        } else {
            "Multi-species water quality: load, write or edit the project's MSX file"
        },
    )
    .clicked()
    {
        crate::msx::open_editor(app);
    }
    let rep = tool_button(
        ui,
        Icon::Report,
        "Report ▾",
        true,
        app.view_mode == ViewMode::Report,
        "View analysis results",
    );
    dropdown(&rep, |ui| {
        for (g, group) in ReportKind::MENU.iter().enumerate() {
            if g > 0 {
                ui.separator();
            }
            for &k in *group {
                if icons::menu_item(ui, k.icon(), k.label(), true).clicked() {
                    crate::reports::open(app, k);
                    ui.close();
                }
            }
        }
    });
}

/// Elad Salomons' EPANET plugins (see `plugins.rs`).
fn plugins_toolbar(app: &mut EpanetApp, ui: &mut Ui) {
    let has_net = !app.net.nodes.is_empty();
    if tool_button(
        ui,
        Icon::Setup,
        "Manage Plugins",
        true,
        false,
        "Install, upgrade and remove plugins (also from a file)",
    )
    .clicked()
    {
        app.dialog = Dialog::PluginManager;
    }
    if tool_button(
        ui,
        Icon::About,
        "About Plugins",
        true,
        false,
        "The EPANET-Plugins project by Elad Salomons",
    )
    .clicked()
    {
        ui.ctx()
            .open_url(egui::OpenUrl::new_tab(crate::plugins::PLUGINS_REPO));
    }
    if tool_button(
        ui,
        Icon::Python,
        "Python Console",
        true,
        app.python.console.open,
        "Python, with the project in `wepanet`",
    )
    .clicked()
    {
        crate::python::console::toggle(app);
    }
    // Installed plugins: the core ones (built in), then the scripts.
    let on = |app: &EpanetApp, name: &str| !app.prefs.disabled_plugins.contains(name);
    let core: Vec<&str> = crate::plugin_manager::CORE
        .iter()
        .map(|c| c.name)
        .filter(|n| app.prefs.core_plugins.contains(*n) && on(app, n))
        .collect();
    if !core.is_empty() || app.scripts.plugins.iter().any(|p| on(app, &p.name)) {
        separator(ui);
    }
    for name in core {
        let c = crate::plugin_manager::core(name).expect("a core plugin");
        if tool_button(ui, c.icon, c.name, has_net, false, c.description).clicked() {
            app.dialog = c.dialog;
        }
    }
    for p in app.scripts.plugins.clone() {
        if app.prefs.disabled_plugins.contains(&p.name) {
            continue;
        }
        let tip = if p.description.is_empty() {
            format!("{} ({})", p.name, p.file)
        } else {
            format!("{}\n{}", p.description, p.file)
        };
        let tex = crate::plugin_manager::installed_icon(app, &ui.ctx().clone(), &p.name);
        let b = tool_button_with(
            ui,
            |pt, c, fg| {
                crate::plugin_manager::paint_icon(
                    pt,
                    crate::plugin_manager::Pic::or_default(tex),
                    c,
                    28.0,
                    fg,
                )
            },
            &format!("{} ▾", p.name),
            true,
            false,
            &tip,
        );
        dropdown(&b, |ui| {
            for item in &p.menu {
                if icons::menu_item(ui, Icon::Analyze, item, true).clicked() {
                    crate::scripting::run(app, &p.name, item);
                    ui.close();
                }
            }
            ui.separator();
            if icons::menu_item(ui, Icon::Delete, "Remove Plugin", true).clicked() {
                crate::scripting::remove(app, &p.name);
                ui.close();
            }
        });
    }
}

fn help_toolbar(app: &mut EpanetApp, ui: &mut Ui) {
    if tool_button(
        ui,
        Icon::Help,
        "User Manual",
        true,
        false,
        "Open the EPANET-UI user manual",
    )
    .clicked()
    {
        crate::help::open_doc(ui.ctx(), crate::help::Doc::Manual, None);
    }
    if tool_button(
        ui,
        Icon::Units,
        "Units",
        true,
        false,
        "Units of measurement",
    )
    .clicked()
    {
        app.dialog = Dialog::Units;
    }
    if tool_button(
        ui,
        Icon::ErrorCodes,
        "Error Codes",
        true,
        false,
        "EPANET error and warning codes",
    )
    .clicked()
    {
        app.dialog = Dialog::ErrorCodes;
    }
    if tool_button(
        ui,
        Icon::Tutorial,
        "Tutorial",
        true,
        false,
        "Basic tutorial",
    )
    .clicked()
    {
        crate::help::open_doc(ui.ctx(), crate::help::Doc::Tutorial, None);
    }
    if tool_button_with(
        ui,
        paint_issue,
        "Report Issue",
        true,
        false,
        "Report a problem or suggest a feature on WEPANET's GitHub",
    )
    .clicked()
    {
        crate::help::report_issue(ui.ctx());
    }
    if tool_button(ui, Icon::About, "About", true, false, "About WEPANET").clicked() {
        app.dialog = Dialog::About;
    }
}

// ----- File menu ----------------------------------------------------------------

/// The File menu: a drop-down panel under the menu bar with the commands on
/// the left, the recent projects on the right and a hint line at the bottom.
pub fn file_menu(app: &mut EpanetApp, ctx: &egui::Context) {
    let pal = app.palette();
    let mut hint = String::new();
    let mut close = false;
    let size = vec2(500.0, 452.0);
    let area = egui::Area::new(egui::Id::new("file_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(0.0, 34.0))
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let v = ui.visuals().clone();
            let bg = if v.dark_mode { v.panel_fill } else { pal.form };
            let p = ui.painter();
            p.rect(
                rect,
                0.0,
                bg,
                Stroke::new(1.0, Color32::from_gray(160)),
                egui::StrokeKind::Inside,
            );
            let left = Rect::from_min_size(rect.min, vec2(190.0, size.y - 40.0));
            let right = Rect::from_min_max(
                pos2(left.right(), rect.top()),
                pos2(rect.right(), left.bottom()),
            );
            let hint_rect = Rect::from_min_max(pos2(rect.left(), left.bottom()), rect.max);
            p.rect_filled(
                right.shrink(1.0),
                0.0,
                if v.dark_mode {
                    v.extreme_bg_color
                } else {
                    Color32::WHITE
                },
            );
            p.vline(
                right.left(),
                right.y_range(),
                Stroke::new(1.0, Color32::from_gray(190)),
            );
            p.hline(
                rect.x_range(),
                hint_rect.top(),
                Stroke::new(1.0, Color32::from_gray(190)),
            );
            let text = v.text_color();

            // commands
            let y = std::cell::Cell::new(left.top() + 6.0);
            let mut item =
                |ui: &mut Ui, icon: Icon, label: &str, help: &str, enabled: bool, h: f32| -> bool {
                    let r = Rect::from_min_size(
                        pos2(left.left() + 1.0, y.get()),
                        vec2(left.width() - 2.0, h),
                    );
                    y.set(y.get() + h);
                    let resp = ui.interact(
                        r,
                        ui.id().with(("fm", label)),
                        if enabled {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    let label = tr(label);
                    if resp.hovered() {
                        hint = tr(help).to_string();
                        if enabled {
                            ui.painter()
                                .rect_filled(r, 0.0, Color32::from_rgb(0xcc, 0xe4, 0xf7));
                        }
                    }
                    let fg = if enabled { text } else { v.weak_text_color() };
                    icons::paint(
                        ui.painter(),
                        icon,
                        r.left_center() + vec2(16.0, 0.0),
                        15.0,
                        fg,
                    );
                    if !label.is_empty() {
                        ui.painter().text(
                            r.left_center() + vec2(34.0, 0.0),
                            Align2::LEFT_CENTER,
                            label,
                            FontId::proportional(13.5),
                            fg,
                        );
                    }
                    resp.clicked()
                };
            if item(ui, Icon::Back, "", "Exit this menu.", true, 36.0) {
                close = true;
            }
            y.set(y.get() + 14.0);
            if item(ui, Icon::New, "New", "Start a new project.", true, 49.0) {
                app.new_project();
                close = true;
            }
            if item(
                ui,
                Icon::Open,
                "Open",
                "Open a previously saved project.",
                true,
                49.0,
            ) {
                close = true;
                app.open(ctx);
            }
            if item(
                ui,
                Icon::Save,
                "Save",
                "Save the project under its current name.",
                true,
                49.0,
            ) {
                app.save(false);
                close = true;
            }
            if item(
                ui,
                Icon::SaveAs,
                "Save as",
                "Save the project under a new name.",
                true,
                49.0,
            ) {
                app.save(true);
                close = true;
            }
            if item(
                ui,
                Icon::Layers,
                "Save Project",
                "Save the network with its coordinate system, basemap, map layers and view, in one .wepanet file.",
                true,
                49.0,
            ) {
                app.save_project();
                close = true;
            }
            let import_y = y.get();
            let import_id = egui::Id::new("backstage_import");
            let mut importing: bool = ctx.data(|d| d.get_temp(import_id)).unwrap_or(false);
            if item(
                ui,
                Icon::Import,
                "Import",
                "Import network data from a shapefile, DXF or CSV file.",
                true,
                49.0,
            ) {
                importing = !importing;
                ctx.data_mut(|d| d.insert_temp(import_id, importing));
            }
            ui.painter().text(
                pos2(left.right() - 44.0, import_y + 24.5),
                Align2::CENTER_CENTER,
                ">",
                FontId::proportional(14.0),
                v.weak_text_color(),
            );
            if item(
                ui,
                Icon::Preferences,
                "Preferences",
                "Configure program settings.",
                true,
                49.0,
            ) {
                app.dialog = Dialog::Preferences;
                close = true;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if item(ui, Icon::Quit, "Quit", "Exit the program.", true, 49.0) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // recent projects and samples
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(right.shrink2(vec2(1.0, 1.0)))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let heading = |ui: &mut Ui, title: &str| {
                let title = tr(title);
                let (r, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                ui.painter().rect_filled(
                    r,
                    0.0,
                    if v.dark_mode {
                        v.faint_bg_color
                    } else {
                        Color32::from_gray(0xf0)
                    },
                );
                ui.painter().hline(
                    r.x_range(),
                    r.bottom(),
                    Stroke::new(1.0, Color32::from_gray(200)),
                );
                ui.painter().text(
                    r.left_center() + vec2(6.0, 0.0),
                    Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(13.5),
                    text,
                );
            };
            let row = |ui: &mut Ui, label: &str, tip: &str| -> bool {
                let (r, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                if resp.hovered() {
                    ui.painter()
                        .rect_filled(r, 0.0, Color32::from_rgb(0xcc, 0xe4, 0xf7));
                }
                ui.painter().text(
                    r.left_center() + vec2(6.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(13.5),
                    text,
                );
                resp.on_hover_text(tr(tip)).clicked()
            };
            if importing {
                heading(&mut child, "Import Network Data From");
                // EPANET-UI's list: tall rows with the file type images
                let sources = [
                    (
                        "GIS Shapefile",
                        "Import pipes and nodes from ESRI shapefiles (.shp, .dbf, .prj or a .zip)",
                        true,
                    ),
                    (
                        "CAD DXF File",
                        "Import from a CAD drawing (coming soon)",
                        false,
                    ),
                    (
                        "CSV Text File",
                        "Import from CSV text files (coming soon)",
                        false,
                    ),
                ];
                for (k, (label, tip, enabled)) in sources.iter().enumerate() {
                    let (r, resp) = child.allocate_exact_size(
                        vec2(child.available_width(), 64.0),
                        if *enabled {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    if *enabled && resp.hovered() {
                        child
                            .painter()
                            .rect_filled(r, 0.0, Color32::from_rgb(0xcc, 0xe4, 0xf7));
                    }
                    crate::icons::paint_import(
                        child.painter(),
                        k,
                        r.left_top() + vec2(22.0, 24.0),
                        26.0,
                    );
                    child.painter().text(
                        r.left_top() + vec2(44.0, 24.0),
                        Align2::LEFT_CENTER,
                        tr(label),
                        FontId::proportional(15.0),
                        if *enabled { text } else { v.weak_text_color() },
                    );
                    if resp.on_hover_text(tr(tip)).clicked() && *enabled {
                        crate::import_shp::open(app);
                        ctx.data_mut(|d| d.insert_temp(import_id, false));
                        close = true;
                    }
                }
            } else {
                heading(&mut child, "Recent Projects");
                for (k, r) in app.prefs.recent.clone().into_iter().enumerate() {
                    let name = std::path::Path::new(&r)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or(r.clone());
                    if row(&mut child, &format!("{}.  {name}", k + 1), &r) {
                        app.open_recent(&r);
                        close = true;
                    }
                }
                if app.prefs.recent.is_empty() {
                    child.add_space(4.0);
                    child.label(
                        egui::RichText::new(format!("  {}", tr("There are no recent projects")))
                            .weak(),
                    );
                    child.add_space(10.0);
                }
                heading(&mut child, "Sample Projects");
                for (name, inp) in eui_core::examples::ALL {
                    let desc = match *name {
                        "Net1" => "Simple network with chlorine decay",
                        "Net2" => "Fluoride tracer study",
                        _ => "Dual-source system, source tracing",
                    };
                    if row(&mut child, &format!("{name}.inp"), tr(desc)) {
                        app.load_example(name, inp);
                        close = true;
                    }
                }
                heading(&mut child, "Benchmark Networks");
                if row(
                    &mut child,
                    "WaterBenchmarkHub catalog...",
                    "Download a network from the WaterFutures WaterBenchmarkHub",
                ) {
                    app.dialog = Dialog::Benchmarks;
                    close = true;
                }
            }

            // hint line
            ui.painter().text(
                hint_rect.left_center() + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                &hint,
                FontId::proportional(13.5),
                text,
            );
        });
    // Clicking anywhere else closes the menu.
    let clicked_outside = ctx.input(|i| i.pointer.any_click())
        && !area.response.contains_pointer()
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| p.y > 34.0);
    if close || clicked_outside {
        app.backstage = false;
    }
}

/// The EPANET-MSX button's icon: a molecule of three atoms.
fn paint_molecule(p: &egui::Painter, c: egui::Pos2, fg: Color32) {
    let atoms = [
        (vec2(-7.0, 5.0), 5.5, Color32::from_rgb(0x2e, 0x86, 0xc1)),
        (vec2(7.0, 5.0), 5.5, Color32::from_rgb(0xc0, 0x39, 0x2b)),
        (vec2(0.0, -7.0), 6.5, Color32::from_rgb(0x27, 0xae, 0x60)),
    ];
    let bond = Stroke::new(2.0, fg);
    for i in 0..3 {
        p.line_segment([c + atoms[i].0, c + atoms[(i + 1) % 3].0], bond);
    }
    for (o, r, col) in atoms {
        p.circle(c + o, r, col, Stroke::new(1.0, fg));
    }
}
