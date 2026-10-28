//! Group Edit and Group Delete (Edit menu), as in EPANET-UI's Group Selector
//! and Group Editor. Group Edit picks junctions or pipes (clicked on the map,
//! by Tag, in a region, or all) and changes one of their properties at once,
//! optionally only where another property passes a filter. Group Delete
//! deletes the nodes (with their links) and the labels in a region.
//!
//! A region is drawn on the map as a polygon, clicking its corners, or as a
//! rectangle, dragging it; Enter without corners takes the whole network.

use std::collections::BTreeSet;

use egui::{Color32, Pos2, Rect, RichText, Stroke, Ui, vec2};
use eui_core::props::{Element, PropKey};
use eui_core::{LinkKind, NodeKind};

use crate::app::EpanetApp;
use crate::i18n::{tr, trf};
use crate::map::MapView;

/// A region being drawn on the map.
#[derive(Default)]
pub(crate) struct Region {
    /// The polygon's corners clicked so far (world coordinates).
    corners: Vec<[f64; 2]>,
    /// Where the rectangle being dragged started (world coordinates).
    from: Option<[f64; 2]>,
}

/// What a region made of the map input.
pub(crate) enum Drawn {
    /// Not its input: the map handles it.
    Ignored,
    /// Taken, the region not finished yet.
    Taken,
    /// Finished: the polygon's corners, in order.
    Closed(Vec<[f64; 2]>),
    /// Enter without corners: the whole network.
    Whole,
}

impl Region {
    /// Clicks add a corner to the polygon, which a right-click, Enter or a
    /// click on its first corner closes; a drag draws a rectangle.
    pub(crate) fn input(&mut self, view: &MapView, rect: Rect, response: &egui::Response) -> Drawn {
        let ctx = &response.ctx;
        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            return match self.corners.len() {
                0 => Drawn::Whole,
                1 | 2 => Drawn::Taken,
                _ => Drawn::Closed(std::mem::take(&mut self.corners)),
            };
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            let start = ctx.input(|i| i.pointer.press_origin());
            self.from = start.map(|p| view.to_world(rect, p));
            self.corners.clear();
            return Drawn::Taken;
        }
        if let Some(from) = self.from {
            if response.drag_stopped() {
                self.from = None;
                let Some(p) = response.interact_pointer_pos().or(response.hover_pos()) else {
                    return Drawn::Taken;
                };
                let to = view.to_world(rect, p);
                // too small to be meant
                if view.to_screen(rect, from).distance(p) < 4.0 {
                    return Drawn::Taken;
                }
                return Drawn::Closed(vec![from, [to[0], from[1]], to, [from[0], to[1]]]);
            }
            return Drawn::Taken;
        }
        if response.secondary_clicked() {
            if self.corners.len() >= 3 {
                return Drawn::Closed(std::mem::take(&mut self.corners));
            }
            self.corners.clear();
            return Drawn::Taken;
        }
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let on_first = self
                .corners
                .first()
                .is_some_and(|&f| view.to_screen(rect, f).distance(p) < 8.0);
            if on_first && self.corners.len() >= 3 {
                return Drawn::Closed(std::mem::take(&mut self.corners));
            }
            self.corners.push(view.to_world(rect, p));
            return Drawn::Taken;
        }
        if response.dragged() {
            return Drawn::Taken;
        }
        Drawn::Ignored
    }

    /// The polygon's corners and edges so far, on to the pointer, or the
    /// rectangle being dragged.
    pub(crate) fn draw(
        &self,
        view: &MapView,
        painter: &egui::Painter,
        rect: Rect,
        hover: Option<Pos2>,
    ) {
        let stroke = Stroke::new(1.5, crate::style::ACCENT);
        if let (Some(from), Some(h)) = (self.from, hover) {
            let r = Rect::from_two_pos(view.to_screen(rect, from), h);
            painter.rect(
                r,
                0.0,
                crate::style::ACCENT.gamma_multiply(0.12),
                stroke,
                egui::StrokeKind::Middle,
            );
            return;
        }
        let mut pts: Vec<Pos2> = self
            .corners
            .iter()
            .map(|&w| view.to_screen(rect, w))
            .collect();
        for &s in &pts {
            painter.rect_filled(Rect::from_center_size(s, vec2(6.0, 6.0)), 0.0, stroke.color);
        }
        if let Some(h) = hover
            && !pts.is_empty()
        {
            pts.push(h);
        }
        if pts.len() >= 3 {
            painter.add(egui::Shape::dashed_line(
                &[pts[pts.len() - 1], pts[0]],
                stroke,
                6.0,
                4.0,
            ));
        }
        painter.add(egui::Shape::line(pts, stroke));
    }
}

/// Is a point inside a polygon (even-odd rule)?
pub(crate) fn inside(poly: &[[f64; 2]], [x, y]: [f64; 2]) -> bool {
    let mut odd = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ([xi, yi], [xj, yj]) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < xi + (y - yi) / (yj - yi) * (xj - xi) {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// The hint shown while a region is drawn.
pub(crate) const REGION_HINT: &str = "Click the corners of a polygon on the map, then right-click or press Enter to close it; or drag a rectangle.";

/// What the group is for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    EditNodes,
    EditLinks,
    Delete,
}

/// Group Edit's two steps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Select,
    Edit,
}

/// The objects in the region, when Group Delete has one.
struct Doomed {
    nodes: Vec<usize>,
    /// Lines of [LABELS] inside the region.
    labels: usize,
    region: Option<Vec<[f64; 2]>>,
}

/// An open Group Edit or Group Delete.
pub(crate) struct Group {
    purpose: Purpose,
    step: Step,
    /// The nodes or links selected.
    selected: BTreeSet<usize>,
    tag: String,
    region: Option<Region>,
    doomed: Option<Doomed>,
    filter_on: bool,
    filter_param: usize,
    filter_relation: usize,
    filter_value: String,
    action: usize,
    param: usize,
    value: String,
    /// The outcome of the last changes, or an error.
    message: String,
}

impl Group {
    fn new(purpose: Purpose) -> Self {
        Group {
            purpose,
            step: Step::Select,
            selected: BTreeSet::new(),
            tag: String::new(),
            // Group Delete starts by drawing the region
            region: (purpose == Purpose::Delete).then(Region::default),
            doomed: None,
            filter_on: false,
            filter_param: 0,
            filter_relation: 0,
            filter_value: String::new(),
            action: 0,
            param: 0,
            value: String::new(),
            message: String::new(),
        }
    }
    fn nodes(&self) -> bool {
        self.purpose != Purpose::EditLinks
    }
}

/// The properties Group Edit changes: EPANET-UI's, of junctions or of pipes.
fn params(nodes: bool) -> &'static [(&'static str, PropKey)] {
    if nodes {
        &[
            ("Tag", PropKey::Tag),
            ("Elevation", PropKey::Field(0)),
            ("Base Demand", PropKey::Field(1)),
            ("Demand Pattern", PropKey::Field(2)),
            ("Emitter Coeff.", PropKey::Keyed("EMITTERS", 1)),
            ("Initial Quality", PropKey::Keyed("QUALITY", 1)),
        ]
    } else {
        &[
            ("Tag", PropKey::Tag),
            ("Diameter", PropKey::Field(1)),
            ("Length", PropKey::Field(0)),
            ("Roughness", PropKey::Field(2)),
            ("Loss Coeff.", PropKey::Field(3)),
            ("Bulk Coeff.", PropKey::Reaction("BULK")),
            ("Wall Coeff.", PropKey::Reaction("WALL")),
            ("Leak Area", PropKey::Keyed("LEAKAGE", 1)),
            ("Leak Expansion", PropKey::Keyed("LEAKAGE", 2)),
        ]
    }
}

const RELATIONS: [&str; 3] = ["Equal To", "Below", "Above"];
const ACTIONS: [&str; 3] = ["Replace", "Multiply", "Add To"];

/// Properties compared and set as text: a Tag, a Demand Pattern.
fn textual(key: PropKey) -> bool {
    matches!(key, PropKey::Tag | PropKey::Field(2))
}

/// Opens Group Edit for nodes or links, or Group Delete.
pub(crate) fn open(app: &mut EpanetApp, purpose: Purpose) {
    app.set_tool(crate::app::Tool::Select);
    app.select(None);
    app.group = Some(Group::new(purpose));
    // where the Group Selector is drawn
    crate::dock::reveal_viewer(app);
}

/// Group Edit or Group Delete is open.
pub(crate) fn active(app: &EpanetApp) -> bool {
    app.group.is_some()
}

/// Esc: drops the region being drawn, or else closes the group. False when
/// no group is open.
pub(crate) fn cancel(app: &mut EpanetApp) -> bool {
    let Some(g) = app.group.as_mut() else {
        return false;
    };
    if g.purpose == Purpose::Delete || g.region.take().is_none() {
        app.group = None;
    }
    true
}

/// The element of a group member.
fn element(nodes: bool, i: usize) -> Element {
    if nodes {
        Element::Node(i)
    } else {
        Element::Link(i)
    }
}

/// The nodes, or the links with both ends, in a region (None: all of them).
fn in_region(app: &EpanetApp, nodes: bool, poly: Option<&[[f64; 2]]>) -> Vec<usize> {
    let net = &app.net;
    let within = |i: usize| match poly {
        None => true,
        Some(poly) => net.nodes[i].pos.is_some_and(|p| inside(poly, p)),
    };
    if nodes {
        (0..net.nodes.len()).filter(|&i| within(i)).collect()
    } else {
        (0..net.links.len())
            .filter(|&i| {
                net.link_ends(&net.links[i])
                    .is_some_and(|(a, b)| within(a) && within(b))
            })
            .collect()
    }
}

/// The [LABELS] lines, and whether each is in the region (None: all are).
fn labels_in(app: &EpanetApp, poly: Option<&[[f64; 2]]>) -> Vec<(String, bool)> {
    app.net
        .section_text("LABELS")
        .lines()
        .map(|line| {
            let t: Vec<&str> = line.split_whitespace().collect();
            let at = (
                t.first().and_then(|v| v.parse().ok()),
                t.get(1).and_then(|v| v.parse().ok()),
            );
            let is_label = !line.trim_start().starts_with(';');
            let hit = match (at, poly) {
                ((Some(x), Some(y)), Some(poly)) => is_label && inside(poly, [x, y]),
                ((Some(_), Some(_)), None) => is_label,
                _ => false,
            };
            (line.to_string(), hit)
        })
        .collect()
}

/// Map input while a group is open: the region being drawn, or clicks that
/// toggle objects in or out of the selection. True when taken.
pub(crate) fn map_input(app: &mut EpanetApp, rect: Rect, response: &egui::Response) -> bool {
    let Some(g) = app.group.as_mut() else {
        return false;
    };
    if let Some(region) = &mut g.region {
        let drawn = region.input(&app.view, rect, response);
        let nodes = g.nodes();
        let purpose = g.purpose;
        let poly = match drawn {
            Drawn::Ignored => return false,
            Drawn::Taken => return true,
            Drawn::Closed(poly) => Some(poly),
            Drawn::Whole => None,
        };
        let found = in_region(app, nodes, poly.as_deref());
        let Some(g) = app.group.as_mut() else {
            return true;
        };
        g.region = None;
        if purpose == Purpose::Delete {
            let labels = labels_in(app, poly.as_deref())
                .iter()
                .filter(|(_, hit)| *hit)
                .count();
            let Some(g) = app.group.as_mut() else {
                return true;
            };
            g.doomed = Some(Doomed {
                nodes: found,
                labels,
                region: poly,
            });
        } else {
            g.selected.extend(found);
        }
        return true;
    }
    if g.step != Step::Select || g.purpose == Purpose::Delete {
        return response.clicked();
    }
    if !response.clicked() {
        return false;
    }
    let Some(p) = response.interact_pointer_pos() else {
        return true;
    };
    let hit = if g.nodes() {
        crate::map::hit_node(app, rect, p)
    } else {
        crate::map::hit_link(app, rect, p).map(|(l, _)| l)
    };
    if let (Some(i), Some(g)) = (hit, app.group.as_mut())
        && !g.selected.remove(&i)
    {
        g.selected.insert(i);
    }
    true
}

/// The color of a node (or link) on the map while a group is open: red in
/// the group, gray otherwise; None when no group is open.
pub(crate) fn shade(app: &EpanetApp, node: bool, i: usize) -> Option<Color32> {
    let g = app.group.as_ref()?;
    let member = match &g.doomed {
        Some(d) => node && d.nodes.contains(&i),
        None => node == g.nodes() && g.selected.contains(&i),
    };
    Some(if member {
        crate::style::SELECTED
    } else {
        Color32::from_gray(0xa8)
    })
}

/// The region being drawn on the map, or Group Delete's.
pub(crate) fn draw(app: &EpanetApp, painter: &egui::Painter, rect: Rect, hover: Option<Pos2>) {
    let Some(g) = &app.group else {
        return;
    };
    let view = &app.view;
    if let Some(d) = &g.doomed
        && let Some(poly) = &d.region
    {
        let pts: Vec<Pos2> = poly.iter().map(|&w| view.to_screen(rect, w)).collect();
        painter.add(egui::Shape::closed_line(
            pts,
            Stroke::new(1.5, crate::style::ACCENT),
        ));
    }
    if let Some(r) = &g.region {
        r.draw(view, painter, rect, hover);
    }
}

/// The Group Selector, Group Editor or Group Delete, at the top of the left
/// panel as the report selectors are.
pub(crate) fn panel(app: &mut EpanetApp, ui: &mut Ui) {
    let Some(g) = &app.group else {
        return;
    };
    let (purpose, step) = (g.purpose, g.step);
    let title = match (purpose, step) {
        (Purpose::Delete, _) => "Group Delete",
        (_, Step::Select) => "Group Selector",
        (_, Step::Edit) => "Group Editor",
    };
    let closed = crate::reports::selector_frame(app, ui, title, |app, ui| match (purpose, step) {
        (Purpose::Delete, _) => delete_ui(app, ui),
        (_, Step::Select) => selector_ui(app, ui),
        (_, Step::Edit) => editor_ui(app, ui),
    });
    if closed {
        app.group = None;
    }
}

fn wide(ui: &mut Ui, text: &str) -> bool {
    let w = ui.available_width();
    ui.add_sized([w, 26.0], egui::Button::new(tr(text)))
        .clicked()
}

fn selector_ui(app: &mut EpanetApp, ui: &mut Ui) {
    let net = &app.net;
    let Some(g) = app.group.as_mut() else { return };
    let nodes = g.nodes();
    ui.label(tr(if nodes {
        "Click nodes on the map to toggle their selection on or off."
    } else {
        "Click links on the map to toggle their selection on or off."
    }));
    ui.add_space(8.0);
    let (with_tag, all, none, count) = if nodes {
        (
            "Select nodes with Tag",
            "Select all nodes",
            "Deselect all nodes",
            "{} nodes selected",
        )
    } else {
        (
            "Select links with Tag",
            "Select all links",
            "Deselect all links",
            "{} links selected",
        )
    };
    if wide(ui, with_tag) {
        let tag = g.tag.trim().to_string();
        if tag.is_empty() {
            g.message = tr("Enter the Tag of the objects to select").into();
        } else {
            let n = if nodes {
                net.nodes.len()
            } else {
                net.links.len()
            };
            g.selected.extend((0..n).filter(|&i| {
                net.get_prop(element(nodes, i), PropKey::Tag)
                    .trim()
                    .eq_ignore_ascii_case(&tag)
            }));
        }
    }
    let w = ui.available_width();
    ui.add(egui::TextEdit::singleline(&mut g.tag).desired_width(w));
    ui.add_space(8.0);
    let drawing = g.region.is_some();
    if wide(
        ui,
        if drawing {
            "Cancel the region"
        } else {
            "Select a region"
        },
    ) {
        g.region = if drawing {
            None
        } else {
            Some(Region::default())
        };
    }
    if g.region.is_some() {
        ui.label(RichText::new(tr(REGION_HINT)).small());
    }
    ui.add_space(8.0);
    if wide(ui, all) {
        let n = if nodes {
            net.nodes.len()
        } else {
            net.links.len()
        };
        g.selected.extend(0..n);
    }
    ui.add_space(8.0);
    if wide(ui, none) {
        g.selected.clear();
    }
    ui.add_space(8.0);
    ui.label(trf(count, &[&g.selected.len()]));
    if !g.message.is_empty() {
        ui.label(RichText::new(&g.message).color(ui.visuals().warn_fg_color));
    }
    ui.add_space(8.0);
    let (mut cancel, mut accept) = (false, false);
    ui.columns(2, |cols| {
        cancel = cols[0].button(tr("Cancel")).clicked();
        cols[1].with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            accept = ui.button(tr("Accept")).clicked();
        });
    });
    if cancel {
        app.group = None;
    } else if accept {
        if g.selected.is_empty() {
            g.message = tr("No objects have been selected.").into();
        } else {
            g.step = Step::Edit;
            g.region = None;
            g.message.clear();
        }
    }
}

/// A drop-down of fixed choices, by index.
fn choice(ui: &mut Ui, id: &str, index: &mut usize, choices: &[&str], width: f32, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(tr(choices[*index]))
            .show_ui(ui, |ui| {
                for (k, c) in choices.iter().enumerate() {
                    ui.selectable_value(index, k, tr(c));
                }
            });
    });
}

fn editor_ui(app: &mut EpanetApp, ui: &mut Ui) {
    let Some(g) = app.group.as_mut() else { return };
    let list = params(g.nodes());
    let names: Vec<&str> = list.iter().map(|(n, _)| *n).collect();
    let w = ui.available_width();
    ui.label(trf(
        if g.nodes() {
            "{} nodes selected"
        } else {
            "{} links selected"
        },
        &[&g.selected.len()],
    ));
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.set_width(w - 12.0);
        ui.label(RichText::new(tr("Optional Filter")).strong());
        crate::widgets::checkbox(ui, &mut g.filter_on, "Only those with");
        let on = g.filter_on;
        ui.horizontal(|ui| {
            choice(
                ui,
                "group_filter_param",
                &mut g.filter_param,
                &names,
                110.0,
                on,
            );
            // a Tag or a pattern is only ever equal to the value
            if textual(list[g.filter_param].1) {
                g.filter_relation = 0;
            }
            let rel = on && !textual(list[g.filter_param].1);
            choice(
                ui,
                "group_filter_rel",
                &mut g.filter_relation,
                &RELATIONS,
                80.0,
                rel,
            );
        });
        ui.add_enabled(
            on,
            egui::TextEdit::singleline(&mut g.filter_value).desired_width(w - 24.0),
        );
    });
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.set_width(w - 12.0);
        ui.label(RichText::new(tr("Edit Action")).strong());
        ui.horizontal(|ui| {
            // a Tag or a pattern is only ever replaced
            if textual(list[g.param].1) {
                g.action = 0;
            }
            let fixed = textual(list[g.param].1);
            choice(ui, "group_action", &mut g.action, &ACTIONS, 80.0, !fixed);
            choice(ui, "group_param", &mut g.param, &names, 110.0, true);
        });
        ui.horizontal(|ui| {
            ui.label(tr(if g.action == 0 { "with" } else { "by" }));
            ui.add(egui::TextEdit::singleline(&mut g.value).desired_width(w - 60.0));
        });
    });
    if !g.message.is_empty() {
        ui.add_space(6.0);
        ui.label(&g.message);
    }
    ui.add_space(8.0);
    let (mut back, mut close, mut apply) = (false, false, false);
    ui.horizontal(|ui| {
        back = ui.button(tr("Back")).clicked();
        close = ui.button(tr("Close")).clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            apply = ui.button(tr("Make Changes")).clicked();
        });
    });
    if back {
        g.step = Step::Select;
        g.message.clear();
    } else if close {
        app.group = None;
    } else if apply {
        make_changes(app);
    }
}

/// A number written as EPANET writes it: no trailing zeros.
fn number(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Applies the edit action to the selected junctions or pipes that pass the
/// filter, and says how many changed.
fn make_changes(app: &mut EpanetApp) {
    let Some(g) = app.group.as_ref() else { return };
    let nodes = g.nodes();
    let list = params(nodes);
    let (key, fkey) = (list[g.param].1, list[g.filter_param].1);
    let value = g.value.trim().to_string();
    let filter_value = g.filter_value.trim().to_string();
    let parse = |s: &str| eui_core::num::parse(s);
    // the values must make sense before anything changes
    let error = if g.filter_on && !textual(fkey) && parse(&filter_value).is_none() {
        Some("Invalid number.")
    } else if key == PropKey::Tag && (value.contains(' ') || value.contains(';')) {
        Some("Tags cannot contain spaces or semi-colons.")
    } else if !textual(key) && parse(&value).is_none() {
        Some("Invalid number.")
    } else if key == PropKey::Field(2)
        && nodes
        && !value.is_empty()
        && !app
            .net
            .raw_lines("PATTERNS")
            .any(|t| t.first() == Some(&value))
    {
        Some("Invalid Pattern ID")
    } else {
        None
    };
    if let Some(e) = error {
        if let Some(g) = app.group.as_mut() {
            g.message = tr(e).into();
        }
        return;
    }
    let (filter_on, relation, action) = (g.filter_on, g.filter_relation, g.action);
    let members: Vec<usize> = g.selected.iter().copied().collect();
    let net = &app.net;
    let kind_ok = |i: usize| {
        if nodes {
            net.nodes[i].kind == NodeKind::Junction
        } else {
            net.links[i].kind == LinkKind::Pipe
        }
    };
    let passes = |i: usize| {
        if !filter_on {
            return true;
        }
        let v = net.get_prop(element(nodes, i), fkey);
        if textual(fkey) {
            return v.trim() == filter_value;
        }
        let (x, f) = (
            parse(&v).unwrap_or(0.0),
            parse(&filter_value).unwrap_or(0.0),
        );
        match relation {
            1 => x <= f,
            2 => x >= f,
            _ => (x - f).abs() < 1e-4,
        }
    };
    let targets: Vec<usize> = members
        .into_iter()
        .filter(|&i| kind_ok(i) && passes(i))
        .collect();
    if !targets.is_empty() {
        app.checkpoint();
    }
    let mut changed = 0;
    for i in &targets {
        let e = element(nodes, *i);
        let new = if textual(key) {
            value.clone()
        } else {
            let v = parse(&value).unwrap_or(0.0);
            let x = parse(&app.net.get_prop(e, key)).unwrap_or(0.0);
            number(match action {
                1 => x * v,
                2 => x + v,
                _ => v,
            })
        };
        if app.net.set_prop(e, key, &new).is_ok() {
            changed += 1;
        }
    }
    if changed > 0 {
        app.touch();
        app.section_buffer = None;
    }
    if let Some(g) = app.group.as_mut() {
        g.message = if changed == 0 {
            tr("No objects match your criteria").into()
        } else {
            trf("{} object(s) have been modified.", &[&changed])
        };
    }
}

fn delete_ui(app: &mut EpanetApp, ui: &mut Ui) {
    let Some(g) = app.group.as_mut() else { return };
    let Some(d) = &g.doomed else {
        ui.label(tr("Press Enter to select the entire network, or draw a region around the objects to delete."));
        ui.add_space(6.0);
        ui.label(RichText::new(tr(REGION_HINT)).small());
        ui.add_space(8.0);
        if ui.button(tr("Cancel")).clicked() {
            app.group = None;
        }
        return;
    };
    ui.label(tr(
        "Do you wish to delete all objects in the selected region?",
    ));
    ui.label(trf(
        "{} nodes (with their links) and {} labels",
        &[&d.nodes.len(), &d.labels],
    ));
    ui.add_space(8.0);
    let (mut yes, mut no) = (false, false);
    ui.horizontal(|ui| {
        yes = ui.button(tr("Yes")).clicked();
        no = ui.button(tr("No")).clicked();
    });
    if yes {
        delete(app);
    } else if no {
        app.group = None;
    }
}

/// Deletes the nodes (with their links) and the labels in Group Delete's region.
fn delete(app: &mut EpanetApp) {
    let Some(d) = app.group.take().and_then(|g| g.doomed) else {
        return;
    };
    app.checkpoint();
    // by ID, as deleting renumbers the rest
    let ids: Vec<String> = d
        .nodes
        .iter()
        .map(|&i| app.net.nodes[i].id.clone())
        .collect();
    for id in &ids {
        if let Some(i) = app.net.node_index(id) {
            app.net.delete_node(i);
        }
    }
    let kept: Vec<String> = labels_in(app, d.region.as_deref())
        .into_iter()
        .filter(|(_, hit)| !hit)
        .map(|(line, _)| line)
        .collect();
    app.net.replace_section("LABELS", kept);
    app.selection = None;
    app.editor_shown = None;
    app.buffer_owner = None;
    app.section_buffer = None;
    app.status = trf("Deleted {} nodes and {} labels", &[&ids.len(), &d.labels]);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use eframe::App as _;

    /// Frames of the whole app, with input, at a growing time.
    pub(crate) struct Run {
        ctx: egui::Context,
        f: eframe::Frame,
        t: f64,
        /// The keys held (Ctrl, Shift…).
        pub(crate) modifiers: egui::Modifiers,
        /// The keys held as last sent.
        sent: egui::Modifiers,
    }

    impl Run {
        /// One frame, `gap` seconds after the last: each text drawn and where.
        pub(crate) fn frame(
            &mut self,
            app: &mut EpanetApp,
            events: Vec<egui::Event>,
            gap: f64,
        ) -> Vec<(String, Rect)> {
            self.t += gap.max(0.02);
            let mut events = events;
            if self.modifiers != self.sent {
                self.sent = self.modifiers;
                events.insert(0, egui::Event::ModifiersChanged(self.modifiers));
            }
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
                time: Some(self.t),
                events,
                ..Default::default()
            };
            let f = &mut self.f;
            let mut out = self.ctx.run_ui(input, |ui| {
                app.logic(ui.ctx(), f);
                app.ui(ui, f);
            });
            out.textures_delta.clear();
            out.shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => {
                        Some((t.galley.text().to_string(), t.visual_bounding_rect()))
                    }
                    _ => None,
                })
                .collect()
        }
        pub(crate) fn button(p: Pos2, pressed: bool, secondary: bool) -> egui::Event {
            egui::Event::PointerButton {
                pos: p,
                button: if secondary {
                    egui::PointerButton::Secondary
                } else {
                    egui::PointerButton::Primary
                },
                pressed,
                modifiers: Default::default(),
            }
        }
        pub(crate) fn click_with(&mut self, app: &mut EpanetApp, p: Pos2, secondary: bool) {
            self.frame(app, vec![egui::Event::PointerMoved(p)], 1.0);
            self.frame(app, vec![Self::button(p, true, secondary)], 0.0);
            self.frame(app, vec![Self::button(p, false, secondary)], 0.0);
            self.frame(app, vec![], 0.0);
        }
        pub(crate) fn click(&mut self, app: &mut EpanetApp, p: Pos2) {
            self.click_with(app, p, false);
        }
        /// Clicks a text drawn, e.g. a button's.
        pub(crate) fn press(&mut self, app: &mut EpanetApp, text: &str) {
            // (a window just opened is first only measured, not drawn)
            self.frame(app, vec![], 0.0);
            let t = self.frame(app, vec![], 0.0);
            let r = t
                .iter()
                .find(|(s, _)| s == text)
                .unwrap_or_else(|| panic!("no {text:?}"))
                .1;
            self.click(app, r.center());
        }
        pub(crate) fn drag(&mut self, app: &mut EpanetApp, from: Pos2, to: Pos2) {
            self.frame(app, vec![egui::Event::PointerMoved(from)], 1.0);
            self.frame(app, vec![Self::button(from, true, false)], 0.0);
            for k in 1..=6 {
                let p = from + (to - from) * (k as f32 / 6.0);
                self.frame(app, vec![egui::Event::PointerMoved(p)], 0.0);
            }
            self.frame(app, vec![Self::button(to, false, false)], 0.0);
            self.frame(app, vec![], 0.0);
        }
        pub(crate) fn key(&mut self, app: &mut EpanetApp, key: egui::Key) {
            let k = egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            };
            self.frame(app, vec![k], 1.0);
            self.frame(app, vec![], 0.0);
        }
    }

    /// Net1 open, drawn a few times.
    pub(crate) fn setup() -> (Run, EpanetApp) {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        let mut run = Run {
            ctx,
            f: eframe::Frame::_new_kittest(),
            t: 0.0,
            modifiers: Default::default(),
            sent: Default::default(),
        };
        for _ in 0..3 {
            run.frame(&mut app, vec![], 0.0);
        }
        (run, app)
    }

    /// Where a node is on the screen.
    pub(crate) fn at(app: &EpanetApp, id: &str) -> Pos2 {
        let map = app.export.map_rect.unwrap();
        let i = app.net.node_index(id).unwrap();
        app.view.to_screen(map, app.net.nodes[i].pos.unwrap())
    }

    fn selected(app: &EpanetApp) -> Vec<String> {
        let g = app.group.as_ref().unwrap();
        let mut ids: Vec<String> = g
            .selected
            .iter()
            .map(|&i| {
                if g.nodes() {
                    app.net.nodes[i].id.clone()
                } else {
                    app.net.links[i].id.clone()
                }
            })
            .collect();
        ids.sort();
        ids
    }

    /// Group Edit of nodes: a rectangle dragged around junctions 22 and 23
    /// selects them, a click toggles 21 in, and the Group Editor adds 10 to
    /// the elevations of those below 700.
    #[test]
    fn group_edit_by_rectangle() {
        let (mut run, mut app) = setup();
        open(&mut app, Purpose::EditNodes);
        run.press(&mut app, "Select a region");
        let (n22, n23) = (at(&app, "22"), at(&app, "23"));
        run.drag(&mut app, n22 - vec2(15.0, 15.0), n23 + vec2(15.0, 15.0));
        assert_eq!(selected(&app), ["22", "23"]);
        let n21 = at(&app, "21");
        run.click(&mut app, n21);
        assert_eq!(selected(&app), ["21", "22", "23"]);
        // on the map: red in the group, gray otherwise
        let node = |id: &str| app.net.node_index(id).unwrap();
        assert_eq!(shade(&app, true, node("22")), Some(crate::style::SELECTED));
        assert_eq!(
            shade(&app, true, node("10")),
            Some(Color32::from_gray(0xa8))
        );
        assert_eq!(shade(&app, false, 0), Some(Color32::from_gray(0xa8)));
        run.press(&mut app, "Accept");
        assert!(app.group.as_ref().is_some_and(|g| g.step == Step::Edit));

        let g = app.group.as_mut().unwrap();
        g.filter_on = true;
        g.filter_param = 1;
        g.filter_relation = 1;
        g.filter_value = "699".into();
        g.param = 1;
        g.action = 2;
        g.value = "10".into();
        run.press(&mut app, "Make Changes");
        let elev = |app: &EpanetApp, id: &str| {
            let i = app.net.node_index(id).unwrap();
            app.net.nodes[i].num(0).unwrap()
        };
        // 21 was 700, 22 695, 23 690
        assert_eq!(elev(&app, "21"), 700.0);
        assert_eq!(elev(&app, "22"), 705.0);
        assert_eq!(elev(&app, "23"), 700.0);
        assert_eq!(
            app.group.as_ref().unwrap().message,
            "2 object(s) have been modified."
        );
        app.undo();
        assert_eq!(elev(&app, "22"), 695.0);
    }

    /// Group Edit of links: a polygon clicked around pipes 21 and 22 (their
    /// ends inside) selects them, closed by a right-click; Enter without a
    /// region selects every link; Esc closes the Group Selector.
    #[test]
    fn group_edit_by_polygon() {
        let (mut run, mut app) = setup();
        open(&mut app, Purpose::EditLinks);
        run.press(&mut app, "Select a region");
        let (a, b) = (at(&app, "21"), at(&app, "23"));
        for c in [
            a + vec2(-20.0, -20.0),
            b + vec2(20.0, -20.0),
            b + vec2(20.0, 20.0),
            a + vec2(-20.0, 20.0),
        ] {
            run.click(&mut app, c);
        }
        run.click_with(&mut app, a, true);
        assert_eq!(selected(&app), ["21", "22"]);
        assert!(app.group.as_ref().unwrap().region.is_none());

        run.press(&mut app, "Select a region");
        run.key(&mut app, egui::Key::Enter);
        assert_eq!(selected(&app).len(), app.net.links.len());
        run.key(&mut app, egui::Key::Escape);
        assert!(app.group.is_none());
    }

    /// Group Delete: a rectangle around junction 23 deletes it, with the
    /// pipe ending there, once Yes is pressed; Enter takes the whole
    /// network, its labels too.
    #[test]
    fn group_delete() {
        let (mut run, mut app) = setup();
        let labels = |app: &EpanetApp| app.net.raw_lines("LABELS").count();
        let before = (app.net.nodes.len(), app.net.links.len(), labels(&app));
        open(&mut app, Purpose::Delete);
        let n23 = at(&app, "23");
        run.drag(&mut app, n23 - vec2(15.0, 15.0), n23 + vec2(15.0, 15.0));
        assert!(app.net.node_index("23").is_some(), "not before Yes");
        run.press(&mut app, "Yes");
        assert!(app.group.is_none());
        assert!(app.net.node_index("23").is_none());
        assert!(app.net.link_index("22").is_none());
        assert_eq!(app.net.nodes.len(), before.0 - 1);
        assert_eq!(labels(&app), before.2);
        app.undo();
        assert_eq!(
            (app.net.nodes.len(), app.net.links.len()),
            (before.0, before.1)
        );

        open(&mut app, Purpose::Delete);
        run.key(&mut app, egui::Key::Enter);
        run.press(&mut app, "Yes");
        assert_eq!(
            (app.net.nodes.len(), app.net.links.len(), labels(&app)),
            (0, 0, 0)
        );
    }

    /// The rectangle being dragged is drawn while the button is held.
    #[test]
    fn the_rectangle_shows_while_dragged() {
        let (mut run, mut app) = setup();
        open(&mut app, Purpose::EditNodes);
        run.press(&mut app, "Select a region");
        let from = at(&app, "22") - vec2(15.0, 15.0);
        let to = from + vec2(120.0, 80.0);
        run.frame(&mut app, vec![egui::Event::PointerMoved(from)], 1.0);
        run.frame(&mut app, vec![Run::button(from, true, false)], 0.0);
        for k in 1..=5 {
            let p = from + (to - from) * (k as f32 / 6.0);
            run.frame(&mut app, vec![egui::Event::PointerMoved(p)], 0.0);
        }
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            time: Some(run.t + 0.02),
            events: vec![egui::Event::PointerMoved(to)],
            ..Default::default()
        };
        let f = &mut run.f;
        let mut out = run.ctx.run_ui(input, |ui| {
            app.logic(ui.ctx(), f);
            app.ui(ui, f);
        });
        out.textures_delta.clear();
        let drawn = out.shapes.iter().any(|c| {
            matches!(&c.shape, egui::Shape::Rect(r)
                if (r.rect.min - from).length() < 1.0 && (r.rect.max - to).length() < 1.0)
        });
        assert!(drawn, "the rectangle from {from:?} to {to:?}");
    }

    /// Group Edit opens in the Map Viewer, which comes into view for it:
    /// its side unfolded, the panel shown.
    #[test]
    fn the_selector_comes_into_view() {
        let (mut run, mut app) = setup();
        app.prefs.docks.fold_left = true;
        app.prefs.show_viewer = false;
        run.frame(&mut app, vec![], 0.0);
        open(&mut app, Purpose::EditNodes);
        assert!(!app.prefs.docks.fold_left && app.prefs.show_viewer);
        run.frame(&mut app, vec![], 0.0);
        let t = run.frame(&mut app, vec![], 0.0);
        assert!(t.iter().any(|(s, _)| s == "Group Selector"));
    }
}
