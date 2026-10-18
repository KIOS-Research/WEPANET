//! The Map Viewer panel (left), as in EPANET-UI's `mapviewerframe.lfm`:
//! View (Map / Report), Themes, Time and Layers.

use egui::{Align2, Color32, RichText, Sense, Ui, vec2};

use crate::app::{EpanetApp, Tool, ViewMode};
use crate::style::{LinkTheme, NodeTheme};

/// A panel title bar in the menu color, as EPANET-UI draws them.
pub(crate) fn panel_header(ui: &mut Ui, title: &str, color: Color32) {
    let title = crate::i18n::tr(title);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        title,
        crate::fonts::bold(13.0),
        Color32::WHITE,
    );
}

/// Elapsed time as EPANET-UI shows it, e.g. `06:00 hrs`.
fn format_hours(secs: u32) -> String {
    format!("{:02}:{:02} hrs", secs / 3600, (secs % 3600) / 60)
}

/// A titled group box.
pub(crate) fn group_box(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(crate::i18n::tr(title)).font(crate::fonts::bold(12.5)));
            ui.add_space(2.0);
            add(ui);
        });
    ui.add_space(6.0);
}

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    let pal = app.palette();
    let wide = app.rpt.selector != crate::reports::Selector::None;
    egui::Panel::left("map_viewer")
        .default_size(250.0)
        .size_range(if wide { 300.0..=420.0 } else { 220.0..=340.0 })
        .resizable(true)
        .frame(egui::Frame::NONE.fill(pal.form))
        .show(ui, |ui| {
            tool_hint(app, ui);
            crate::reports::selector(app, ui);
            panel_header(ui, "Map Viewer", pal.menu);
            // The Overview Map takes the room the panel's contents leave (their
            // height is measured each frame), so that no scroll bar is needed.
            let content_id = egui::Id::new("map_viewer_content_height");
            let content_h: f32 = ui.ctx().data(|d| d.get_temp(content_id)).unwrap_or(520.0);
            if app.prefs.show_overview {
                let h = (ui.available_height() - content_h - 4.0).max(120.0);
                egui::Panel::bottom("overview_map")
                    .exact_size(h)
                    .frame(egui::Frame::NONE.fill(pal.form).inner_margin(6))
                    .show(ui, |ui| overview(app, ui));
            }
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    let h = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| contents(app, ui))
                        .response
                        .rect
                        .height();
                    ui.ctx().data_mut(|d| d.insert_temp(content_id, h));
                });
        });
}

/// A theme drop-down. The list is tall enough to show every theme without a
/// scroll bar and all its rows are the same widget, so it keeps a steady size
/// (a scroll bar coming and going made it jitter). Result themes are
/// disabled until there are results.
fn theme_combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    current: String,
    items: &[(T, String, bool)],
    split: usize,
) -> f32 {
    egui::ComboBox::from_id_salt(id)
        .selected_text(crate::i18n::tr(&current))
        .width(130.0)
        .height(f32::INFINITY)
        .show_ui(ui, |ui| {
            ui.set_min_width(150.0);
            for (k, (t, label, enabled)) in items.iter().enumerate() {
                // the EPANET-MSX species follow the other themes
                if k == split {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("EPANET-MSX species"))
                            .weak()
                            .small(),
                    );
                }
                let t = *t;
                let resp = ui
                    .add_enabled(
                        *enabled,
                        egui::Button::selectable(*value == t, crate::i18n::tr(label)),
                    )
                    .on_disabled_hover_text(crate::i18n::tr("Run an analysis first"));
                if resp.clicked() {
                    *value = t;
                }
            }
        })
        .response
        .rect
        .right()
}

fn contents(app: &mut EpanetApp, ui: &mut Ui) {
    group_box(ui, "View", |ui| {
        ui.horizontal(|ui| {
            crate::widgets::radio_value(ui, &mut app.view_mode, ViewMode::Map, "Map");
            ui.add_space(16.0);
            crate::widgets::radio_value(ui, &mut app.view_mode, ViewMode::Report, "Report");
        });
    });

    group_box(ui, "Themes", |ui| {
        ui.label(crate::i18n::tr("Nodes"));
        // the pencil below the right end of the drop-down, as in EPANET-UI
        let combo_right = ui
            .horizontal(|ui| {
                let has_results = app.results.is_some();
                let items: Vec<(NodeTheme, String, bool)> = app
                    .node_themes()
                    .into_iter()
                    .map(|t| (t, app.node_theme_name(t), !t.is_result() || has_results))
                    .collect();
                let current = app.node_theme_name(app.prefs.node_theme);
                let right = theme_combo(
                    ui,
                    "node_theme",
                    &mut app.prefs.node_theme,
                    current,
                    &items,
                    items
                        .iter()
                        .filter(|i| !matches!(i.0, NodeTheme::Msx(_)))
                        .count(),
                );
                ui.label(RichText::new(app.node_theme_units(app.prefs.node_theme)).weak());
                right
            })
            .inner;
        ui.horizontal(|ui| {
            crate::widgets::checkbox(ui, &mut app.prefs.node_legend_visible, "Legend");
            ui.add_space((combo_right - PENCIL - ui.cursor().left()).max(4.0));
            if ui
                .add(pencil_button())
                .on_hover_text(crate::i18n::tr("Edit the node legend"))
                .clicked()
            {
                crate::dialogs::open_legend_editor(app, true);
            }
        });
        ui.add_space(4.0);
        ui.label(crate::i18n::tr("Links"));
        let combo_right = ui
            .horizontal(|ui| {
                let has_results = app.results.is_some();
                let items: Vec<(LinkTheme, String, bool)> = app
                    .link_themes()
                    .into_iter()
                    .map(|t| (t, app.link_theme_name(t), !t.is_result() || has_results))
                    .collect();
                let current = app.link_theme_name(app.prefs.link_theme);
                let right = theme_combo(
                    ui,
                    "link_theme",
                    &mut app.prefs.link_theme,
                    current,
                    &items,
                    items
                        .iter()
                        .filter(|i| !matches!(i.0, LinkTheme::Msx(_)))
                        .count(),
                );
                ui.label(RichText::new(app.link_theme_units(app.prefs.link_theme)).weak());
                right
            })
            .inner;
        ui.horizontal(|ui| {
            crate::widgets::checkbox(ui, &mut app.prefs.link_legend_visible, "Legend");
            ui.add_space((combo_right - PENCIL - ui.cursor().left()).max(4.0));
            if ui
                .add(pencil_button())
                .on_hover_text(crate::i18n::tr("Edit the link legend"))
                .clicked()
            {
                crate::dialogs::open_legend_editor(app, false);
            }
        });
    });

    // Like EPANET-UI, the Time group only appears once there are results.
    if let Some(res) = &app.results {
        let start = eui_core::analysis::start_clocktime(&app.net);
        group_box(ui, "Time", |ui| {
            let n = res.n_periods;
            let t = res.period_time(app.period);
            let label = if res.duration == 0 || n <= 1 {
                crate::i18n::tr("Single period").to_string()
            } else {
                format!(
                    "{}  ({})",
                    format_hours(t),
                    eui_core::analysis::clock_time(start + t)
                )
            };
            ui.label(label);
            if n > 1 {
                let p = app.period;
                ui.spacing_mut().slider_width = ui.available_width() - 10.0;
                let mut v = p as f64;
                crate::widgets::slider(ui, true, &mut v, 0.0..=(n - 1) as f64, false, Some(1.0));
                app.period = v.round() as usize;
            }
            ui.add_space(4.0);
            ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(40.0);
            ui.horizontal(|ui| {
                let label = crate::i18n::tr(if app.animating { "Stop" } else { "Animate" });
                if ui
                    .add_enabled(n > 1, egui::Button::new(label).min_size(vec2(84.0, 28.0)))
                    .clicked()
                {
                    app.animating = !app.animating;
                }
                let mut fps = app.prefs.animate_fps as f64;
                let r = crate::widgets::slider(ui, n > 1, &mut fps, 0.5..=20.0, true, None);
                app.prefs.animate_fps = fps as f32;
                r.on_hover_text(crate::i18n::tr("Animation speed"));
            });
        });
    }

    group_box(ui, "Layers", |ui| {
        crate::widgets::checkbox(ui, &mut app.prefs.show_junctions, "Junctions");
        crate::widgets::checkbox(ui, &mut app.prefs.show_tanks, "Tanks/Reservoirs");
        crate::widgets::checkbox(ui, &mut app.prefs.show_links, "Links");
        crate::widgets::checkbox(ui, &mut app.prefs.show_pumps, "Pumps");
        crate::widgets::checkbox(ui, &mut app.prefs.show_valves, "Valves");
        crate::widgets::checkbox(ui, &mut app.prefs.show_map_labels, "Map Labels");
        ui.add_enabled(
            app.prefs.basemap != crate::basemap::BasemapKind::None,
            crate::widgets::Checkbox::new(&mut app.prefs.show_basemap, "Basemap"),
        )
        .on_disabled_hover_text(crate::i18n::tr(
            "Add a basemap from the Map toolbar (Basemap > Load)",
        ));
        crate::widgets::checkbox(ui, &mut app.prefs.show_overview, "Overview Map");
    });
}

const PENCIL: f32 = 28.0;

/// The small pencil button beside "Legend".
fn pencil_button() -> impl egui::Widget {
    |ui: &mut Ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(PENCIL, PENCIL), Sense::click());
        let v = ui.visuals();
        ui.painter().rect(
            rect,
            2.0,
            if resp.hovered() {
                v.widgets.hovered.weak_bg_fill
            } else {
                v.extreme_bg_color
            },
            v.widgets.inactive.bg_stroke,
            egui::StrokeKind::Inside,
        );
        crate::icons::paint(
            ui.painter(),
            crate::icons::Icon::Pencil,
            rect.center(),
            17.0,
            v.text_color(),
        );
        resp
    }
}

/// Instructions shown while adding objects, as in EPANET-UI's hint panel.
fn tool_hint(app: &EpanetApp, ui: &mut Ui) {
    let (title, steps): (&str, &[&str]) = match app.tool {
        Tool::AddNode(_) => (
            "To Add a Node:",
            &[
                "Move the pointer to the location of the new node and left-click.",
                "Repeat to add more nodes.",
                "Press Escape or right-click to quit adding nodes.",
            ],
        ),
        Tool::AddLink(_) => (
            "To Add a Link:",
            &[
                "Left-click on the start node of the link, move the pointer to the end node and left-click again.",
                "Left-click at intermediate points to add new junctions along the way. Right-click to cancel drawing the link.",
                "Repeat to add more links.",
                "Press Escape or right-click to quit adding links.",
            ],
        ),
        Tool::AddLabel => (
            "To Add a Label:",
            &[
                "Left-click at the label's location.",
                "Type in the label's text and press Enter.",
                "Repeat to add more labels.",
                "Press Escape or right-click to quit adding labels.",
            ],
        ),
        _ => return,
    };
    let bg = if ui.visuals().dark_mode {
        Color32::from_rgb(0x3a, 0x38, 0x2a)
    } else {
        crate::style::HINT_BG
    };
    egui::Frame::NONE
        .fill(bg)
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 10,
            bottom: 16,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).font(crate::fonts::bold(13.0)));
            for s in steps {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(*s).wrap());
                });
            }
        });
}

/// The Overview Map (EPANET-UI's `overviewmapframe`): the whole network in
/// gray with a red frame around the part the map shows; clicking or dragging
/// in it moves the map there.
fn overview(app: &mut EpanetApp, ui: &mut Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let dark = ui.visuals().dark_mode;
    let p = ui.painter_at(rect);
    p.rect(
        rect,
        0.0,
        if dark {
            ui.visuals().extreme_bg_color
        } else {
            Color32::WHITE
        },
        egui::Stroke::new(1.0, Color32::from_gray(170)),
        egui::StrokeKind::Inside,
    );
    let Some([x0, y0, x1, y1]) = app.bounds() else {
        return;
    };
    let inner = rect.shrink(10.0);
    let s = (inner.width() as f64 / (x1 - x0).max(1e-9))
        .min(inner.height() as f64 / (y1 - y0).max(1e-9));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let c = inner.center();
    let to_screen =
        |[x, y]: [f64; 2]| egui::pos2(c.x + ((x - cx) * s) as f32, c.y - ((y - cy) * s) as f32);
    let to_world = |q: egui::Pos2| [cx + (q.x - c.x) as f64 / s, cy - (q.y - c.y) as f64 / s];
    let line = egui::Stroke::new(1.0, Color32::from_gray(120));
    app.refresh_geometry();
    // Small links: one per pixel is all the overview can show.
    let mut specks = crate::geometry::Occupancy::new(rect, 1.0);
    for (i, l) in app.net.links.iter().enumerate() {
        let Some((a, b)) = app.geom.ends.get(i).copied().flatten() else {
            continue;
        };
        let (Some(pa), Some(pb)) = (app.net.nodes[a].pos, app.net.nodes[b].pos) else {
            continue;
        };
        if let Some(wb) = app.geom.boxes.get(i).copied().flatten()
            && (wb[2] - wb[0]).max(wb[3] - wb[1]) * s < 2.0
        {
            if specks.claim(to_screen(pa)) {
                p.line_segment([to_screen(pa), to_screen(pb)], line);
            }
            continue;
        }
        let mut pts = vec![to_screen(pa)];
        pts.extend(l.vertices.iter().map(|&v| to_screen(v)));
        pts.push(to_screen(pb));
        p.add(egui::Shape::line(pts, line));
    }
    // the part of the network the map shows
    if let Some(m) = app.export.map_rect {
        let a = to_screen(app.view.to_world(m, m.left_top()));
        let b = to_screen(app.view.to_world(m, m.right_bottom()));
        p.rect_stroke(
            egui::Rect::from_two_pos(a, b),
            0.0,
            egui::Stroke::new(1.5, Color32::from_rgb(0xe0, 0x20, 0x20)),
            egui::StrokeKind::Middle,
        );
    }
    if (resp.clicked() || resp.dragged())
        && let Some(q) = resp.interact_pointer_pos()
    {
        app.view.center_on(to_world(q));
        app.view_mode = ViewMode::Map;
    }
}
