//! The Project Explorer panel (right unless the user moves it), as in
//! EPANET-UI's `projectframe.lfm`: a tree of project data categories above
//! the Property Editor.

use egui::{Color32, RichText, Ui};
use eui_core::props::{self, Element, OptionCategory, PropKind};
use eui_core::{LinkKind, LinkVar, NodeKind, NodeVar};

use crate::app::{EpanetApp, ExplorerItem};
use crate::i18n::tr;
use crate::style;

/// The Project Explorer and the Property Editor, as they first are: the
/// tree above, with a splitter between them.
#[cfg(test)]
pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    use crate::dock::Pane;
    egui::Panel::right("project_explorer")
        .default_size(300.0)
        .show(ui, |ui| {
            crate::dock::stack(
                app,
                ui,
                &[
                    (vec![Pane::ProjectExplorer], Pane::ProjectExplorer),
                    (vec![Pane::PropertyEditor], Pane::PropertyEditor),
                ],
            );
        });
}

/// The Project Explorer panel (see [`crate::dock`]): the tree, on white as
/// in EPANET-UI.
pub(crate) fn tree_pane(app: &mut EpanetApp, ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .id_salt("tree")
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    tree(app, ui)
                });
        });
}

/// The Property Editor panel: what the item chosen in the tree holds.
pub(crate) fn editor_pane(app: &mut EpanetApp, ui: &mut Ui) {
    // Reset edit buffers whenever the edited object or the project changes.
    let owner = (app.selection, app.explorer, app.generation);
    if app.buffer_owner != Some(owner) {
        app.buffers.clear();
        app.buffer_owner = Some(owner);
        app.prop_hint.clear();
    }
    hint_box(app, ui);
    if ui.input(|i| i.key_pressed(egui::Key::F1)) {
        crate::help::open(ui.ctx(), help_topic(app));
    }
    egui::ScrollArea::vertical()
        .id_salt("props")
        .auto_shrink(false)
        .show(ui, |ui| editor(app, ui));
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
            RichText::new(tr(label)).weak()
        } else {
            RichText::new(tr(label))
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
    egui::CollapsingHeader::new(tr("Analysis Options"))
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
    egui::CollapsingHeader::new(tr("Control Actions"))
        .default_open(true)
        .show(ui, |ui| {
            item(ui, app, "Simple Controls", ExplorerItem::SimpleControls);
            item(ui, app, "Rule-Based Controls", ExplorerItem::RuleControls);
        });
    item(ui, app, "Map Labels", ExplorerItem::Labels);
}

/// The manual's topic for what the Property Editor shows (F1).
fn help_topic(app: &EpanetApp) -> &'static str {
    match app.explorer {
        ExplorerItem::Title => "Working with Projects",
        ExplorerItem::Options(c) => match c {
            OptionCategory::Hydraulics => "Hydraulic Options",
            OptionCategory::Demands => "Demand Options",
            OptionCategory::Quality => "Water Quality Options",
            OptionCategory::Times => "Time Options",
            OptionCategory::Energy => "Energy Options",
        },
        ExplorerItem::Nodes => match app.selection {
            Some(Element::Node(i)) => match app.net.nodes.get(i).map(|n| n.kind) {
                Some(NodeKind::Reservoir) => "Reservoir Properties",
                Some(NodeKind::Tank) => "Tank Properties",
                _ => "Junction Properties",
            },
            _ => "Junction Properties",
        },
        ExplorerItem::Links => match app.selection {
            Some(Element::Link(i)) => match app.net.links.get(i).map(|l| l.kind) {
                Some(LinkKind::Pump) => "Pump Properties",
                Some(LinkKind::Valve) => "Valve Properties",
                _ => "Pipe Properties",
            },
            _ => "Pipe Properties",
        },
        ExplorerItem::Patterns => "Time Patterns",
        ExplorerItem::Curves => "Data Curves",
        ExplorerItem::SimpleControls => "Simple Controls Editor",
        ExplorerItem::RuleControls => "Rule-Based Controls Editor",
        ExplorerItem::Labels => "Label Properties",
    }
}

fn hint_box(app: &EpanetApp, ui: &mut Ui) {
    egui::Panel::bottom("prop_hint")
        .exact_size(44.0)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let text = if app.prop_hint.is_empty() {
                tr("Press Enter to record an edit, F1 for Help")
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
                            RichText::new(tr(title))
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
                &format!("{} - {}", tr("Analysis Options"), tr(cat.label())),
                false,
            );
            options_editor(app, ui, cat);
        }
        ExplorerItem::Nodes | ExplorerItem::Links => element_editor(app, ui),
        ExplorerItem::Patterns => {
            item_header(ui, menu, "Time Patterns", false);
            crate::forms::patterns_panel(app, ui);
        }
        ExplorerItem::Curves => {
            item_header(ui, menu, "Data Curves", false);
            crate::forms::curves_panel(app, ui);
        }
        ExplorerItem::SimpleControls => {
            item_header(ui, menu, "Simple Controls", false);
            crate::forms::controls_panel(app, ui);
        }
        ExplorerItem::RuleControls => {
            item_header(ui, menu, "Rule-Based Controls", false);
            crate::forms::rules_panel(app, ui);
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
    ui.label(tr(name));
    let mut out = None;
    match kind {
        PropKind::ReadOnly => {
            ui.label(RichText::new(style::show_num(value)).weak());
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
            let shown = if kind == PropKind::Number {
                style::show_num(value)
            } else {
                value.to_string()
            };
            let buf = app
                .buffers
                .entry(name.to_string())
                .or_insert_with(|| shown.clone());
            let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(120.0));
            if resp.lost_focus() {
                if *buf != shown {
                    let v = buf.trim().to_string();
                    if kind != PropKind::Number {
                        out = Some(v);
                    } else if !v.is_empty() && !eui_core::num::is_number(&v) {
                        app.prop_hint = format!("\"{v}\" is not a valid number for {name}");
                        *buf = shown;
                    } else {
                        out = Some(eui_core::num::normalize(&v));
                    }
                }
            } else if !resp.has_focus() && *buf != shown {
                *buf = shown;
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
                    ui.label(RichText::new(tr("Property")).strong());
                    ui.label(RichText::new(tr("Value")).strong());
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
    // EPANET-MSX models the water quality instead of EPANET: its single
    // species quality properties are left out.
    let msx = app.msx_active();
    let mut msx_change: Option<MsxChange> = None;
    grid(ui, "element_props", |ui| {
        for p in &rows {
            if msx && QUALITY_PROPS.contains(&p.name) {
                continue;
            }
            let units = prop_units(app, p.name);
            let label = if p.name == "ID" {
                format!("{kind_label} ID")
            } else {
                p.name.to_string()
            };
            if p.key == props::PropKey::KeyedRest("SOURCES")
                && let Element::Node(i) = sel
            {
                // EPANET-UI's Source Editor
                if source_row(ui, &label, &p.value) {
                    crate::source::open(app, crate::source::Target::Quality(i));
                }
                continue;
            }
            // patterns and curves are chosen in EPANET-UI's selectors
            let pick = crate::forms::Pick::Prop(sel, p.key);
            if PATTERN_PROPS.contains(&p.name) {
                if pick_row(ui, &label, &p.value) {
                    crate::forms::open_patterns(app, pick);
                }
                continue;
            }
            if CURVE_PROPS.contains(&p.name) {
                if pick_row(ui, &label, &p.value) {
                    crate::forms::open_curves(app, pick);
                }
                continue;
            }
            if let Some(v) = prop_row(app, ui, &label, &p.value, p.kind, &units) {
                commit = Some((p.key, p.name, v));
            }
        }
        if msx {
            msx_change = msx_rows(app, ui, sel);
        }
        result_rows(app, ui, sel);
    });
    if let Some(c) = msx_change {
        apply_msx_change(app, sel, c);
    }
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
                app.prop_hint = format!("{}: {}", tr(name), tr(&e));
                app.buffers.remove(name);
            }
        }
    }
}

/// EPANET's single species water quality properties, left out while
/// EPANET-MSX models the water quality.
const QUALITY_PROPS: [&str; 5] = [
    "Initial Quality",
    "Source Quality",
    "Bulk Coeff.",
    "Wall Coeff.",
    "Reaction Coeff.",
];

/// Element properties chosen in the Time Pattern Selector.
const PATTERN_PROPS: [&str; 4] = [
    "Demand Pattern",
    "Elev. Pattern",
    "Speed Pattern",
    "Price Pattern",
];

/// Element properties chosen in the Data Curve Selector.
const CURVE_PROPS: [&str; 3] = ["Volume Curve", "Pump Curve", "Effic. Curve"];

/// A property chosen in a selector: its value, and a `…` button that
/// opens the selector. Returns true when clicked.
fn pick_row(ui: &mut Ui, label: &str, value: &str) -> bool {
    ui.label(label);
    let clicked = ui
        .horizontal(|ui| {
            ui.add(egui::Label::new(value).truncate())
                .on_hover_text(value);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small_button("…").on_hover_text("Select").clicked()
            })
            .inner
        })
        .inner;
    ui.label("");
    ui.end_row();
    clicked
}

/// A source property: its value on a button that opens the Source Editor.
/// Returns true when clicked.
fn source_row(ui: &mut Ui, label: &str, value: &str) -> bool {
    ui.label(label);
    let shown = if value.trim().is_empty() {
        "(none)".to_string()
    } else {
        value.to_string()
    };
    let clicked = ui
        .add(egui::Button::new(format!("{shown}  …")).min_size(egui::vec2(120.0, 0.0)))
        .on_hover_text("Edit the source: strength, time pattern and type")
        .clicked();
    ui.label("");
    ui.end_row();
    clicked
}

/// A change made in the EPANET-MSX rows of the Property Editor.
enum MsxChange {
    Initial(String, Option<f64>),
    Parameter(String, Option<f64>),
    Source(String),
}

/// The EPANET-MSX rows of a node or pipe: per species its initial
/// concentration and (nodes) its source, then the model's parameters (pipes
/// and tanks).
fn msx_rows(app: &mut EpanetApp, ui: &mut Ui, sel: Element) -> Option<MsxChange> {
    let file = app.net.msx.clone()?;
    let model = file.model();
    let (id, node, tank, pipe) = match sel {
        Element::Node(i) => (
            app.net.nodes[i].id.clone(),
            true,
            app.net.nodes[i].kind == NodeKind::Tank,
            false,
        ),
        Element::Link(i) => (
            app.net.links[i].id.clone(),
            false,
            false,
            app.net.links[i].kind == LinkKind::Pipe,
        ),
    };
    if !node && !pipe {
        return None;
    }
    ui.label(RichText::new("EPANET-MSX").strong());
    ui.label("");
    ui.label("");
    ui.end_row();
    let mut change = None;
    for (s, sp) in model.species.iter().enumerate() {
        if node && sp.wall {
            continue; // wall species live on pipe walls
        }
        let value = if node {
            file.node_initial(&id, &sp.id)
        } else {
            file.link_initial(&id, &sp.id)
        }
        .unwrap_or_default();
        let units = model.units(s);
        let name = format!("Initial {}", sp.id);
        if let Some(v) = prop_row(app, ui, &name, &value, PropKind::Number, &units) {
            change = Some(MsxChange::Initial(sp.id.clone(), v.parse().ok()));
        }
        if node {
            let source = file.source(&id, &sp.id).unwrap_or_default();
            if source_row(ui, &format!("Source {}", sp.id), &source) {
                change = Some(MsxChange::Source(sp.id.clone()));
            }
        }
    }
    if tank || pipe {
        for p in model.parameters() {
            let value = file.parameter(tank, &id, &p.id).unwrap_or_default();
            let name = format!("Parameter {}", p.id);
            if let Some(v) = prop_row(app, ui, &name, &value, PropKind::Number, "") {
                change = Some(MsxChange::Parameter(p.id.clone(), v.parse().ok()));
            }
        }
    }
    change
}

fn apply_msx_change(app: &mut EpanetApp, sel: Element, c: MsxChange) {
    let (id, node, tank) = match sel {
        Element::Node(i) => (
            app.net.nodes[i].id.clone(),
            true,
            app.net.nodes[i].kind == NodeKind::Tank,
        ),
        Element::Link(i) => (app.net.links[i].id.clone(), false, false),
    };
    let result = match c {
        MsxChange::Source(sp) => {
            if let Element::Node(i) = sel {
                crate::source::open(app, crate::source::Target::Species(i, sp));
            }
            Ok(())
        }
        MsxChange::Initial(sp, v) => crate::msx::edit(app, |m| {
            if node {
                m.set_node_initial(&id, &sp, v);
            } else {
                m.set_link_initial(&id, &sp, v);
            }
            Ok(())
        }),
        MsxChange::Parameter(p, v) => crate::msx::edit(app, |m| {
            m.set_parameter(tank, &id, &p, v);
            Ok(())
        }),
    };
    if let Err(e) = result {
        app.prop_hint = e;
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
            ui.label(tr(n));
            result_cell(ui, "N/A");
            ui.label("");
            ui.end_row();
        }
        return;
    };
    let ro = |ui: &mut Ui, name: &str, v: Option<f32>, units: String| {
        ui.label(tr(name));
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
            ui.label(tr("Status"));
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
    // EPANET-MSX species at the time shown
    if let Some(m) = &res.msx {
        let (nodes, r) = match sel {
            Element::Node(i) => (true, app.node_result_index(i)),
            Element::Link(i) => (false, app.link_result_index(i)),
        };
        let Some(r) = r else {
            return;
        };
        for s in crate::msx::result_species(Some(res), nodes) {
            let v = if nodes {
                m.node(app.period, s, r)
            } else {
                m.link(app.period, s, r)
            };
            ro(ui, &m.species()[s].id, v, m.units(s));
        }
    }
}

// ----- options, title -----------------------------------------------------------

/// EPANET-UI's two-column Property / Value grid of the Analysis Options.
fn options_grid(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            egui::Grid::new("options")
                .num_columns(2)
                .striped(true)
                .spacing([10.0, 6.0])
                .min_col_width(150.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(tr("Property")).strong());
                    ui.label(RichText::new(tr("Value")).strong());
                    ui.end_row();
                    add(ui);
                });
        });
}

/// How an option's value is shown: times as `HH:MM`, prices and charges to
/// four decimals, the pump efficiency to two, keywords by their names.
fn option_shown(key: &str, value: &str) -> String {
    let num = |d: usize| {
        value.trim().parse::<f64>().map_or_else(
            |_| value.to_string(),
            |v| style::show_num(&format!("{v:.d$}")),
        )
    };
    match key {
        "DURATION" | "HYDRAULIC TIMESTEP" | "QUALITY TIMESTEP" | "PATTERN TIMESTEP"
        | "PATTERN START" | "REPORT TIMESTEP" | "REPORT START" | "RULE TIMESTEP"
        | "START CLOCKTIME" => eui_core::analysis::format_time_option(value),
        "GLOBAL EFFICIENCY" => num(2),
        "GLOBAL PRICE" | "DEMAND CHARGE" => num(4),
        _ => style::show_num(value),
    }
}

/// The names EPANET-UI gives to option keywords.
fn choice_label(key: &str, value: &str) -> String {
    match (key, value.to_ascii_uppercase().as_str()) {
        ("STATUS", "NO") => "None".into(),
        ("STATUS", "YES") => "Normal".into(),
        ("STATUS", "FULL") => "Full".into(),
        (_, "") => "(none)".into(),
        (_, v) => {
            let mut c = v.chars();
            match c.next() {
                Some(f) if v.len() > 3 => format!("{f}{}", c.as_str().to_ascii_lowercase()),
                _ => v.to_string(),
            }
        }
    }
}

/// An option row chosen in the Time Pattern Selector.
const PATTERN_OPTIONS: [&str; 2] = ["PATTERN", "GLOBAL PATTERN"];

fn options_editor(app: &mut EpanetApp, ui: &mut Ui, cat: OptionCategory) {
    let specs = props::option_specs(cat);
    let mut commit: Option<(&'static str, &'static str, String)> = None;
    options_grid(ui, |ui| {
        if cat == OptionCategory::Quality {
            // EPANET-UI: a row for each water quality model, each opening
            // its editor
            let msx = app.msx_active();
            let (model, chem, _, trace) = props::quality_parts(&app.net);
            let single = match model.as_str() {
                "Chemical" => chem,
                "Trace" => format!("Trace {trace}"),
                m => m.to_string(),
            };
            let single = if msx {
                format!("{single} (not used)")
            } else {
                single
            };
            ui.label("Single Species Model");
            if model_button(ui, &single) {
                crate::forms::open_quality(app);
            }
            ui.end_row();
            ui.label("Multi-Species Model");
            let multi = match &app.net.msx {
                Some(m) if app.msx.enabled => m.name.clone(),
                Some(m) => format!("{} (off)", m.name),
                None => "(none)".into(),
            };
            if model_button(ui, &multi) {
                crate::msx::open_editor(app);
            }
            ui.end_row();
            return;
        }
        for s in &specs {
            let value = app
                .net
                .option(s.section, s.key)
                .unwrap_or_else(|| s.default.to_string());
            ui.label(s.name);
            if PATTERN_OPTIONS.contains(&s.key) {
                ui.horizontal(|ui| {
                    ui.label(&value);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("…")
                            .on_hover_text("Select a pattern")
                            .clicked()
                        {
                            crate::forms::open_patterns(
                                app,
                                crate::forms::Pick::Option(s.section, s.key),
                            );
                        }
                    });
                });
                ui.end_row();
                continue;
            }
            match s.kind {
                PropKind::Choice(choices) => {
                    egui::ComboBox::from_id_salt(("option", s.key))
                        .selected_text(choice_label(s.key, &value))
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for c in choices {
                                if ui
                                    .selectable_label(
                                        value.eq_ignore_ascii_case(c),
                                        choice_label(s.key, c),
                                    )
                                    .clicked()
                                    && !value.eq_ignore_ascii_case(c)
                                {
                                    commit = Some((s.section, s.key, c.to_string()));
                                }
                            }
                        });
                }
                _ => {
                    let shown = option_shown(s.key, &value);
                    let buf = app
                        .buffers
                        .entry(s.name.to_string())
                        .or_insert_with(|| shown.clone());
                    let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(140.0));
                    if resp.lost_focus() {
                        let v = buf.trim().to_string();
                        if v != shown {
                            if s.kind == PropKind::Number
                                && !v.is_empty()
                                && !eui_core::num::is_number(&v)
                            {
                                app.prop_hint =
                                    format!("\"{v}\" is not a valid number for {}", s.name);
                                *buf = shown;
                            } else {
                                commit = Some((s.section, s.key, eui_core::num::normalize(&v)));
                            }
                        }
                    } else if !resp.has_focus() && *buf != shown {
                        *buf = shown;
                    }
                }
            }
            ui.end_row();
        }
    });
    if let Some((section, key, v)) = commit {
        match app.net.check_option(section, key, &v) {
            Ok(()) => {
                app.checkpoint();
                app.net.set_option(section, key, &v);
            }
            Err(e) => {
                let name = specs.iter().find(|s| s.key == key).map_or(key, |s| s.name);
                app.prop_hint = format!("{}: {}", tr(name), tr(&e));
                app.buffers.remove(name);
            }
        }
    }
    if cat == OptionCategory::Times {
        ui.label(
            RichText::new(
                "  Times are hours:minutes (e.g. 24:00); Duration may also be decimal hours.",
            )
            .small()
            .weak(),
        );
    }
}

/// A water quality model row's value: a button that opens its editor.
fn model_button(ui: &mut Ui, text: &str) -> bool {
    ui.horizontal(|ui| {
        ui.label(text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small_button("…")
                .on_hover_text("Open its editor")
                .clicked()
        })
        .inner
    })
    .inner
}

fn title_editor(app: &mut EpanetApp, ui: &mut Ui) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.label(tr("Title"));
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
            ui.label(tr("Notes"));
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

// ----- text sections ----------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The Property Editor as drawn: its pieces of text.
    fn texts(app: &mut EpanetApp, ctx: &egui::Context) -> Vec<String> {
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 2400.0),
            )),
            ..Default::default()
        };
        let mut texts = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(input(), |ui| show(app, ui));
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect();
        }
        texts
    }

    /// With EPANET-MSX on, a node shows the species' initial concentrations
    /// and sources instead of EPANET's single species quality.
    #[test]
    fn msx_replaces_single_species_quality_in_the_property_editor() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        crate::msx::open_example(&mut app);
        app.explorer = ExplorerItem::Nodes;
        app.selection = Some(Element::Node(app.net.node_index("Source").unwrap()));
        let t = texts(&mut app, &ctx);
        for s in ["EPANET-MSX", "Initial AS3", "Source NH2CL", "(none)  …"] {
            assert!(t.iter().any(|x| x == s), "{s} missing from {t:?}");
        }
        assert!(
            !t.iter()
                .any(|x| x == "Initial Quality" || x == "Source Quality")
        );
        assert!(
            !t.iter().any(|x| x == "Initial AS5s"),
            "no wall species at nodes"
        );
        // a pipe: every species, and no EPANET reaction coefficients
        app.explorer = ExplorerItem::Links;
        app.selection = Some(Element::Link(app.net.link_index("5").unwrap()));
        let t = texts(&mut app, &ctx);
        assert!(t.iter().any(|x| x == "Initial AS5s"), "{t:?}");
        assert!(!t.iter().any(|x| x == "Bulk Coeff."));
        // EPANET-MSX off: EPANET's quality is back
        app.msx.enabled = false;
        app.explorer = ExplorerItem::Nodes;
        app.selection = Some(Element::Node(app.net.node_index("Source").unwrap()));
        let t = texts(&mut app, &ctx);
        assert!(t.iter().any(|x| x == "Initial Quality"), "{t:?}");
        assert!(t.iter().any(|x| x == "Source Quality"));
        assert!(!t.iter().any(|x| x == "Initial AS3"));
    }
}

#[cfg(test)]
mod tree_tests {
    use super::*;

    /// The whole tree shows, Control Actions open, nothing scrolled away.
    #[test]
    fn the_tree_shows_every_category() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.load_example("Net1", eui_core::examples::NET1);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut visible = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(input(), |ui| show(&mut app, ui));
            out.textures_delta.clear();
            visible = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t)
                        if c.clip_rect.contains_rect(c.shape.visual_bounding_rect()) =>
                    {
                        Some(t.galley.text().to_string())
                    }
                    _ => None,
                })
                .collect();
        }
        for s in [
            "Title / Notes",
            "Hydraulics",
            "Energy",
            "Network Nodes",
            "Data Curves",
            "Simple Controls",
            "Rule-Based Controls",
            "Map Labels",
        ] {
            assert!(
                visible.iter().any(|x| x == s),
                "{s} not fully visible: {visible:?}"
            );
        }
    }
}

#[cfg(test)]
mod switch_tests {
    use super::*;
    use eframe::App as _;

    /// Data Curves after Time Patterns is laid out right from its first
    /// frame: the panel does not widen and settle again.
    #[test]
    fn curves_after_patterns_do_not_jump() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.load_example("Net1", eui_core::examples::NET1);
        let mut frame = eframe::Frame::_new_kittest();
        let mut events = Vec::new();
        let click = |at: egui::Pos2, events: &mut Vec<egui::Event>, pressed| {
            events.push(egui::Event::PointerMoved(at));
            events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        };
        let mut frames = Vec::new();
        for f in 0..14 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx(), &mut frame);
                app.ui(ui, &mut frame);
            });
            out.textures_delta.clear();
            let texts: Vec<(String, egui::Rect)> = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => {
                        Some((t.galley.text().to_string(), t.visual_bounding_rect()))
                    }
                    _ => None,
                })
                .collect();
            let at = |s: &str| texts.iter().find(|t| t.0 == s).map(|t| t.1);
            match f {
                2 | 3 => click(at("Time Patterns").unwrap().center(), &mut events, f == 2),
                8 | 9 => click(at("Data Curves").unwrap().center(), &mut events, f == 8),
                _ => {}
            }
            if app.explorer == ExplorerItem::Curves {
                frames.push((at("Data Curves").unwrap(), at("Description").unwrap()));
            }
        }
        assert!(frames.len() > 2);
        for w in frames.windows(2) {
            assert_eq!(w[0], w[1], "the panel moved");
        }
        assert!(frames[0].1.max.x <= 1400.0, "{:?}", frames[0]);
    }
}
