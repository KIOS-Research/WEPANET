//! The Project Explorer panel (right), as in EPANET-UI's `projectframe.lfm`:
//! a tree of project data categories above the Property Editor.

use egui::{Color32, RichText, Ui};
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints};
use eui_core::props::{self, Element, OptionCategory, PropKind};
use eui_core::{LinkKind, LinkVar, NodeKind, NodeVar};

use crate::app::{EpanetApp, ExplorerItem};
use crate::style;
use crate::viewer::panel_header;

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    let pal = app.palette();
    egui::Panel::right("project_explorer")
        .default_size(300.0)
        .size_range(240.0..=520.0)
        .resizable(true)
        .frame(egui::Frame::NONE.fill(ui.visuals().panel_fill))
        .show(ui, |ui| {
            panel_header(ui, "Project Explorer", pal.menu);
            egui::ScrollArea::vertical()
                .id_salt("tree")
                .max_height(230.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // The tree sits on white, as in EPANET-UI.
                    egui::Frame::NONE
                        .fill(if ui.visuals().dark_mode {
                            ui.visuals().extreme_bg_color
                        } else {
                            Color32::WHITE
                        })
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            tree(app, ui)
                        });
                });
            ui.separator();
            // Reset edit buffers whenever the edited object or the project changes.
            let owner = (app.selection, app.explorer, app.generation);
            if app.buffer_owner != Some(owner) {
                app.buffers.clear();
                app.buffer_owner = Some(owner);
                app.prop_hint.clear();
            }
            hint_box(app, ui);
            egui::ScrollArea::vertical()
                .id_salt("props")
                .auto_shrink(false)
                .show(ui, |ui| editor(app, ui));
        });
}

fn tree(app: &mut EpanetApp, ui: &mut Ui) {
    let item = |ui: &mut Ui, app: &mut EpanetApp, label: &str, it: ExplorerItem| {
        // Categories with no data are grayed out, as in EPANET-UI.
        let empty = match it {
            ExplorerItem::Nodes => app.net.nodes.is_empty(),
            ExplorerItem::Links => app.net.links.is_empty(),
            ExplorerItem::Labels => app.net.raw_lines("LABELS").next().is_none(),
            _ => false,
        };
        let text = if empty {
            RichText::new(label).weak()
        } else {
            RichText::new(label)
        };
        if ui.selectable_label(app.explorer == it, text).clicked() {
            app.explorer = it;
            if it == ExplorerItem::Nodes && !matches!(app.selection, Some(Element::Node(_))) {
                app.selection = (!app.net.nodes.is_empty()).then_some(Element::Node(0));
            }
            if it == ExplorerItem::Links && !matches!(app.selection, Some(Element::Link(_))) {
                app.selection = (!app.net.links.is_empty()).then_some(Element::Link(0));
            }
        }
    };
    item(ui, app, "Title / Notes", ExplorerItem::Title);
    egui::CollapsingHeader::new("Analysis Options")
        .default_open(true)
        .show(ui, |ui| {
            for c in OptionCategory::ALL {
                item(ui, app, c.label(), ExplorerItem::Options(c));
            }
        });
    item(ui, app, "Network Nodes", ExplorerItem::Nodes);
    item(ui, app, "Network Links", ExplorerItem::Links);
    item(ui, app, "Time Patterns", ExplorerItem::Patterns);
    item(ui, app, "Data Curves", ExplorerItem::Curves);
    egui::CollapsingHeader::new("Control Actions")
        .default_open(false)
        .show(ui, |ui| {
            item(ui, app, "Simple Controls", ExplorerItem::SimpleControls);
            item(ui, app, "Rule-Based Controls", ExplorerItem::RuleControls);
        });
    item(ui, app, "Map Labels", ExplorerItem::Labels);
}

fn hint_box(app: &EpanetApp, ui: &mut Ui) {
    egui::Panel::bottom("prop_hint")
        .exact_size(44.0)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let text = if app.prop_hint.is_empty() {
                "Press Enter to record an edit"
            } else {
                &app.prop_hint
            };
            let bg = if app.prefs.dark {
                Color32::from_rgb(0x3a, 0x38, 0x2a)
            } else {
                style::HINT_BG
            };
            egui::Frame::NONE
                .fill(bg)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_height(ui.available_height());
                    ui.label(RichText::new(text).color(if app.prefs.dark {
                        Color32::LIGHT_GRAY
                    } else {
                        Color32::from_gray(40)
                    }));
                });
        });
}

/// Header bar of the property editor, optionally with ◀ ▶ navigation.
fn item_header(ui: &mut Ui, color: Color32, title: &str, nav: bool) -> i32 {
    let mut step = 0;
    egui::Frame::NONE
        .fill(color)
        .inner_margin(egui::Margin::symmetric(4, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let arrow = |ui: &mut Ui, s: &str| {
                    ui.add(
                        egui::Button::new(RichText::new(s).color(Color32::BLACK))
                            .fill(Color32::WHITE)
                            .small(),
                    )
                };
                if nav && arrow(ui, "◀").on_hover_text("Previous").clicked() {
                    step = -1;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if nav && arrow(ui, "▶").on_hover_text("Next").clicked() {
                        step = 1;
                    }
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(title)
                                .font(crate::fonts::bold(13.5))
                                .color(Color32::WHITE),
                        )
                    });
                });
            });
        });
    step
}

fn editor(app: &mut EpanetApp, ui: &mut Ui) {
    let menu = app.palette().menu;
    match app.explorer {
        ExplorerItem::Title => {
            item_header(ui, menu, "Title / Notes", false);
            title_editor(app, ui);
        }
        ExplorerItem::Options(cat) => {
            item_header(
                ui,
                menu,
                &format!("Analysis Options - {}", cat.label()),
                false,
            );
            options_editor(app, ui, cat);
        }
        ExplorerItem::Nodes | ExplorerItem::Links => element_editor(app, ui),
        ExplorerItem::Patterns => {
            item_header(ui, menu, "Time Patterns", false);
            pattern_editor(app, ui);
        }
        ExplorerItem::Curves => {
            item_header(ui, menu, "Data Curves", false);
            curve_editor(app, ui);
        }
        ExplorerItem::SimpleControls => {
            item_header(ui, menu, "Simple Controls", false);
            section_editor(
                app,
                ui,
                "CONTROLS",
                "LINK 12 CLOSED IF NODE 23 ABOVE 140\nLINK 9 OPEN AT TIME 6",
            );
        }
        ExplorerItem::RuleControls => {
            item_header(ui, menu, "Rule-Based Controls", false);
            section_editor(
                app,
                ui,
                "RULES",
                "RULE 1\nIF TANK 1 LEVEL ABOVE 19\nTHEN PUMP 335 STATUS IS CLOSED",
            );
        }
        ExplorerItem::Labels => {
            item_header(ui, menu, "Map Labels", false);
            section_editor(
                app,
                ui,
                "LABELS",
                "X-Coord  Y-Coord  \"Label text\"  [Anchor node]",
            );
        }
    }
}

// ----- property grid ----------------------------------------------------------

/// One property row. Returns the committed new value (on Enter / focus loss,
/// or immediately for choices).
fn prop_row(
    app: &mut EpanetApp,
    ui: &mut Ui,
    name: &str,
    value: &str,
    kind: PropKind,
    units: &str,
) -> Option<String> {
    ui.label(name);
    let mut out = None;
    match kind {
        PropKind::ReadOnly => {
            ui.label(RichText::new(value).weak());
        }
        PropKind::Choice(choices) => {
            let shown = if value.is_empty() { "(none)" } else { value };
            egui::ComboBox::from_id_salt(("choice", name))
                .selected_text(shown)
                .width(120.0)
                .show_ui(ui, |ui| {
                    for c in choices {
                        let label = if c.is_empty() { "(none)" } else { c };
                        if ui
                            .selectable_label(value.eq_ignore_ascii_case(c), label)
                            .clicked()
                            && !value.eq_ignore_ascii_case(c)
                        {
                            out = Some(c.to_string());
                        }
                    }
                });
        }
        PropKind::Text | PropKind::Number => {
            let buf = app
                .buffers
                .entry(name.to_string())
                .or_insert_with(|| value.to_string());
            let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(120.0));
            if resp.lost_focus() {
                if *buf != value {
                    let v = buf.trim().to_string();
                    if kind == PropKind::Number && !v.is_empty() && v.parse::<f64>().is_err() {
                        app.prop_hint = format!("\"{v}\" is not a valid number for {name}");
                        *buf = value.to_string();
                    } else {
                        out = Some(v);
                    }
                }
            } else if !resp.has_focus() && *buf != value {
                *buf = value.to_string();
            }
        }
    }
    ui.label(RichText::new(units).weak());
    ui.end_row();
    out
}

fn grid(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            egui::Grid::new(id)
                .num_columns(3)
                .striped(true)
                .spacing([8.0, 5.0])
                .min_col_width(40.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Property").strong());
                    ui.label(RichText::new("Value").strong());
                    ui.label("");
                    ui.end_row();
                    add(ui);
                });
        });
}

fn prop_units(app: &EpanetApp, name: &str) -> String {
    let si = app.net.is_si();
    match name {
        "Elevation" | "Initial Depth" | "Minimum Depth" | "Maximum Depth" | "Length" => {
            eui_core::units::length(si).into()
        }
        "Diameter" => eui_core::units::diameter(si).into(),
        "Base Demand" => app.net.flow_units(),
        "Minimum Volume" => (if si { "m³" } else { "ft³" }).into(),
        "Power" => (if si { "kW" } else { "hp" }).into(),
        "Leak Area" => (if si { "mm²" } else { "in²" }).into(),
        _ => String::new(),
    }
}

fn element_editor(app: &mut EpanetApp, ui: &mut Ui) {
    let menu = app.palette().menu;
    let want_nodes = app.explorer == ExplorerItem::Nodes;
    let sel = match (app.selection, want_nodes) {
        (Some(Element::Node(i)), true) if i < app.net.nodes.len() => Element::Node(i),
        (Some(Element::Link(i)), false) if i < app.net.links.len() => Element::Link(i),
        _ => {
            item_header(
                ui,
                menu,
                if want_nodes {
                    "Network Nodes"
                } else {
                    "Network Links"
                },
                false,
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(if want_nodes {
                    "  The project has no nodes."
                } else {
                    "  The project has no links."
                })
                .weak(),
            );
            return;
        }
    };
    let (title, count, idx) = match sel {
        Element::Node(i) => (
            format!("{} {}", app.net.nodes[i].kind.label(), app.net.nodes[i].id),
            app.net.nodes.len(),
            i,
        ),
        Element::Link(i) => (
            format!("{} {}", app.net.links[i].kind.label(), app.net.links[i].id),
            app.net.links.len(),
            i,
        ),
    };
    let step = item_header(ui, menu, &title, true);
    if step != 0 {
        let j = (idx as i64 + step as i64).rem_euclid(count as i64) as usize;
        app.selection = Some(match sel {
            Element::Node(_) => Element::Node(j),
            Element::Link(_) => Element::Link(j),
        });
        return;
    }

    let rows = app.net.element_props(sel);
    let mut commit: Option<(props::PropKey, &'static str, String)> = None;
    let kind_label = match sel {
        Element::Node(i) => app.net.nodes[i].kind.label(),
        Element::Link(i) => app.net.links[i].kind.label(),
    };
    grid(ui, "element_props", |ui| {
        for p in &rows {
            let units = prop_units(app, p.name);
            let label = if p.name == "ID" {
                format!("{kind_label} ID")
            } else {
                p.name.to_string()
            };
            if let Some(v) = prop_row(app, ui, &label, &p.value, p.kind, &units) {
                commit = Some((p.key, p.name, v));
            }
        }
        result_rows(app, ui, sel);
    });
    if let Some((key, name, v)) = commit {
        let mut trial = app.net.clone();
        match trial.set_prop(sel, key, &v) {
            Ok(()) => {
                app.checkpoint();
                app.net = trial;
                if key == props::PropKey::Id {
                    app.buffer_owner = None;
                }
                if key == props::PropKey::Field(0) || key == props::PropKey::Field(1) {
                    app.prop_hint.clear();
                }
            }
            Err(e) => {
                app.prop_hint = format!("{name}: {e}");
                app.buffers.remove(name);
            }
        }
    }
}

/// A computed value in the property grid, on EPANET-UI's yellow background.
fn result_cell(ui: &mut Ui, text: &str) {
    let dark = ui.visuals().dark_mode;
    egui::Frame::NONE
        .fill(if dark {
            Color32::from_rgb(0x3a, 0x38, 0x2a)
        } else {
            style::HINT_BG
        })
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.set_min_width(112.0);
            ui.label(text);
        });
}

fn result_rows(app: &EpanetApp, ui: &mut Ui, sel: Element) {
    let Some(res) = &app.results else {
        // Before a run, the computed rows read N/A.
        let names: &[&str] = match sel {
            Element::Node(i) => match app.net.nodes[i].kind {
                NodeKind::Junction => &["Total Demand", "Hydraulic Head", "Pressure"],
                NodeKind::Reservoir => &["Outflow Rate", "Hydraulic Head"],
                NodeKind::Tank => &["Inflow Rate", "Hydraulic Head", "Water Depth"],
            },
            Element::Link(_) => &["Flow Rate", "Status"],
        };
        for n in names {
            ui.label(*n);
            result_cell(ui, "N/A");
            ui.label("");
            ui.end_row();
        }
        return;
    };
    let ro = |ui: &mut Ui, name: &str, v: Option<f32>, units: String| {
        ui.label(name);
        result_cell(ui, &v.map_or("N/A".into(), |v| style::fmt_fixed(v as f64)));
        ui.label(RichText::new(units).weak());
        ui.end_row();
    };
    let quality = res.quality_flag != 0;
    match sel {
        Element::Node(i) => {
            let kind = app.net.nodes[i].kind;
            let demand_name = match kind {
                NodeKind::Junction => "Total Demand",
                NodeKind::Reservoir => "Outflow Rate",
                NodeKind::Tank => "Inflow Rate",
            };
            let d = app
                .node_result(i, NodeVar::Demand)
                .map(|d| if kind == NodeKind::Junction { d } else { -d });
            ro(ui, demand_name, d, res.node_units(NodeVar::Demand));
            let head = app.node_result(i, NodeVar::Head);
            ro(ui, "Hydraulic Head", head, res.node_units(NodeVar::Head));
            match kind {
                NodeKind::Junction => ro(
                    ui,
                    "Pressure",
                    app.node_result(i, NodeVar::Pressure),
                    res.node_units(NodeVar::Pressure),
                ),
                NodeKind::Tank => {
                    let depth = head.zip(app.net.nodes[i].num(0)).map(|(h, e)| h - e as f32);
                    ro(ui, "Water Depth", depth, res.node_units(NodeVar::Head));
                }
                NodeKind::Reservoir => {}
            }
            if quality {
                ro(
                    ui,
                    "Quality",
                    app.node_result(i, NodeVar::Quality),
                    res.node_units(NodeVar::Quality),
                );
            }
        }
        Element::Link(i) => {
            let kind = app.net.links[i].kind;
            ro(
                ui,
                "Flow Rate",
                app.link_result(i, LinkVar::Flow),
                res.link_units(LinkVar::Flow),
            );
            match kind {
                LinkKind::Pipe => {
                    ro(
                        ui,
                        "Velocity",
                        app.link_result(i, LinkVar::Velocity),
                        res.link_units(LinkVar::Velocity),
                    );
                    ro(
                        ui,
                        "Head Loss",
                        app.link_result(i, LinkVar::Headloss),
                        res.link_units(LinkVar::Headloss),
                    );
                    ro(
                        ui,
                        "Friction Factor",
                        app.link_result(i, LinkVar::Friction),
                        String::new(),
                    );
                }
                LinkKind::Pump => {
                    let gain = app.link_result(i, LinkVar::Headloss).map(|h| -h);
                    ro(
                        ui,
                        "Head Added",
                        gain,
                        eui_core::units::length(res.is_si()).into(),
                    );
                    ro(
                        ui,
                        "Speed",
                        app.link_result(i, LinkVar::Setting),
                        String::new(),
                    );
                }
                LinkKind::Valve => {
                    ro(
                        ui,
                        "Head Loss",
                        app.link_result(i, LinkVar::Headloss),
                        eui_core::units::length(res.is_si()).into(),
                    );
                    ro(
                        ui,
                        "Setting",
                        app.link_result(i, LinkVar::Setting),
                        String::new(),
                    );
                }
            }
            ui.label("Status");
            let s = app.link_result(i, LinkVar::Status).unwrap_or(-1.0) as i32;
            result_cell(
                ui,
                match s {
                    0..=2 => "Closed",
                    4 => "Active",
                    -1 => "N/A",
                    _ => "Open",
                },
            );
            ui.label("");
            ui.end_row();
            if quality {
                ro(
                    ui,
                    "Quality",
                    app.link_result(i, LinkVar::Quality),
                    res.link_units(LinkVar::Quality),
                );
            }
        }
    }
}

// ----- options, title -----------------------------------------------------------

fn options_editor(app: &mut EpanetApp, ui: &mut Ui, cat: OptionCategory) {
    let specs = props::option_specs(cat);
    let mut commit: Option<(&'static str, &'static str, String)> = None;
    let mut quality: Option<(String, String, String, String)> = None;
    grid(ui, "options", |ui| {
        if cat == OptionCategory::Quality {
            let (model, chem, units, trace) = props::quality_parts(&app.net);
            let mut q = (model.clone(), chem.clone(), units.clone(), trace.clone());
            let mut changed = false;
            if let Some(v) = prop_row(
                app,
                ui,
                "Parameter",
                &model,
                PropKind::Choice(&["None", "Chemical", "Age", "Trace"]),
                "",
            ) {
                q.0 = v;
                changed = true;
            }
            if q.0 == "Chemical" {
                if let Some(v) = prop_row(app, ui, "Chemical Name", &chem, PropKind::Text, "") {
                    q.1 = v;
                    changed = true;
                }
                if let Some(v) = prop_row(
                    app,
                    ui,
                    "Mass Units",
                    &units,
                    PropKind::Choice(&["mg/L", "ug/L"]),
                    "",
                ) {
                    q.2 = v;
                    changed = true;
                }
            }
            if q.0 == "Trace"
                && let Some(v) = prop_row(app, ui, "Trace Node", &trace, PropKind::Text, "")
            {
                q.3 = v;
                changed = true;
            }
            if changed {
                quality = Some(q);
            }
        }
        for s in &specs {
            let value = app
                .net
                .option(s.section, s.key)
                .unwrap_or_else(|| s.default.to_string());
            if let Some(v) = prop_row(app, ui, s.name, &value, s.kind, "") {
                commit = Some((s.section, s.key, v));
            }
        }
    });
    if let Some((model, chem, units, trace)) = quality {
        if model == "Trace" && !trace.is_empty() && app.net.node_index(&trace).is_none() {
            app.prop_hint = format!("Trace node \"{trace}\" does not exist");
        } else {
            app.checkpoint();
            props::set_quality_parts(&mut app.net, &model, &chem, &units, &trace);
        }
    }
    if let Some((section, key, v)) = commit {
        app.checkpoint();
        app.net.set_option(section, key, &v);
    }
    if cat == OptionCategory::Times {
        ui.label(
            RichText::new(
                "  Times are hours:minutes (e.g. 24:00) or a number with SEC / MIN / HOURS / DAYS.",
            )
            .small()
            .weak(),
        );
    }
}

fn title_editor(app: &mut EpanetApp, ui: &mut Ui) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.label("Title");
            let title = app.net.title.first().cloned().unwrap_or_default();
            let notes = app
                .net
                .title
                .iter()
                .skip(1)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let tb = app
                .buffers
                .entry("Title".into())
                .or_insert_with(|| title.clone());
            let r1 = ui.add(egui::TextEdit::singleline(tb).desired_width(f32::INFINITY));
            let new_title = tb.clone();
            ui.add_space(6.0);
            ui.label("Notes");
            let nb = app
                .buffers
                .entry("Notes".into())
                .or_insert_with(|| notes.clone());
            let r2 = ui.add(
                egui::TextEdit::multiline(nb)
                    .desired_rows(8)
                    .desired_width(f32::INFINITY),
            );
            let new_notes = nb.clone();
            if (r1.lost_focus() || r2.lost_focus()) && (new_title != title || new_notes != notes) {
                app.checkpoint();
                let clean = |s: &str| s.replace('[', "(").replace(']', ")");
                let mut lines = vec![clean(&new_title)];
                lines.extend(new_notes.lines().map(clean));
                while lines.last().is_some_and(|l| l.trim().is_empty()) && lines.len() > 1 {
                    lines.pop();
                }
                app.net.title = lines;
            }
        });
}

// ----- patterns, curves, text sections ---------------------------------------------

fn id_picker(ui: &mut Ui, salt: &str, ids: &[String], index: &mut usize) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(ids.get(*index).cloned().unwrap_or_default())
        .width(120.0)
        .show_ui(ui, |ui| {
            for (i, id) in ids.iter().enumerate() {
                ui.selectable_value(index, i, id);
            }
        });
}

fn number_cell(app: &mut EpanetApp, ui: &mut Ui, key: String, value: f64) -> Option<f64> {
    let buf = app.buffers.entry(key).or_insert_with(|| format!("{value}"));
    let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(80.0));
    if resp.lost_focus() {
        match buf.trim().parse::<f64>() {
            Ok(v) if v != value => return Some(v),
            Ok(_) => {}
            Err(_) => *buf = format!("{value}"),
        }
    } else if !resp.has_focus() && buf.trim().parse::<f64>().ok() != Some(value) {
        *buf = format!("{value}");
    }
    None
}

fn pattern_editor(app: &mut EpanetApp, ui: &mut Ui) {
    let mut pats = app.net.patterns();
    let mut changed = false;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Pattern");
                let ids: Vec<String> = pats.iter().map(|p| p.0.clone()).collect();
                let before = app.pattern_index;
                id_picker(ui, "pattern_id", &ids, &mut app.pattern_index);
                if before != app.pattern_index {
                    app.buffers.clear();
                }
                if ui.button("Add").clicked() {
                    let id = (1..)
                        .map(|i| i.to_string())
                        .find(|id| !ids.contains(id))
                        .unwrap();
                    pats.push((id, vec![1.0; 24]));
                    app.pattern_index = pats.len() - 1;
                    changed = true;
                }
                if ui
                    .add_enabled(!pats.is_empty(), egui::Button::new("Delete"))
                    .clicked()
                {
                    pats.remove(app.pattern_index.min(pats.len() - 1));
                    app.pattern_index = 0;
                    changed = true;
                }
            });
            if pats.is_empty() {
                ui.label(RichText::new("The project has no time patterns.").weak());
                return;
            }
            let k = app.pattern_index.min(pats.len() - 1);
            let step = app
                .net
                .option("TIMES", "PATTERN TIMESTEP")
                .unwrap_or_else(|| "1:00".into());
            let bars: Vec<Bar> = pats[k]
                .1
                .iter()
                .enumerate()
                .map(|(i, &v)| Bar::new(i as f64 + 0.5, v).width(0.9).fill(style::RAMP[0]))
                .collect();
            // EPANET-UI's title: "Time (Time Period = 1.00 hrs)"
            let hours = {
                let mut it = step.trim().split(':');
                let h: f64 = it.next().and_then(|v| v.trim().parse().ok()).unwrap_or(1.0);
                let m: f64 = it.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
                h + m / 60.0
            };
            Plot::new("pattern_plot")
                .height(140.0)
                .x_axis_label(format!("Time (Time Period = {hours:.2} hrs)"))
                .y_axis_label("Multiplier")
                .allow_scroll(false)
                .allow_drag(false)
                .allow_zoom(false)
                .show(ui, |p| {
                    p.bar_chart(BarChart::new("Multiplier", bars));
                });
            egui::Grid::new("pattern_grid")
                .striped(true)
                .num_columns(2)
                .show(ui, |ui| {
                    ui.strong("Period");
                    ui.strong("Multiplier");
                    ui.end_row();
                    for i in 0..pats[k].1.len() {
                        ui.label((i + 1).to_string());
                        if let Some(v) = number_cell(app, ui, format!("pat{i}"), pats[k].1[i]) {
                            pats[k].1[i] = v;
                            changed = true;
                        }
                        ui.end_row();
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("+ Period").clicked() {
                    let last = pats[k].1.last().copied().unwrap_or(1.0);
                    pats[k].1.push(last);
                    changed = true;
                }
                if ui
                    .add_enabled(pats[k].1.len() > 1, egui::Button::new("− Period"))
                    .clicked()
                {
                    pats[k].1.pop();
                    changed = true;
                }
            });
        });
    if changed {
        app.checkpoint();
        app.net.set_patterns(&pats);
        app.buffers.clear();
    }
}

fn curve_editor(app: &mut EpanetApp, ui: &mut Ui) {
    let mut curves = app.net.curves();
    let mut changed = false;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Curve");
                let ids: Vec<String> = curves.iter().map(|c| c.0.clone()).collect();
                let before = app.curve_index;
                id_picker(ui, "curve_id", &ids, &mut app.curve_index);
                if before != app.curve_index {
                    app.buffers.clear();
                }
                if ui.button("Add").clicked() {
                    let id = (1..)
                        .map(|i| i.to_string())
                        .find(|id| !ids.contains(id))
                        .unwrap();
                    curves.push((
                        id,
                        "PUMP".into(),
                        vec![[0.0, 100.0], [500.0, 80.0], [1000.0, 40.0]],
                    ));
                    app.curve_index = curves.len() - 1;
                    changed = true;
                }
                if ui
                    .add_enabled(!curves.is_empty(), egui::Button::new("Delete"))
                    .clicked()
                {
                    curves.remove(app.curve_index.min(curves.len() - 1));
                    app.curve_index = 0;
                    changed = true;
                }
            });
            if curves.is_empty() {
                ui.label(RichText::new("The project has no data curves.").weak());
                return;
            }
            let k = app.curve_index.min(curves.len() - 1);
            ui.horizontal(|ui| {
                ui.label("Type");
                egui::ComboBox::from_id_salt("curve_type")
                    .selected_text(if curves[k].1.is_empty() {
                        "GENERIC"
                    } else {
                        &curves[k].1
                    })
                    .show_ui(ui, |ui| {
                        for t in [
                            "PUMP",
                            "EFFICIENCY",
                            "VOLUME",
                            "HEADLOSS",
                            "VALVE",
                            "GENERIC",
                        ] {
                            if ui.selectable_label(curves[k].1 == t, t).clicked() {
                                curves[k].1 = t.into();
                                changed = true;
                            }
                        }
                    });
            });
            let pts: PlotPoints = curves[k].2.iter().map(|&[x, y]| [x, y]).collect();
            // axis titles by curve type, as EPANET-UI's curve editor
            let (xt, yt) = match curves[k].1.to_ascii_uppercase().as_str() {
                "VOLUME" => ("Depth", "Volume"),
                "PUMP" => ("Flow", "Head"),
                "EFFICIENCY" => ("Flow", "Efficiency"),
                "HEADLOSS" => ("Flow", "Head Loss"),
                "VALVE" => ("% Open", "% Full Flow"),
                _ => ("X", "Y"),
            };
            Plot::new("curve_plot")
                .height(160.0)
                .x_axis_label(xt)
                .y_axis_label(yt)
                .allow_scroll(false)
                .show(ui, |p| {
                    p.line(
                        Line::new(curves[k].0.clone(), pts)
                            .stroke(egui::Stroke::new(2.0, style::ACCENT)),
                    );
                });
            egui::Grid::new("curve_grid")
                .striped(true)
                .num_columns(2)
                .show(ui, |ui| {
                    ui.strong("X");
                    ui.strong("Y");
                    ui.end_row();
                    for i in 0..curves[k].2.len() {
                        for c in 0..2 {
                            if let Some(v) =
                                number_cell(app, ui, format!("cv{i}_{c}"), curves[k].2[i][c])
                            {
                                curves[k].2[i][c] = v;
                                changed = true;
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("+ Point").clicked() {
                    let last = curves[k].2.last().copied().unwrap_or([0.0, 0.0]);
                    curves[k].2.push(last);
                    changed = true;
                }
                if ui
                    .add_enabled(curves[k].2.len() > 1, egui::Button::new("− Point"))
                    .clicked()
                {
                    curves[k].2.pop();
                    changed = true;
                }
            });
        });
    if changed {
        app.checkpoint();
        app.net.set_curves(&curves);
        app.buffers.clear();
    }
}

fn section_editor(app: &mut EpanetApp, ui: &mut Ui, section: &str, example: &str) {
    let item = app.explorer;
    if app.section_buffer.as_ref().map(|(i, _)| *i) != Some(item) {
        app.section_buffer = Some((item, app.net.section_text(section)));
    }
    let current = app.net.section_text(section);
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            let Some((_, text)) = app.section_buffer.as_mut() else {
                return;
            };
            ui.add(
                egui::TextEdit::multiline(text)
                    .code_editor()
                    .desired_rows(14)
                    .desired_width(f32::INFINITY)
                    .hint_text(example),
            );
            let edited = *text != current;
            let mut apply = false;
            let mut revert = false;
            ui.horizontal(|ui| {
                apply = ui.add_enabled(edited, egui::Button::new("Apply")).clicked();
                revert = ui
                    .add_enabled(edited, egui::Button::new("Revert"))
                    .clicked();
            });
            if apply {
                let lines = text.lines().map(str::to_string).collect();
                app.checkpoint();
                app.net.replace_section(section, lines);
                app.section_buffer = None;
            } else if revert {
                app.section_buffer = None;
            }
        });
}
