//! The network map: drawing, navigation and direct editing.

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
    pos2, vec2,
};
use eui_core::props::Element;
use eui_core::{LinkKind, LinkVar, NodeKind};

use crate::app::{EpanetApp, Tool};
use crate::geometry::Occupancy;
use crate::style::{self, Legend};

/// World <-> screen transform. World Y points up, screen Y points down.
#[derive(Clone, Debug)]
pub struct MapView {
    pub center: [f64; 2],
    /// Screen points per world unit.
    pub scale: f64,
    /// Request a zoom-to-extent on the next frame.
    pub fit_pending: bool,
    /// What to fit when the network has no coordinates yet (e.g. the world
    /// for a web basemap on a new project).
    pub fit_to: Option<[f64; 4]>,
    /// Keep re-fitting when the map area resizes, until the user pans or zooms.
    /// (In the browser the canvas reaches its final size a few frames late.)
    auto_fit: bool,
    last_size: Vec2,
}

impl Default for MapView {
    fn default() -> Self {
        MapView {
            center: [0.0, 0.0],
            scale: 1.0,
            fit_pending: true,
            fit_to: None,
            auto_fit: false,
            last_size: Vec2::ZERO,
        }
    }
}

impl MapView {
    pub fn to_screen(&self, rect: Rect, p: [f64; 2]) -> Pos2 {
        let c = rect.center();
        pos2(
            c.x + ((p[0] - self.center[0]) * self.scale) as f32,
            c.y - ((p[1] - self.center[1]) * self.scale) as f32,
        )
    }

    pub fn to_world(&self, rect: Rect, p: Pos2) -> [f64; 2] {
        let c = rect.center();
        [
            self.center[0] + (p.x - c.x) as f64 / self.scale,
            self.center[1] - (p.y - c.y) as f64 / self.scale,
        ]
    }

    pub fn fit(&mut self, rect: Rect, bounds: Option<[f64; 4]>) {
        let Some([x0, y0, x1, y1]) = bounds else {
            return;
        };
        self.center = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
        self.scale = fit_scale(rect, [x0, y0, x1, y1]);
    }

    /// How many zoom steps (as the Zoom In tool takes) the map is past its full
    /// extent; negative when zoomed out further.
    pub fn zoom_level(&self, rect: Rect, bounds: Option<[f64; 4]>) -> f64 {
        bounds.map_or(0.0, |b| {
            (self.scale / fit_scale(rect, b)).ln() / ZOOM_STEP.ln()
        })
    }

    /// Zooms by `factor`, keeping the world point under `anchor` fixed.
    fn zoom_at(&mut self, rect: Rect, anchor: Pos2, factor: f64) {
        self.auto_fit = false;
        let before = self.to_world(rect, anchor);
        self.scale = (self.scale * factor).clamp(1e-9, 1e9);
        let after = self.to_world(rect, anchor);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }

    /// Zoom in (positive) or out (negative) around the map center.
    pub fn zoom_steps(&mut self, steps: f64) {
        self.auto_fit = false;
        self.scale *= ZOOM_STEP.powf(steps);
    }

    /// Moves the map so that `world` is in the middle (Overview Map).
    pub fn center_on(&mut self, world: [f64; 2]) {
        self.auto_fit = false;
        self.center = world;
    }

    fn pan(&mut self, d: Vec2) {
        self.auto_fit = false;
        self.center[0] -= d.x as f64 / self.scale;
        self.center[1] += d.y as f64 / self.scale;
    }
}

/// Scale factor of one Zoom In / Zoom Out step.
const ZOOM_STEP: f64 = 1.5;

/// The scale that fits `bounds` in `rect` with a margin (the full extent).
fn fit_scale(rect: Rect, [x0, y0, x1, y1]: [f64; 4]) -> f64 {
    let w = (x1 - x0).max(1e-9);
    let h = (y1 - y0).max(1e-9);
    let s = (rect.width() as f64 * 0.9 / w).min(rect.height() as f64 * 0.9 / h);
    if s.is_finite() && s > 0.0 { s } else { 1.0 }
}

const HIT_RADIUS: f32 = 8.0;

fn node_visible(app: &EpanetApp, kind: NodeKind) -> bool {
    match kind {
        NodeKind::Junction => app.prefs.show_junctions,
        _ => app.prefs.show_tanks,
    }
}

/// Screen-space polyline of a link (end nodes plus vertices).
fn link_points(app: &EpanetApp, rect: Rect, i: usize) -> Option<Vec<Pos2>> {
    link_points_within(app, rect, i, 0.0)
}

/// As [`link_points`], leaving out vertices closer than `tol` points to the
/// previous one (they cannot be seen, but cost time to draw).
fn link_points_within(app: &EpanetApp, rect: Rect, i: usize, tol: f32) -> Option<Vec<Pos2>> {
    let link = &app.net.links[i];
    let (a, b) = app.geom.ends.get(i).copied().flatten()?;
    let pa = app.net.nodes[a].pos?;
    let pb = app.net.nodes[b].pos?;
    let mut pts = Vec::with_capacity(link.vertices.len() + 2);
    pts.push(app.view.to_screen(rect, pa));
    for &v in &link.vertices {
        let s = app.view.to_screen(rect, v);
        if tol <= 0.0 || pts.last().is_none_or(|l| l.distance(s) >= tol) {
            pts.push(s);
        }
    }
    let end = app.view.to_screen(rect, pb);
    if pts.len() > 1 && pts.last().is_some_and(|l| l.distance(end) < tol) {
        pts.pop();
    }
    pts.push(end);
    Some(pts)
}

/// Whether the world point `w` is within `tol` world units of box `b`.
fn near_box(b: [f64; 4], w: [f64; 2], tol: f64) -> bool {
    w[0] >= b[0] - tol && w[0] <= b[2] + tol && w[1] >= b[1] - tol && w[1] <= b[3] + tol
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(p)
}

fn hit_node(app: &EpanetApp, rect: Rect, p: Pos2) -> Option<usize> {
    let r = HIT_RADIUS.max(app.prefs.node_size + 4.0);
    app.net
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| node_visible(app, n.kind))
        .filter_map(|(i, n)| Some((i, app.view.to_screen(rect, n.pos?).distance(p))))
        .filter(|&(_, d)| d <= r)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Nearest link and the index of the polyline segment hit.
fn hit_link(app: &EpanetApp, rect: Rect, p: Pos2) -> Option<(usize, usize)> {
    if !app.prefs.show_links {
        return None;
    }
    let w = app.view.to_world(rect, p);
    let tol = 6.0 / app.view.scale;
    (0..app.net.links.len())
        .filter(|&i| {
            app.geom
                .boxes
                .get(i)
                .copied()
                .flatten()
                .is_some_and(|b| near_box(b, w, tol))
        })
        .filter_map(|i| {
            let pts = link_points(app, rect, i)?;
            let (seg, d) = pts
                .windows(2)
                .enumerate()
                .map(|(s, w)| (s, dist_to_segment(p, w[0], w[1])))
                .min_by(|a, b| a.1.total_cmp(&b.1))?;
            (d <= 6.0).then_some((i, seg, d))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(i, s, _)| (i, s))
}

fn hit(app: &EpanetApp, rect: Rect, p: Pos2) -> Option<Element> {
    hit_node(app, rect, p)
        .map(Element::Node)
        .or_else(|| hit_link(app, rect, p).map(|(i, _)| Element::Link(i)))
}

fn hit_vertex(app: &EpanetApp, rect: Rect, link: usize, p: Pos2) -> Option<usize> {
    app.net.links[link]
        .vertices
        .iter()
        .position(|&v| app.view.to_screen(rect, v).distance(p) <= HIT_RADIUS)
}

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let pal = app.map_palette();
    painter.rect_filled(rect, 0.0, pal.map_bg);
    app.export.map_rect = Some(rect);
    app.refresh_geometry();

    let view = &mut app.view;
    if view.fit_pending {
        view.fit_pending = false;
        view.auto_fit = true;
        view.last_size = Vec2::ZERO;
    }
    if view.auto_fit && rect.size() != view.last_size {
        view.fit(rect, app.geom.bounds.or(view.fit_to));
    }
    view.last_size = rect.size();

    handle_input(app, ui, rect, &response);
    // Edits made by the input above show up in this frame's drawing.
    app.refresh_geometry();
    crate::basemap::draw(app, ui, &painter, rect, pal.map_bg);

    let highlight = |is_node: bool, i: usize| {
        app.query.active && app.query.nodes == is_node && app.query.hits.binary_search(&i).is_ok()
    };
    let node_legend = app.node_legend();
    let link_legend = app.link_legend();
    // Labels and arrows can wait until the map is zoomed in far enough.
    let zoom = if app.prefs.label_zoom > 0 || app.prefs.arrow_zoom > 0 {
        app.view.zoom_level(rect, app.bounds())
    } else {
        0.0
    };
    // allow for rounding, so a level is reached after that many Zoom In steps
    let zoomed_to = |level: u32| level == 0 || zoom >= level as f64 - 0.01;

    // ---- links ----
    let mut mids: Vec<(usize, Pos2)> = Vec::new();
    if app.prefs.show_links {
        let arrows =
            app.prefs.show_arrows && app.results.is_some() && zoomed_to(app.prefs.arrow_zoom);
        // Pump and valve symbols grow with the link width, as in EPANET-UI.
        let symbol = app.prefs.link_width + 3.0;
        // The world box on screen: links outside it are skipped unprojected.
        let a = app.view.to_world(rect, rect.left_top() - vec2(4.0, 4.0));
        let b = app
            .view
            .to_world(rect, rect.right_bottom() + vec2(4.0, 4.0));
        let shown = [a[0], b[1], b[0], a[1]];
        // Links smaller than a pixel: one per pixel is all that can be seen.
        let mut specks = Occupancy::new(rect, 1.0);
        for i in 0..app.net.links.len() {
            let kind = app.net.links[i].kind;
            if (kind == LinkKind::Pump && !app.prefs.show_pumps)
                || (kind == LinkKind::Valve && !app.prefs.show_valves)
            {
                continue;
            }
            let Some(wb) = app.geom.boxes.get(i).copied().flatten() else {
                continue;
            };
            if wb[2] < shown[0] || wb[0] > shown[2] || wb[3] < shown[1] || wb[1] > shown[3] {
                continue;
            }
            let selected = app.selection == Some(Element::Link(i));
            let speck = kind == LinkKind::Pipe
                && !selected
                && !highlight(false, i)
                && ((wb[2] - wb[0]).max(wb[3] - wb[1]) * app.view.scale) < 1.5;
            if speck
                && !specks.claim(
                    app.view
                        .to_screen(rect, [(wb[0] + wb[2]) / 2.0, (wb[1] + wb[3]) / 2.0]),
                )
            {
                continue;
            }
            let Some(pts) = link_points_within(app, rect, i, 1.0) else {
                continue;
            };
            let bb = Rect::from_points(&pts);
            let value = app.link_theme_value(i);
            let color = if selected {
                style::SELECTED
            } else if highlight(false, i) {
                style::SELECTED.gamma_multiply(0.8)
            } else {
                value.map_or(pal.link, |v| link_legend.color(v))
            };
            let mut width = app.prefs.link_width;
            if app.prefs.link_proportional
                && let Some(v) = value
            {
                width += link_legend.class(v) as f32;
            }
            if selected || highlight(false, i) {
                width += 2.0;
            }
            if app.prefs.link_border {
                painter.add(Shape::line(pts.clone(), Stroke::new(width + 2.0, OUTLINE)));
            }
            painter.add(Shape::line(pts.clone(), Stroke::new(width, color)));

            let (mid, dir) = midpoint(&pts);
            mids.push((i, mid));
            match kind {
                LinkKind::Pump => draw_pump(&painter, mid, dir, symbol, color),
                LinkKind::Valve => draw_valve(&painter, mid, dir, symbol, color),
                LinkKind::Pipe => {}
            }
            if arrows
                && kind == LinkKind::Pipe
                && bb.size().length() > 40.0
                && let Some(q) = app.link_result(i, LinkVar::Flow)
                && q.abs() > 1e-6
            {
                draw_arrow(
                    &painter,
                    mid,
                    if q > 0.0 { dir } else { -dir },
                    app.prefs.arrow_size,
                    color,
                );
            }
            if selected && app.tool == Tool::Reshape {
                for &v in &app.net.links[i].vertices {
                    let s = app.view.to_screen(rect, v);
                    painter.rect(
                        Rect::from_center_size(s, vec2(7.0, 7.0)),
                        0.0,
                        pal.map_bg,
                        Stroke::new(1.5, style::SELECTED),
                        StrokeKind::Middle,
                    );
                }
            }
        }
    }

    // ---- pending link rubber band ----
    if let (Some(start), Some(hover)) = (app.link_start, response.hover_pos())
        && let Some(p) = app.net.nodes.get(start).and_then(|n| n.pos)
    {
        let a = app.view.to_screen(rect, p);
        painter.add(Shape::dashed_line(
            &[a, hover],
            Stroke::new(1.5, style::ACCENT),
            6.0,
            4.0,
        ));
    }

    // ---- nodes ----
    let mut visible_nodes = Vec::new();
    // Junctions closer than a dot's width hide each other; draw one per spot.
    let mut spots = Occupancy::new(rect.expand(10.0), 1.5 * app.prefs.node_size);
    for (i, n) in app.net.nodes.iter().enumerate() {
        if !node_visible(app, n.kind) {
            continue;
        }
        let Some(p) = n.pos else { continue };
        let s = app.view.to_screen(rect, p);
        if !rect.expand(10.0).contains(s) {
            continue;
        }
        let marked = app.selection == Some(Element::Node(i))
            || app.link_start == Some(i)
            || highlight(true, i);
        if n.kind == NodeKind::Junction && !marked && !spots.claim(s) {
            visible_nodes.push((i, s, app.prefs.node_size));
            continue;
        }
        let value = app.node_theme_value(i);
        let fill = value.map_or(pal.node, |v| node_legend.color(v));
        let mut r = app.prefs.node_size;
        if app.prefs.node_proportional
            && let Some(v) = value
        {
            r += node_legend.class(v) as f32;
        }
        visible_nodes.push((i, s, r));
        draw_node(&painter, s, n.kind, r, fill, app.prefs.node_border);
        if app.selection == Some(Element::Node(i)) || app.link_start == Some(i) {
            // The marker blinks (about twice a second) if the user wants it to.
            let on = !app.prefs.blink_marker
                || app.link_start == Some(i)
                || (ui.input(|inp| inp.time) * 2.0) as i64 % 2 == 0;
            if app.prefs.blink_marker {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
            }
            if on {
                painter.circle_stroke(s, r + 4.0, Stroke::new(2.0, style::SELECTED));
            }
        } else if highlight(true, i) {
            painter.circle_stroke(
                s,
                r + 3.0,
                Stroke::new(2.0, style::SELECTED.gamma_multiply(0.8)),
            );
        }
    }

    // ---- labels (only when they can be read) ----
    let font = FontId::proportional(app.prefs.label_font);
    let few = visible_nodes.len() <= 600 && zoomed_to(app.prefs.label_zoom);
    let opaque = app.prefs.opaque_labels.then_some(pal.map_bg);
    if few && (app.prefs.show_node_ids || app.prefs.show_node_values) {
        for &(i, s, r) in &visible_nodes {
            let mut parts = Vec::new();
            if app.prefs.show_node_ids {
                parts.push(app.net.nodes[i].id.clone());
            }
            if app.prefs.show_node_values
                && let Some(v) = app.node_theme_value(i)
            {
                parts.push(style::fmt_value(v));
            }
            if !parts.is_empty() {
                map_text(
                    &painter,
                    s + vec2(r + 3.0, -r - 1.0),
                    Align2::LEFT_BOTTOM,
                    parts.join("  "),
                    font.clone(),
                    pal.label,
                    opaque,
                );
            }
        }
    }
    if few && (app.prefs.show_link_ids || app.prefs.show_link_values) {
        for &(i, mid) in &mids {
            let mut parts = Vec::new();
            if app.prefs.show_link_ids {
                parts.push(app.net.links[i].id.clone());
            }
            if app.prefs.show_link_values
                && let Some(v) = app.link_theme_value(i)
            {
                parts.push(style::fmt_value(v));
            }
            if !parts.is_empty() {
                map_text(
                    &painter,
                    mid + vec2(4.0, 4.0),
                    Align2::LEFT_TOP,
                    parts.join("  "),
                    font.clone(),
                    pal.label.gamma_multiply(0.85),
                    opaque,
                );
            }
        }
    }
    if app.prefs.show_map_labels {
        for t in app.net.raw_lines("LABELS") {
            let (Some(x), Some(y)) = (
                t.first().and_then(|v| v.parse().ok()),
                t.get(1).and_then(|v| v.parse().ok()),
            ) else {
                continue;
            };
            let s = app.view.to_screen(rect, [x, y]);
            if rect.contains(s) {
                let text = t.get(2).cloned().unwrap_or_default();
                painter.text(
                    s,
                    Align2::LEFT_TOP,
                    text,
                    FontId::proportional(app.prefs.label_font + 2.0),
                    pal.label,
                );
            }
        }
    }

    // ---- overlays ----
    draw_legends(app, ui, rect, node_legend, link_legend);
    if app.net.nodes.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Use Project > Add to place nodes, or open an .inp file (you can also drop one here)",
            FontId::proportional(15.0),
            ui.visuals().weak_text_color(),
        );
    }
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(
            1.0,
            Color32::from_gray(if app.prefs.dark { 70 } else { 170 }),
        ),
        StrokeKind::Inside,
    );
    label_editor(app, ui, rect);
    app.cursor = response.hover_pos().map(|p| app.view.to_world(rect, p));
}

fn handle_input(app: &mut EpanetApp, ui: &mut Ui, rect: Rect, response: &egui::Response) {
    // Zoom with the wheel / pinch, around the cursor.
    if let Some(hover) = response.hover_pos() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let factor = (scroll as f64 * 0.002).exp() * zoom as f64;
        if (factor - 1.0).abs() > 1e-6 {
            app.view.zoom_at(rect, hover, factor);
        }
    }
    // The basemap Georeference tool takes clicks on the map.
    if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
        && crate::basemap::map_click(app, app.view.to_world(rect, p))
    {
        return;
    }
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let reshape_link = (app.tool == Tool::Reshape)
        .then(|| app.selected_link())
        .flatten();

    // Hover feedback.
    if let Some(p) = response.hover_pos()
        && app.drag_node.is_none()
        && !response.dragged()
    {
        if let Some(sel) = hit(app, rect, p) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if app.prefs.map_hints {
                hover_tooltip(app, response, sel);
            }
        } else if app.tool != Tool::Select {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    // Dragging: a vertex (Reshape), a node (Select) or the map.
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        if let Some(l) = reshape_link {
            if let Some(v) = hit_vertex(app, rect, l, p) {
                app.checkpoint();
                app.drag_vertex = Some(v);
            }
        } else if app.tool == Tool::Select
            && let Some(i) = hit_node(app, rect, p)
        {
            app.checkpoint();
            app.drag_node = Some(i);
            app.select(Some(Element::Node(i)));
        }
    }
    if response.dragged() {
        let primary = response.dragged_by(egui::PointerButton::Primary);
        match (app.drag_node, app.drag_vertex, reshape_link, pointer) {
            (Some(i), _, _, Some(p)) if primary => {
                app.net.nodes[i].pos = Some(app.view.to_world(rect, p));
                app.geom.mark_moved();
                app.update_lengths(i);
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            (_, Some(v), Some(l), Some(p)) if primary => {
                if let Some(slot) = app.net.links[l].vertices.get_mut(v) {
                    *slot = app.view.to_world(rect, p);
                    app.geom.mark_moved();
                }
            }
            _ => {
                // The Align tool drags the basemap image instead of the map.
                let d = response.drag_delta();
                let s = app.view.scale;
                if !crate::basemap::align_drag(app, [d.x as f64 / s, -d.y as f64 / s]) {
                    app.view.pan(d);
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
        }
    }
    if response.drag_stopped() {
        if let (Some(_), Some(l)) = (app.drag_vertex, reshape_link)
            && app.prefs.auto_length
            && app.net.links[l].kind == LinkKind::Pipe
            && let Some(len) = app.net.link_map_length(l)
        {
            app.net.links[l].fields[0] = format!("{len:.2}");
        }
        app.drag_node = None;
        app.drag_vertex = None;
    }

    // Right-click: cancel a link being drawn, or delete a vertex.
    if response.secondary_clicked() {
        // Right-click cancels the link being drawn, or stops adding objects.
        match app.tool {
            Tool::AddLink(_) if app.link_start.is_some() => {}
            Tool::AddNode(_) | Tool::AddLink(_) | Tool::AddLabel => app.set_tool(Tool::Select),
            _ => {}
        }
        app.link_start = None;
        app.label_edit = None;
        if let (Some(l), Some(p)) = (reshape_link, response.interact_pointer_pos())
            && let Some(v) = hit_vertex(app, rect, l, p)
        {
            app.checkpoint();
            app.net.links[l].vertices.remove(v);
        }
    }

    if !response.clicked() {
        if response.double_clicked()
            && app.tool == Tool::Select
            && hit(app, rect, pointer.unwrap_or_default()).is_none()
        {
            app.view.fit(rect, app.bounds());
        }
        return;
    }
    let Some(p) = response.interact_pointer_pos() else {
        return;
    };
    match app.tool {
        Tool::Select => app.select(hit(app, rect, p)),
        Tool::Reshape => match (reshape_link, hit_link(app, rect, p)) {
            (Some(l), Some((h, seg))) if h == l && hit_vertex(app, rect, l, p).is_none() => {
                app.checkpoint();
                app.net.links[l]
                    .vertices
                    .insert(seg, app.view.to_world(rect, p));
            }
            (_, Some((h, _))) => app.select(Some(Element::Link(h))),
            _ => {}
        },
        Tool::AddLabel => app.label_edit = Some((app.view.to_world(rect, p), String::new())),
        Tool::AddNode(kind) => {
            app.checkpoint();
            let i = app
                .setup
                .add_node(&mut app.net, kind, app.view.to_world(rect, p));
            app.select(Some(Element::Node(i)));
            app.status = format!("Added {} {}", kind.label(), app.net.nodes[i].id);
        }
        Tool::AddLink(kind) => match (app.link_start, hit_node(app, rect, p)) {
            (None, Some(i)) => {
                app.link_start = Some(i);
                app.status = format!(
                    "{} from {}: now click the end node",
                    kind.label(),
                    app.net.nodes[i].id
                );
            }
            (Some(a), Some(b)) if a != b => {
                app.checkpoint();
                let i = app
                    .setup
                    .add_link(&mut app.net, kind, a, b, app.prefs.auto_length);
                app.select(Some(Element::Link(i)));
                app.link_start = None;
                app.status = format!("Added {} {}", kind.label(), app.net.links[i].id);
            }
            (Some(a), None) => {
                // Clicking empty space while drawing adds a junction there and keeps drawing.
                app.checkpoint();
                let b = app.setup.add_node(
                    &mut app.net,
                    NodeKind::Junction,
                    app.view.to_world(rect, p),
                );
                let i = app
                    .setup
                    .add_link(&mut app.net, kind, a, b, app.prefs.auto_length);
                app.select(Some(Element::Link(i)));
                app.link_start = Some(b);
                app.status = format!(
                    "Added {} {} — keep clicking, Esc to stop",
                    kind.label(),
                    app.net.links[i].id
                );
            }
            _ => {}
        },
    }
}

fn hover_tooltip(app: &EpanetApp, response: &egui::Response, sel: Element) {
    let text = match sel {
        Element::Node(i) => {
            let n = &app.net.nodes[i];
            let mut t = format!("{} {}", n.kind.label(), n.id);
            if let Some(v) = app.node_theme_value(i) {
                t += &format!(
                    "\n{}: {} {}",
                    app.node_theme_name(app.prefs.node_theme),
                    style::fmt_value(v),
                    app.node_theme_units(app.prefs.node_theme)
                );
            }
            t
        }
        Element::Link(i) => {
            let l = &app.net.links[i];
            let mut t = format!("{} {}  ({} → {})", l.kind.label(), l.id, l.from, l.to);
            if let Some(v) = app.link_theme_value(i) {
                t += &format!(
                    "\n{}: {} {}",
                    app.link_theme_name(app.prefs.link_theme),
                    style::fmt_value(v),
                    app.link_theme_units(app.prefs.link_theme)
                );
            }
            t
        }
    };
    response.clone().on_hover_text_at_pointer(text);
}

fn midpoint(pts: &[Pos2]) -> (Pos2, Vec2) {
    let total: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let len = w[0].distance(w[1]);
        if acc + len >= total / 2.0 && len > 0.0 {
            let t = (total / 2.0 - acc) / len;
            return (w[0] + (w[1] - w[0]) * t, (w[1] - w[0]).normalized());
        }
        acc += len;
    }
    (pts[0], Vec2::X)
}

/// Node symbols, drawn as EPANET-UI's `maprenderer` does: a dot for a
/// junction, an open container with water for a reservoir and a bowl on a
/// stem for a tank. `size` is the node size (half the dot's width).
fn draw_node(
    painter: &egui::Painter,
    s: Pos2,
    kind: NodeKind,
    size: f32,
    fill: Color32,
    border: bool,
) {
    let edge = Stroke::new(1.0, if border { OUTLINE } else { fill });
    let at = |x: f32, y: f32| s + vec2(x, y);
    let w = size + 3.0;
    match kind {
        NodeKind::Junction => {
            painter.circle(s, size, fill, edge);
        }
        NodeKind::Reservoir => {
            painter.add(Shape::line(
                vec![
                    at(-w, -w),
                    at(-w, size + 1.0),
                    at(w + 1.0, size + 1.0),
                    at(w + 1.0, -w - 1.0),
                ],
                Stroke::new(1.0, OUTLINE),
            ));
            painter.rect(
                Rect::from_min_max(at(-w, -size), at(w + 1.0, size + 1.0)),
                0.0,
                fill,
                edge,
                StrokeKind::Inside,
            );
        }
        NodeKind::Tank => {
            painter.rect(
                Rect::from_min_max(at(-size, 0.0), at(size + 1.0, w + 1.0)),
                0.0,
                fill,
                edge,
                StrokeKind::Inside,
            );
            painter.rect(
                Rect::from_min_max(at(-w, -w), at(w + 1.0, 1.0)),
                CornerRadius::same(2),
                fill,
                edge,
                StrokeKind::Inside,
            );
        }
    }
}

/// Outline of map symbols (EPANET-UI's `OutlineColor`).
const OUTLINE: Color32 = Color32::BLACK;

/// A pump: a disc with a discharge nozzle, pointing along the link.
fn draw_pump(painter: &egui::Painter, c: Pos2, dir: Vec2, size: f32, color: Color32) {
    let r = 2.0 * size;
    let d = if dir.x < 0.0 { -1.0 } else { 1.0 };
    let n = vec2(dir.y, -dir.x) * (0.5 * d);
    let edge = Stroke::new(1.0, OUTLINE);
    painter.add(Shape::convex_polygon(
        vec![c, c + dir * r, c + (dir + n) * r, c + n * r],
        color,
        edge,
    ));
    painter.circle(c, size, color, edge);
}

/// A valve: two triangles meeting at their tips, across the link.
fn draw_valve(painter: &egui::Painter, c: Pos2, dir: Vec2, size: f32, color: Color32) {
    let (cos, sin) = (dir.x, dir.y);
    for d in [1.0, -1.0] {
        let s = size * d;
        let p1 = c + vec2((-cos + sin) * s, -(sin + cos) * s);
        let p2 = c + vec2((-cos - sin) * s, -(sin - cos) * s);
        painter.add(Shape::convex_polygon(
            vec![c, p1, p2],
            color,
            Stroke::new(1.0, OUTLINE),
        ));
    }
}

/// A flow direction arrow; `size` 4 is the original 8-point-long arrow.
fn draw_arrow(painter: &egui::Painter, c: Pos2, dir: Vec2, size: f32, color: Color32) {
    let k = size / 4.0;
    let n = dir.rot90();
    painter.add(Shape::convex_polygon(
        vec![
            c + dir * 5.0 * k,
            c - dir * 3.0 * k + n * 3.5 * k,
            c - dir * 3.0 * k - n * 3.5 * k,
        ],
        color,
        Stroke::NONE,
    ));
}

/// Map text, on a box of the background color when `opaque` is given.
fn map_text(
    painter: &egui::Painter,
    pos: Pos2,
    align: Align2,
    text: String,
    font: FontId,
    color: Color32,
    opaque: Option<Color32>,
) {
    let galley = painter.layout_no_wrap(text, font, color);
    let r = align.anchor_size(pos, galley.size());
    if let Some(bg) = opaque {
        painter.rect_filled(r.expand(1.0), 0.0, bg);
    }
    painter.galley(r.min, galley, color);
}

/// Legends drawn as EPANET-UI does: a white box with the theme name and units;
/// node classes as dots, link classes as a vertical color bar.
fn draw_legends(app: &EpanetApp, ui: &Ui, rect: Rect, node: Legend, link: Legend) {
    let mut items: Vec<(String, Legend, bool)> = Vec::new();
    let nt = app.prefs.node_theme;
    if app.prefs.node_legend_visible
        && nt != crate::style::NodeTheme::None
        && (!nt.is_result() || app.results.is_some())
    {
        let u = app.node_theme_units(nt);
        items.push((
            if u.is_empty() {
                app.node_theme_name(nt)
            } else {
                format!("{} ({u})", app.node_theme_name(nt))
            },
            node,
            true,
        ));
    }
    let lt = app.prefs.link_theme;
    if app.prefs.link_legend_visible
        && lt != crate::style::LinkTheme::None
        && (!lt.is_result() || app.results.is_some())
    {
        let u = app.link_theme_units(lt);
        items.push((
            if u.is_empty() {
                app.link_theme_name(lt)
            } else {
                format!("{} ({u})", app.link_theme_name(lt))
            },
            link,
            false,
        ));
    }
    let painter = ui.painter_at(rect);
    let font = FontId::proportional(12.5);
    let row = 19.0;
    let ink = Color32::from_gray(30);
    let mut top = rect.top() + 10.0;
    for (title, legend, is_node) in items {
        let h = row * 6.0 + 12.0;
        let frame = Rect::from_min_size(pos2(rect.left() + 10.0, top), vec2(150.0, h));
        // Framed legends get a border (Legend Editor > Framed).
        let framed = if is_node {
            app.prefs.node_legend_framed
        } else {
            app.prefs.link_legend_framed
        };
        painter.rect(
            frame,
            CornerRadius::same(2),
            Color32::WHITE,
            if framed {
                Stroke::new(1.0, Color32::from_gray(160))
            } else {
                Stroke::NONE
            },
            StrokeKind::Inside,
        );
        let mut y = frame.top() + 7.0;
        painter.text(
            pos2(frame.left() + 10.0, y),
            Align2::LEFT_TOP,
            title,
            font.clone(),
            ink,
        );
        y += row + 2.0;
        for k in 0..5 {
            let b = legend.breaks;
            let label = match k {
                0 => format!("< {}", style::fmt_break(b[0])),
                4 => format!("> {}", style::fmt_break(b[3])),
                _ => format!(
                    "{} - {}",
                    style::fmt_break(b[k - 1]),
                    style::fmt_break(b[k])
                ),
            };
            let c = pos2(frame.left() + 20.0, y + row / 2.0 - 2.0);
            if is_node {
                painter.circle(
                    c,
                    5.5,
                    legend.colors[k],
                    Stroke::new(0.8, Color32::from_gray(90)),
                );
            } else {
                let bar = Rect::from_min_size(pos2(c.x - 4.0, y - 1.0), vec2(9.0, row));
                painter.rect_filled(bar, 0.0, legend.colors[k]);
                painter.vline(
                    bar.left(),
                    bar.y_range(),
                    Stroke::new(0.8, Color32::from_gray(90)),
                );
                painter.vline(
                    bar.right(),
                    bar.y_range(),
                    Stroke::new(0.8, Color32::from_gray(90)),
                );
                if k == 0 {
                    painter.hline(
                        bar.x_range(),
                        bar.top(),
                        Stroke::new(0.8, Color32::from_gray(90)),
                    );
                }
                if k == 4 {
                    painter.hline(
                        bar.x_range(),
                        bar.bottom(),
                        Stroke::new(0.8, Color32::from_gray(90)),
                    );
                }
            }
            painter.text(
                pos2(frame.left() + 36.0, y),
                Align2::LEFT_TOP,
                label,
                font.clone(),
                ink,
            );
            y += row;
        }
        top = frame.bottom() + 8.0;
    }
}

/// The text box where a new map label is typed (Project ▸ Add ▸ Map Label):
/// Enter adds the label, Escape or clicking elsewhere cancels it.
fn label_editor(app: &mut EpanetApp, ui: &mut Ui, rect: Rect) {
    let Some((pos, mut text)) = app.label_edit.take() else {
        return;
    };
    let at = app.view.to_screen(rect, pos);
    let edit_rect = Rect::from_min_size(at, vec2(160.0, 26.0));
    let id = ui.id().with("map_label_editor");
    let resp = ui.put(
        edit_rect,
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .hint_text("Label text"),
    );
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    if resp.lost_focus() {
        let text = text.trim().replace('"', "'");
        if enter && !text.is_empty() {
            app.checkpoint();
            let mut lines: Vec<String> = app
                .net
                .section_text("LABELS")
                .lines()
                .map(str::to_string)
                .collect();
            lines.push(format!(" {:.2}\t{:.2}\t\"{text}\"", pos[0], pos[1]));
            app.net.replace_section("LABELS", lines);
            app.section_buffer = None;
            app.status = format!("Added the map label \"{text}\"");
        }
        return;
    }
    app.label_edit = Some((pos, text));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Times map frames for a large network:
    /// `WEPANET_BENCH_INP=big.inp cargo test --release -p eui-ui map_frame_time -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn map_frame_time() {
        let Ok(path) = std::env::var("WEPANET_BENCH_INP") else {
            return;
        };
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let bytes = std::fs::read(&path).unwrap();
        let t = std::time::Instant::now();
        app.load_example("bench", &crate::io::decode_text(&bytes));
        app.dialog = crate::app::Dialog::None;
        eprintln!("load {:?}", t.elapsed());
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))),
            ..Default::default()
        };
        for zoom in [0.0, 6.0] {
            for frame in 0..4 {
                let t = std::time::Instant::now();
                let out = ctx.run_ui(input(), |ui| {
                    app.refresh_geometry();
                    if frame == 1 {
                        app.view.zoom_steps(zoom);
                    }
                    egui::CentralPanel::default().show(ui, |ui| show(&mut app, ui));
                });
                let ran = t.elapsed();
                let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
                let verts: usize = prims
                    .iter()
                    .map(|p| match &p.primitive {
                        egui::epaint::Primitive::Mesh(m) => m.vertices.len(),
                        _ => 0,
                    })
                    .sum();
                eprintln!(
                    "zoom {zoom} frame {frame}: ui {ran:?}, total {:?}, {verts} vertices",
                    t.elapsed()
                );
            }
        }
    }
}
