//! The side panels — Map Viewer, Map Layers, Project Explorer, Property
//! Editor — which the user can move. Panels in one place show as tabs, one at
//! a time. Dragged by its title bar (or tab) to the left or the right edge of
//! the window a panel docks there, above or below the others on that side (by
//! where it is let go); onto another panel's title bar it joins it as a tab;
//! let go elsewhere it floats as a window of its own. The title bar's context
//! menu does the same, and a double click floats a docked panel or docks a
//! floating one again.

use egui::{Align2, Color32, CursorIcon, Id, Pos2, Rect, Response, Sense, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::app::EpanetApp;

/// A movable panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    MapViewer,
    ProjectExplorer,
    PropertyEditor,
    Layers,
    Identify,
}

impl Pane {
    const ALL: [Pane; 5] = [
        Pane::MapViewer,
        Pane::ProjectExplorer,
        Pane::PropertyEditor,
        Pane::Layers,
        Pane::Identify,
    ];

    fn title(self) -> &'static str {
        match self {
            Pane::MapViewer => "Map Viewer",
            Pane::ProjectExplorer => "Project Explorer",
            Pane::PropertyEditor => "Property Editor",
            Pane::Layers => "Map Layers",
            Pane::Identify => "Identify Results",
        }
    }

    fn width(self) -> f32 {
        match self {
            Pane::MapViewer | Pane::Layers => 250.0,
            Pane::ProjectExplorer | Pane::PropertyEditor | Pane::Identify => 300.0,
        }
    }

    /// Its height above the next panel on its side, at first.
    fn height(self) -> f32 {
        match self {
            Pane::ProjectExplorer | Pane::Identify => 428.0,
            Pane::MapViewer => 560.0,
            Pane::PropertyEditor | Pane::Layers => 300.0,
        }
    }

    fn fill(self, app: &EpanetApp, ui: &Ui) -> Color32 {
        match self {
            Pane::MapViewer | Pane::Layers => app.palette().form,
            // the tree sits on white, as in EPANET-UI
            Pane::ProjectExplorer if ui.visuals().dark_mode => ui.visuals().extreme_bg_color,
            Pane::ProjectExplorer => Color32::WHITE,
            Pane::PropertyEditor | Pane::Identify => ui.visuals().panel_fill,
        }
    }

    fn visible(self, app: &EpanetApp) -> bool {
        match self {
            Pane::MapViewer => app.prefs.show_viewer,
            Pane::ProjectExplorer | Pane::PropertyEditor => app.prefs.show_explorer,
            Pane::Layers => !app.prefs.hide_layers,
            Pane::Identify => app.prefs.show_explorer && !app.prefs.hide_identify,
        }
    }

    /// Whether its title bar (or tab) has a cross that closes it.
    fn closable(self) -> bool {
        self == Pane::Identify
    }

    /// Closes it (the Identify tool opens it again).
    fn close(self, app: &mut EpanetApp) {
        if self == Pane::Identify {
            app.prefs.hide_identify = true;
        }
    }

    fn draw(self, app: &mut EpanetApp, ui: &mut Ui) {
        match self {
            Pane::MapViewer => crate::viewer::pane(app, ui),
            Pane::ProjectExplorer => crate::explorer::tree_pane(app, ui),
            Pane::PropertyEditor => crate::explorer::editor_pane(app, ui),
            Pane::Layers => crate::layers::pane(app, ui),
            Pane::Identify => crate::layers::identify_pane(app, ui),
        }
    }
}

/// Panels in one place, shown as tabs: one at a time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub panes: Vec<Pane>,
    /// The one shown.
    pub active: Pane,
}

impl Group {
    fn of(panes: &[Pane]) -> Self {
        Group {
            panes: panes.to_vec(),
            active: panes[0],
        }
    }
}

/// Where the panels are, top to bottom on each side.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub left: Vec<Group>,
    pub right: Vec<Group>,
    pub floating: Vec<Pane>,
}

impl Default for Layout {
    /// As in EPANET-UI, with the Map Layers a tab beside the Map Viewer.
    fn default() -> Self {
        Layout {
            left: vec![Group::of(&[Pane::MapViewer, Pane::Layers])],
            right: vec![
                Group::of(&[Pane::ProjectExplorer, Pane::Identify]),
                Group::of(&[Pane::PropertyEditor]),
            ],
            floating: Vec::new(),
        }
    }
}

/// Where a panel goes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    /// On the left, at the top (or the bottom).
    Left(bool),
    Right(bool),
    Floating,
    /// A tab beside this panel.
    With(Pane),
}

impl Layout {
    fn groups_mut(&mut self) -> impl Iterator<Item = &mut Group> {
        self.left.iter_mut().chain(self.right.iter_mut())
    }

    fn take(&mut self, pane: Pane) {
        for g in self.groups_mut() {
            g.panes.retain(|p| *p != pane);
            if g.active == pane
                && let Some(&first) = g.panes.first()
            {
                g.active = first;
            }
        }
        self.left.retain(|g| !g.panes.is_empty());
        self.right.retain(|g| !g.panes.is_empty());
        self.floating.retain(|p| *p != pane);
    }

    fn contains(&self, pane: Pane) -> bool {
        self.left
            .iter()
            .chain(&self.right)
            .any(|g| g.panes.contains(&pane))
            || self.floating.contains(&pane)
    }

    /// Every panel once.
    fn repair(&mut self) {
        let mut seen = Vec::new();
        for g in self.groups_mut() {
            g.panes.retain(|p| {
                let new = !seen.contains(p);
                seen.push(*p);
                new
            });
            if !g.panes.contains(&g.active)
                && let Some(&first) = g.panes.first()
            {
                g.active = first;
            }
        }
        self.floating.retain(|p| {
            let new = !seen.contains(p);
            seen.push(*p);
            new
        });
        self.left.retain(|g| !g.panes.is_empty());
        self.right.retain(|g| !g.panes.is_empty());
        for p in Pane::ALL {
            if self.contains(p) {
                continue;
            }
            match p {
                Pane::Layers if self.contains(Pane::MapViewer) => {
                    self.put(p, Place::With(Pane::MapViewer));
                    self.activate(Pane::MapViewer);
                }
                Pane::Identify if self.contains(Pane::ProjectExplorer) => {
                    self.put(p, Place::With(Pane::ProjectExplorer));
                    self.activate(Pane::ProjectExplorer);
                }
                Pane::MapViewer | Pane::Layers => self.put(p, Place::Left(false)),
                _ => self.put(p, Place::Right(false)),
            }
        }
    }

    fn put(&mut self, pane: Pane, place: Place) {
        self.take(pane);
        let (list, top) = match place {
            Place::Left(top) => (&mut self.left, top),
            Place::Right(top) => (&mut self.right, top),
            Place::Floating => {
                self.floating.push(pane);
                return;
            }
            Place::With(other) => {
                let joined = match self.groups_mut().find(|g| g.panes.contains(&other)) {
                    Some(g) => {
                        g.panes.push(pane);
                        g.active = pane;
                        true
                    }
                    None => false,
                };
                if !joined {
                    self.floating.push(pane);
                }
                return;
            }
        };
        let g = Group::of(&[pane]);
        if top {
            list.insert(0, g);
        } else {
            list.push(g);
        }
    }

    /// Shows `pane` in its place (the tab chosen).
    pub fn activate(&mut self, pane: Pane) {
        if let Some(g) = self.groups_mut().find(|g| g.panes.contains(&pane)) {
            g.active = pane;
        }
    }
}

/// The part of the window's width, at either edge, that docks a panel let go there.
const EDGE: f32 = 0.2;

/// How far a title bar must be dragged to move its panel.
const MOVE: f32 = 40.0;

const BAR_H: f32 = 28.0;

/// Where the title bars of the docked groups were last drawn, and their
/// panels: the places a panel can join as a tab.
#[derive(Clone, Default)]
struct Bars(Vec<(Vec<Pane>, Rect)>);

/// Shows the side panels and the floating ones.
pub(crate) fn show(app: &mut EpanetApp, ui: &mut Ui) {
    app.prefs.docks.repair();
    let layout = app.prefs.docks.clone();
    let ctx = ui.ctx().clone();
    let bars_id = Id::new("dock_bars");
    let last_bars: Bars = ctx.data(|d| d.get_temp(bars_id)).unwrap_or_default();
    let mut bars = Bars::default();
    let mut moves = Vec::new();
    let mut chosen = None;
    for (left, groups) in [(true, &layout.left), (false, &layout.right)] {
        let shown = visible_groups(app, groups);
        if shown.is_empty() {
            continue;
        }
        let panel = if left {
            egui::Panel::left("dock_left")
        } else {
            egui::Panel::right("dock_right")
        };
        let width = shown
            .iter()
            .flat_map(|(panes, _)| panes.iter().map(|p| p.width()))
            .fold(0.0, f32::max);
        panel
            .default_size(width)
            .size_range(220.0..=520.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(ui.visuals().panel_fill))
            .show(ui, |ui| {
                let here = if left {
                    Place::Left(false)
                } else {
                    Place::Right(false)
                };
                for (panes, tabs, rect) in stack(app, ui, &shown) {
                    bars.0.push((panes.clone(), rect));
                    for (p, r) in tabs {
                        if r.clicked() && panes.len() > 1 {
                            chosen = Some(p);
                        }
                        if let Some(to) = moved(&ctx, &r, here, None, p, &panes, &last_bars) {
                            moves.push((p, to));
                        }
                    }
                }
            });
    }
    let floating: Vec<Pane> = layout
        .floating
        .iter()
        .copied()
        .filter(|p| p.visible(app))
        .collect();
    for p in floating {
        let id = Id::new("dock_float").with(p);
        let screen = ctx.content_rect();
        let pos = ctx
            .data(|d| d.get_temp::<Pos2>(id.with("pos")))
            .unwrap_or_else(|| screen.center() - vec2(p.width() / 2.0, 280.0));
        let fill = p.fill(app, ui);
        let menu = app.palette().menu;
        let shown = egui::Window::new(format!("{p:?}"))
            .id(id)
            .title_bar(false)
            .current_pos(pos)
            .default_size([p.width(), 560.0])
            .min_width(220.0)
            .resizable(true)
            .constrain(true)
            .frame(
                egui::Frame::window(&ctx.global_style())
                    .fill(fill)
                    .inner_margin(0),
            )
            .show(&ctx, |ui| {
                let bar = title_bar(ui, p.title(), menu);
                if p.closable() && cross(ui, bar.rect, p) {
                    p.close(app);
                }
                p.draw(app, ui);
                bar
            });
        if let Some(r) = shown
            && let Some(bar) = r.inner
            && let Some(to) = moved(
                &ctx,
                &bar,
                Place::Floating,
                Some(r.response.rect),
                p,
                &[p],
                &last_bars,
            )
        {
            moves.push((p, to));
        }
    }
    ctx.data_mut(|d| d.insert_temp(bars_id, bars));
    if let Some(p) = chosen {
        app.prefs.docks.activate(p);
    }
    for (p, to) in moves {
        app.prefs.docks.put(p, to);
    }
}

/// The groups to show: their visible panels and the one shown.
fn visible_groups(app: &EpanetApp, groups: &[Group]) -> Vec<(Vec<Pane>, Pane)> {
    groups
        .iter()
        .filter_map(|g| {
            let panes: Vec<Pane> = g.panes.iter().copied().filter(|p| p.visible(app)).collect();
            let active = if panes.contains(&g.active) {
                g.active
            } else {
                *panes.first()?
            };
            Some((panes, active))
        })
        .collect()
}

/// A group's place drawn: its panels, their title bars or tabs, and where
/// those are.
type Drawn = (Vec<Pane>, Vec<(Pane, Response)>, Rect);

/// `groups` one above the other, with a splitter between each two, each
/// showing its title bar (or tabs) and the panel chosen.
pub(crate) fn stack(app: &mut EpanetApp, ui: &mut Ui, groups: &[(Vec<Pane>, Pane)]) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for (k, (panes, active)) in groups.iter().enumerate() {
        let p = *active;
        let fill = p.fill(app, ui);
        let body = |app: &mut EpanetApp, ui: &mut Ui| {
            let (tabs, rect, closed) = header(app, ui, panes, p);
            if let Some(c) = closed {
                c.close(app);
            }
            p.draw(app, ui);
            (tabs, rect)
        };
        let (tabs, rect) = if k + 1 < groups.len() {
            // Not below the default: the first frame (in the browser) can be
            // laid out before the window has its real height.
            let height = panes.iter().map(|q| q.height()).fold(0.0, f32::max);
            let max = (ui.available_height() - 120.0).max(height);
            egui::Panel::top(Id::new("dock_stack").with(panes[0]))
                .default_size(height)
                .size_range(60.0..=max)
                .resizable(true)
                .frame(egui::Frame::NONE.fill(fill))
                .show(ui, |ui| {
                    // a panel with little in it (a tab) keeps the place's
                    // height: the panel takes the height of what it holds
                    ui.set_min_height(ui.available_height());
                    body(app, ui)
                })
                .inner
        } else {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 0.0, fill);
            body(app, ui)
        };
        drawn.push((panes.clone(), tabs, rect));
    }
    drawn
}

/// A group's title bar, or its tabs when it holds more than one panel; and
/// the panel whose cross was clicked.
fn header(
    app: &EpanetApp,
    ui: &mut Ui,
    panes: &[Pane],
    active: Pane,
) -> (Vec<(Pane, Response)>, Rect, Option<Pane>) {
    let menu = app.palette().menu;
    let mut closed = None;
    if panes.len() == 1 {
        let r = title_bar(ui, panes[0].title(), menu);
        let rect = r.rect;
        if panes[0].closable() && cross(ui, rect, panes[0]) {
            closed = Some(panes[0]);
        }
        return (vec![(panes[0], r)], rect, closed);
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_H), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, menu.gamma_multiply(0.55));
    let w = rect.width() / panes.len() as f32;
    let mut tabs = Vec::new();
    for (k, &p) in panes.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(w * k as f32, 0.0), vec2(w, BAR_H));
        let resp = ui.interact(r, Id::new("dock_tab").with(p), Sense::click_and_drag());
        let on = p == active;
        if on {
            painter.rect_filled(r, 0.0, menu);
        } else if resp.hovered() {
            painter.rect_filled(r, 0.0, menu.gamma_multiply(0.75));
        }
        if k > 0 {
            painter.vline(
                r.left(),
                r.y_range().shrink(5.0),
                egui::Stroke::new(1.0, Color32::from_white_alpha(90)),
            );
        }
        let title = crate::i18n::tr(p.title());
        let font = if on {
            crate::fonts::bold(13.0)
        } else {
            egui::FontId::proportional(13.0)
        };
        let color = if on {
            Color32::WHITE
        } else {
            Color32::from_white_alpha(225)
        };
        // the title clear of the cross
        let text = if p.closable() {
            Rect::from_min_max(r.min, egui::pos2(r.right() - 24.0, r.bottom()))
        } else {
            r
        };
        painter.with_clip_rect(text.shrink2(vec2(4.0, 0.0))).text(
            text.center(),
            Align2::CENTER_CENTER,
            title,
            font,
            color,
        );
        tabs.push((p, grab(resp)));
        if p.closable() && cross(ui, r, p) {
            closed = Some(p);
        }
    }
    (tabs, rect, closed)
}

/// The cross at the right of a title bar or tab that closes its panel.
fn cross(ui: &mut Ui, bar: Rect, pane: Pane) -> bool {
    let r = Rect::from_center_size(
        egui::pos2(bar.right() - 14.0, bar.center().y),
        vec2(18.0, 18.0),
    );
    let resp = ui.interact(r, Id::new("dock_close").with(pane), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(r, 3.0, Color32::from_white_alpha(70));
    }
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        "×",
        egui::FontId::proportional(16.0),
        Color32::WHITE,
    );
    resp.on_hover_text(crate::i18n::tr("Close")).clicked()
}

/// A title bar's cursor and tooltip.
fn grab(resp: Response) -> Response {
    let grabbing = resp.dragged();
    resp.on_hover_cursor(if grabbing {
        CursorIcon::Grabbing
    } else {
        CursorIcon::Grab
    })
    .on_hover_text(crate::i18n::tr(
        "Drag to the left or the right of the window to dock it there, onto another panel's title to make it a tab, or elsewhere to float it. Right-click for more.",
    ))
}

/// A panel's title bar: drag it to move the panel, right-click it to choose
/// where it goes.
pub(crate) fn title_bar(ui: &mut Ui, title: &str, color: Color32) -> Response {
    let title = crate::i18n::tr(title);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), BAR_H), Sense::click_and_drag());
    ui.painter().rect_filled(rect, 0.0, color);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        title,
        crate::fonts::bold(13.0),
        Color32::WHITE,
    );
    grab(resp)
}

/// Where the title bar's drag, double click or menu puts the panel.
fn moved(
    ctx: &egui::Context,
    bar: &Response,
    here: Place,
    window: Option<Rect>,
    pane: Pane,
    group: &[Pane],
    bars: &Bars,
) -> Option<Place> {
    let float_pos = Id::new("dock_float").with(pane).with("pos");
    let floating = here == Place::Floating;
    let mut to = None;
    bar.context_menu(|ui| {
        let alone = group.len() == 1;
        for (place, label) in [
            (Place::Left(false), "Dock Left"),
            (Place::Right(false), "Dock Right"),
            (Place::Floating, "Float"),
        ] {
            if ui
                .add_enabled(
                    place != here || !alone,
                    egui::Button::new(crate::i18n::tr(label)),
                )
                .clicked()
            {
                to = Some(place);
                ui.close();
            }
        }
    });
    if bar.double_clicked() {
        to = Some(match window {
            Some(w) if w.center().x < ctx.content_rect().center().x => Place::Left(false),
            Some(_) => Place::Right(false),
            None => Place::Floating,
        });
    }
    let pointer = ctx.pointer_latest_pos();
    // where the drag began (the input forgets it once the button is up)
    let start = Id::new("dock_drag_start").with(pane);
    if bar.drag_started()
        && let Some(o) = ctx.input(|i| i.pointer.press_origin())
    {
        ctx.data_mut(|d| d.insert_temp(start, o));
    }
    let far = match (ctx.data(|d| d.get_temp::<Pos2>(start)), pointer) {
        (Some(o), Some(p)) => o.distance(p) > MOVE,
        _ => false,
    };
    if bar.dragged()
        && let Some(w) = window
    {
        // a floating panel follows the pointer
        let pos = w.min + bar.drag_delta();
        ctx.data_mut(|d| d.insert_temp(float_pos, pos));
    }
    if bar.dragged()
        && far
        && let Some(p) = pointer
    {
        show_target(ctx, p, floating, pane, bars);
    }
    if bar.drag_stopped()
        && far
        && let Some(p) = pointer
    {
        let place = target(ctx, p, pane, bars);
        if !(floating && place == Place::Floating) {
            if place == Place::Floating {
                ctx.data_mut(|d| d.insert_temp(float_pos, p - vec2(40.0, 14.0)));
            }
            to = Some(place);
        }
    }
    to
}

/// The title bar under `p` of a group `pane` is not in, if any.
fn bar_at(p: Pos2, pane: Pane, bars: &Bars) -> Option<(Pane, Rect)> {
    bars.0
        .iter()
        .find(|(panes, r)| r.contains(p) && !panes.contains(&pane))
        .map(|(panes, r)| (panes[0], *r))
}

/// Where a panel let go at `p` goes.
fn target(ctx: &egui::Context, p: Pos2, pane: Pane, bars: &Bars) -> Place {
    if let Some((other, _)) = bar_at(p, pane, bars) {
        return Place::With(other);
    }
    let r = ctx.content_rect();
    let top = p.y < r.center().y;
    if p.x < r.left() + r.width() * EDGE {
        Place::Left(top)
    } else if p.x > r.right() - r.width() * EDGE {
        Place::Right(top)
    } else {
        Place::Floating
    }
}

/// While a panel is dragged: where letting it go would put it.
fn show_target(ctx: &egui::Context, p: Pos2, floating: bool, pane: Pane, bars: &Bars) {
    let r = ctx.content_rect();
    let w = r.width() * EDGE;
    let half = |top: bool| {
        if top {
            (r.top(), r.center().y)
        } else {
            (r.center().y, r.bottom())
        }
    };
    let zone = match target(ctx, p, pane, bars) {
        Place::With(_) => bar_at(p, pane, bars)
            .map(|(_, r)| r)
            .unwrap_or(Rect::NOTHING),
        Place::Left(top) => {
            let (y0, y1) = half(top);
            Rect::from_min_max(egui::pos2(r.left(), y0), egui::pos2(r.left() + w, y1))
        }
        Place::Right(top) => {
            let (y0, y1) = half(top);
            Rect::from_min_max(egui::pos2(r.right() - w, y0), egui::pos2(r.right(), y1))
        }
        // a docked panel let go elsewhere floats there
        Place::Floating if !floating => {
            Rect::from_min_size(p - vec2(40.0, 14.0), vec2(260.0, 320.0))
        }
        Place::Floating => return,
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("dock_target"),
    ));
    let accent = ctx.global_style().visuals.selection.bg_fill;
    painter.rect(
        zone.shrink(2.0),
        4.0,
        accent.gamma_multiply(0.25),
        egui::Stroke::new(2.0, accent),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App as _;

    /// The whole window, run for a frame with `events`: each text drawn and where.
    fn frame(
        app: &mut EpanetApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<(String, Rect)> {
        let mut f = eframe::Frame::_new_kittest();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            app.logic(ui.ctx(), &mut f);
            app.ui(ui, &mut f);
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

    fn at(texts: &[(String, Rect)], s: &str) -> Rect {
        texts
            .iter()
            .find(|t| t.0 == s)
            .unwrap_or_else(|| panic!("no {s:?}"))
            .1
    }

    /// Drags the title bar called `title` to `to`, over a few frames.
    fn drag(app: &mut EpanetApp, ctx: &egui::Context, title: &str, to: Pos2) {
        let from = at(&frame(app, ctx, vec![]), title).center();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(app, ctx, vec![egui::Event::PointerMoved(from)]);
        frame(app, ctx, vec![button(from, true)]);
        for k in 1..=4 {
            let p = from + (to - from) * (k as f32 / 4.0);
            frame(app, ctx, vec![egui::Event::PointerMoved(p)]);
        }
        frame(app, ctx, vec![button(to, false)]);
        frame(app, ctx, vec![]);
    }

    fn app() -> (EpanetApp, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.load_example("Net1", eui_core::examples::NET1);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        (app, ctx)
    }

    /// Each side's groups, as their panels.
    fn sides(l: &Layout) -> (Vec<Vec<Pane>>, Vec<Vec<Pane>>) {
        let p = |gs: &[Group]| gs.iter().map(|g| g.panes.clone()).collect();
        (p(&l.left), p(&l.right))
    }

    #[test]
    fn panels_start_as_in_epanet_ui() {
        let (mut app, ctx) = app();
        let t = frame(&mut app, &ctx, vec![]);
        let (viewer, layers, explorer, editor) = (
            at(&t, "Map Viewer"),
            at(&t, "Map Layers"),
            at(&t, "Project Explorer"),
            at(&t, "Property Editor"),
        );
        assert!(viewer.center().x < 400.0);
        // the Map Layers a tab beside the Map Viewer, which shows
        assert!((layers.center().y - viewer.center().y).abs() < 1.0);
        assert!(layers.center().x > viewer.center().x);
        assert!(t.iter().any(|(s, _)| s == "Themes"));
        assert!(explorer.center().x > 1000.0);
        // the Identify Results a tab beside the Project Explorer, which shows
        let results = at(&t, "Identify Results");
        assert!((results.center().y - explorer.center().y).abs() < 1.0);
        assert!(results.center().x > explorer.center().x);
        assert!(t.iter().any(|(s, _)| s == "Title / Notes"));
        // the Property Editor under them
        assert!(editor.left() > 1000.0);
        assert!(
            editor.top() > explorer.bottom() + 100.0,
            "{explorer:?} {editor:?}"
        );
    }

    #[test]
    fn tabs_show_one_panel_at_a_time() {
        let (mut app, ctx) = app();
        let click = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let tab = at(&frame(&mut app, &ctx, vec![]), "Map Layers").center();
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(tab)]);
        frame(&mut app, &ctx, vec![click(tab, true)]);
        frame(&mut app, &ctx, vec![click(tab, false)]);
        assert_eq!(app.prefs.docks.left[0].active, Pane::Layers);
        let t = frame(&mut app, &ctx, vec![]);
        assert!(
            !t.iter().any(|(s, _)| s == "Themes"),
            "the Map Viewer is hidden"
        );
        assert!(t.iter().any(|(s, _)| s.starts_with("No layers yet")));
    }

    /// A short panel shown as a tab (the Identify Results before any click)
    /// leaves its group's height as it was.
    #[test]
    fn a_short_tab_keeps_the_group_height() {
        let (mut app, ctx) = app();

        let editor = at(&frame(&mut app, &ctx, vec![]), "Property Editor").top();
        app.prefs.docks.activate(Pane::Identify);
        for _ in 0..4 {
            frame(&mut app, &ctx, vec![]);
        }
        app.prefs.docks.activate(Pane::ProjectExplorer);
        frame(&mut app, &ctx, vec![]);
        let after = at(&frame(&mut app, &ctx, vec![]), "Property Editor").top();
        assert!((after - editor).abs() < 2.0, "{editor} -> {after}");
    }

    /// The Identify Results closes with the cross on its tab, and opens again
    /// with the Identify tool.
    #[test]
    fn identify_results_close() {
        let (mut app, ctx) = app();
        let t = frame(&mut app, &ctx, vec![]);
        let cross = at(&t, "×").center();
        let click = |pressed| egui::Event::PointerButton {
            pos: cross,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(cross)]);
        frame(&mut app, &ctx, vec![click(true)]);
        frame(&mut app, &ctx, vec![click(false)]);
        assert!(app.prefs.hide_identify);
        let t = frame(&mut app, &ctx, vec![]);
        assert!(!t.iter().any(|(s, _)| s == "Identify Results"));
        // the Project Explorer stays, with its own title bar
        at(&t, "Project Explorer");
        crate::layers::start_identify(&mut app);
        assert!(!app.prefs.hide_identify);
        at(&frame(&mut app, &ctx, vec![]), "Identify Results");
    }

    #[test]
    fn panels_move_by_their_title_bars() {
        let (mut app, ctx) = app();
        // the Property Editor let go in the middle: a window of its own
        drag(&mut app, &ctx, "Property Editor", egui::pos2(700.0, 300.0));
        assert_eq!(app.prefs.docks.floating, [Pane::PropertyEditor]);
        let t = frame(&mut app, &ctx, vec![]);
        let bar = at(&t, "Property Editor");
        assert!((bar.center().y - 300.0).abs() < 40.0, "{bar:?}");
        at(&t, "Title / Notes"); // its contents came along
        // the Project Explorer to the left edge, below the Map Viewer
        drag(&mut app, &ctx, "Project Explorer", egui::pos2(30.0, 700.0));
        let (left, right) = sides(&app.prefs.docks);
        assert_eq!(
            left,
            [
                vec![Pane::MapViewer, Pane::Layers],
                vec![Pane::ProjectExplorer]
            ]
        );
        // its tab stays
        assert_eq!(right, [vec![Pane::Identify]]);
        let t = frame(&mut app, &ctx, vec![]);
        let (viewer, explorer) = (at(&t, "Map Viewer"), at(&t, "Project Explorer"));
        assert!(explorer.center().x < 400.0 && explorer.top() > viewer.bottom());
        // the Property Editor onto the Project Explorer's title: its tab
        let onto = at(&frame(&mut app, &ctx, vec![]), "Project Explorer").center();
        drag(&mut app, &ctx, "Property Editor", onto);
        let (left, _) = sides(&app.prefs.docks);
        assert_eq!(left[1], [Pane::ProjectExplorer, Pane::PropertyEditor]);
        assert_eq!(app.prefs.docks.left[1].active, Pane::PropertyEditor);
        assert!(app.prefs.docks.floating.is_empty());
        // and out again, to the top of the right edge
        drag(&mut app, &ctx, "Property Editor", egui::pos2(1380.0, 200.0));
        let (left, right) = sides(&app.prefs.docks);
        assert_eq!(right, [vec![Pane::PropertyEditor], vec![Pane::Identify]]);
        assert_eq!(left[1], [Pane::ProjectExplorer]);
        assert!(
            at(&frame(&mut app, &ctx, vec![]), "Property Editor")
                .center()
                .x
                > 1000.0
        );
        // a click on a title bar moves nothing
        let before = app.prefs.docks.clone();
        let to = at(&frame(&mut app, &ctx, vec![]), "Map Viewer").center() + vec2(5.0, 0.0);
        drag(&mut app, &ctx, "Map Viewer", to);
        assert_eq!(app.prefs.docks.left, before.left);
        assert_eq!(app.prefs.docks.right, before.right);
    }

    #[test]
    fn layouts_are_repaired_and_changed() {
        let mut l = Layout {
            left: vec![Group::of(&[Pane::MapViewer, Pane::MapViewer])],
            right: vec![],
            floating: vec![Pane::PropertyEditor],
        };
        l.repair();
        let (left, right) = sides(&l);
        // the Map Layers joins the Map Viewer as a tab
        assert_eq!(left, [vec![Pane::MapViewer, Pane::Layers]]);
        assert_eq!(l.left[0].active, Pane::MapViewer);
        // the Identify Results joins the Project Explorer as a tab
        assert_eq!(right, [vec![Pane::ProjectExplorer, Pane::Identify]]);
        l.put(Pane::Identify, Place::Floating);
        l.put(Pane::PropertyEditor, Place::Right(true));
        let (_, right) = sides(&l);
        assert_eq!(
            right,
            [vec![Pane::PropertyEditor], vec![Pane::ProjectExplorer]]
        );
        assert_eq!(l.floating, [Pane::Identify]);
        // the last tab of a group taken away: the group goes
        l.put(Pane::PropertyEditor, Place::With(Pane::ProjectExplorer));
        let (_, right) = sides(&l);
        assert_eq!(right, [vec![Pane::ProjectExplorer, Pane::PropertyEditor]]);
    }

    #[test]
    fn where_a_panel_goes() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                ..Default::default()
            },
            |_| {},
        );
        out.textures_delta.clear();
        let bars = Bars(vec![(
            vec![Pane::ProjectExplorer],
            Rect::from_min_size(egui::pos2(800.0, 50.0), vec2(200.0, 28.0)),
        )]);
        let go = |x, y| target(&ctx, egui::pos2(x, y), Pane::PropertyEditor, &bars);
        assert_eq!(go(100.0, 10.0), Place::Left(true));
        assert_eq!(go(100.0, 700.0), Place::Left(false));
        assert_eq!(go(500.0, 10.0), Place::Floating);
        assert_eq!(go(900.0, 500.0), Place::Right(false));
        assert_eq!(go(900.0, 60.0), Place::With(Pane::ProjectExplorer));
        // not onto its own title
        let own = target(&ctx, egui::pos2(900.0, 60.0), Pane::ProjectExplorer, &bars);
        assert_eq!(own, Place::Right(true));
    }
}
