//! The side panels — Map Viewer, Project Explorer, Property Editor — which
//! the user can move. Dragged by its title bar to the left or the
//! right edge of the window a panel docks there, above or below the others on
//! that side (by where it is let go); let go in between it floats as a window
//! of its own. The title bar's context menu does the same, and a double click
//! floats a docked panel or docks a floating one again.

use egui::{Align2, Color32, CursorIcon, Id, Pos2, Rect, Response, Sense, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::app::EpanetApp;

/// A movable panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    MapViewer,
    ProjectExplorer,
    PropertyEditor,
}

impl Pane {
    const ALL: [Pane; 3] = [Pane::MapViewer, Pane::ProjectExplorer, Pane::PropertyEditor];

    fn width(self) -> f32 {
        match self {
            Pane::MapViewer => 250.0,
            Pane::ProjectExplorer | Pane::PropertyEditor => 300.0,
        }
    }

    /// Its height above the next panel on its side, at first.
    fn height(self) -> f32 {
        match self {
            Pane::ProjectExplorer => 428.0,
            Pane::MapViewer | Pane::PropertyEditor => 300.0,
        }
    }

    fn fill(self, app: &EpanetApp, ui: &Ui) -> Color32 {
        match self {
            Pane::MapViewer => app.palette().form,
            // the tree sits on white, as in EPANET-UI
            Pane::ProjectExplorer if ui.visuals().dark_mode => ui.visuals().extreme_bg_color,
            Pane::ProjectExplorer => Color32::WHITE,
            Pane::PropertyEditor => ui.visuals().panel_fill,
        }
    }

    fn visible(self, app: &EpanetApp) -> bool {
        match self {
            Pane::MapViewer => app.prefs.show_viewer,
            Pane::ProjectExplorer | Pane::PropertyEditor => app.prefs.show_explorer,
        }
    }

    /// Draws it; returns its title bar (from [`title_bar`]).
    fn draw(self, app: &mut EpanetApp, ui: &mut Ui) -> Response {
        match self {
            Pane::MapViewer => crate::viewer::pane(app, ui),
            Pane::ProjectExplorer => crate::explorer::tree_pane(app, ui),
            Pane::PropertyEditor => crate::explorer::editor_pane(app, ui),
        }
    }
}

/// Where the panels are, top to bottom on each side.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub left: Vec<Pane>,
    pub right: Vec<Pane>,
    pub floating: Vec<Pane>,
}

impl Default for Layout {
    /// As in EPANET-UI.
    fn default() -> Self {
        Layout {
            left: vec![Pane::MapViewer],
            right: vec![Pane::ProjectExplorer, Pane::PropertyEditor],
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
}

impl Layout {
    /// Every panel once.
    fn repair(&mut self) {
        let mut seen = Vec::new();
        for list in [&mut self.left, &mut self.right, &mut self.floating] {
            list.retain(|p| {
                let new = !seen.contains(p);
                seen.push(*p);
                new
            });
        }
        let default = Layout::default();
        for p in Pane::ALL.into_iter().filter(|p| !seen.contains(p)) {
            if default.left.contains(&p) {
                self.left.push(p);
            } else {
                self.right.push(p);
            }
        }
    }

    fn put(&mut self, pane: Pane, place: Place) {
        for list in [&mut self.left, &mut self.right, &mut self.floating] {
            list.retain(|p| *p != pane);
        }
        let (list, top) = match place {
            Place::Left(top) => (&mut self.left, top),
            Place::Right(top) => (&mut self.right, top),
            Place::Floating => (&mut self.floating, false),
        };
        if top {
            list.insert(0, pane);
        } else {
            list.push(pane);
        }
    }
}

/// The part of the window's width, at either edge, that docks a panel let go there.
const EDGE: f32 = 0.2;

/// How far a title bar must be dragged to move its panel.
const MOVE: f32 = 40.0;

/// Shows the side panels and the floating ones.
pub(crate) fn show(app: &mut EpanetApp, ui: &mut Ui) {
    app.prefs.dock.repair();
    let layout = app.prefs.dock.clone();
    let ctx = ui.ctx().clone();
    let mut moves = Vec::new();
    for (left, panes) in [(true, &layout.left), (false, &layout.right)] {
        let panes: Vec<Pane> = panes.iter().copied().filter(|p| p.visible(app)).collect();
        if panes.is_empty() {
            continue;
        }
        let panel = if left {
            egui::Panel::left("dock_left")
        } else {
            egui::Panel::right("dock_right")
        };
        let width = panes.iter().map(|p| p.width()).fold(0.0, f32::max);
        panel
            .default_size(width)
            .size_range(220.0..=520.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(ui.visuals().panel_fill))
            .show(ui, |ui| {
                for (p, bar) in stack(app, ui, &panes) {
                    let here = if left {
                        Place::Left(false)
                    } else {
                        Place::Right(false)
                    };
                    if let Some(to) = moved(&ctx, &bar, here, None, p) {
                        moves.push((p, to));
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
            .show(&ctx, |ui| p.draw(app, ui));
        if let Some(r) = shown
            && let Some(bar) = r.inner
            && let Some(to) = moved(&ctx, &bar, Place::Floating, Some(r.response.rect), p)
        {
            moves.push((p, to));
        }
    }
    for (p, to) in moves {
        app.prefs.dock.put(p, to);
    }
}

/// `panes` one above the other, with a splitter between each two.
pub(crate) fn stack(app: &mut EpanetApp, ui: &mut Ui, panes: &[Pane]) -> Vec<(Pane, Response)> {
    let mut bars = Vec::new();
    for (k, &p) in panes.iter().enumerate() {
        let fill = p.fill(app, ui);
        if k + 1 < panes.len() {
            // Not below the default: the first frame (in the browser) can be
            // laid out before the window has its real height.
            let max = (ui.available_height() - 120.0).max(p.height());
            let bar = egui::Panel::top(Id::new("dock_stack").with(p))
                .default_size(p.height())
                .size_range(60.0..=max)
                .resizable(true)
                .frame(egui::Frame::NONE.fill(fill))
                .show(ui, |ui| p.draw(app, ui))
                .inner;
            bars.push((p, bar));
        } else {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 0.0, fill);
            bars.push((p, p.draw(app, ui)));
        }
    }
    bars
}

/// A panel's title bar: drag it to move the panel, right-click it to choose
/// where it goes.
pub(crate) fn title_bar(ui: &mut Ui, title: &str, color: Color32) -> Response {
    let title = crate::i18n::tr(title);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click_and_drag());
    ui.painter().rect_filled(rect, 0.0, color);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        title,
        crate::fonts::bold(13.0),
        Color32::WHITE,
    );
    let grabbing = resp.dragged();
    let resp = resp.on_hover_cursor(if grabbing {
        CursorIcon::Grabbing
    } else {
        CursorIcon::Grab
    });
    resp.on_hover_text(crate::i18n::tr(
        "Drag to the left or the right of the window to dock it there, or between to float it. Right-click for more.",
    ))
}

/// Where the title bar's drag, double click or menu puts the panel.
fn moved(
    ctx: &egui::Context,
    bar: &Response,
    here: Place,
    window: Option<Rect>,
    pane: Pane,
) -> Option<Place> {
    let float_pos = Id::new("dock_float").with(pane).with("pos");
    let floating = here == Place::Floating;
    let mut to = None;
    bar.context_menu(|ui| {
        for (place, label) in [
            (Place::Left(false), "Dock Left"),
            (Place::Right(false), "Dock Right"),
            (Place::Floating, "Float"),
        ] {
            if ui
                .add_enabled(place != here, egui::Button::new(crate::i18n::tr(label)))
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
        show_target(ctx, p, floating);
    }
    if bar.drag_stopped()
        && far
        && let Some(p) = pointer
    {
        let place = target(ctx, p);
        if !(floating && place == Place::Floating) {
            if place == Place::Floating {
                ctx.data_mut(|d| d.insert_temp(float_pos, p - vec2(40.0, 14.0)));
            }
            to = Some(place);
        }
    }
    to
}

/// Where a panel let go at `p` goes.
fn target(ctx: &egui::Context, p: Pos2) -> Place {
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
fn show_target(ctx: &egui::Context, p: Pos2, floating: bool) {
    let r = ctx.content_rect();
    let w = r.width() * EDGE;
    let half = |top: bool| {
        if top {
            (r.top(), r.center().y)
        } else {
            (r.center().y, r.bottom())
        }
    };
    let zone = match target(ctx, p) {
        Place::Left(top) => {
            let (y0, y1) = half(top);
            Rect::from_min_max(egui::pos2(r.left(), y0), egui::pos2(r.left() + w, y1))
        }
        Place::Right(top) => {
            let (y0, y1) = half(top);
            Rect::from_min_max(egui::pos2(r.right() - w, y0), egui::pos2(r.right(), y1))
        }
        // a docked panel let go in between floats there
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

    #[test]
    fn panels_start_as_in_epanet_ui() {
        let (mut app, ctx) = app();
        let t = frame(&mut app, &ctx, vec![]);
        let (viewer, explorer, editor) = (
            at(&t, "Map Viewer"),
            at(&t, "Project Explorer"),
            at(&t, "Property Editor"),
        );
        assert!(viewer.center().x < 400.0);
        assert!(explorer.center().x > 1000.0);
        // the Property Editor under the Project Explorer
        assert!((editor.center().x - explorer.center().x).abs() < 1.0);
        assert!(
            editor.top() > explorer.bottom() + 100.0,
            "{explorer:?} {editor:?}"
        );
    }

    #[test]
    fn panels_move_by_their_title_bars() {
        let (mut app, ctx) = app();
        // the Property Editor let go in the middle: a window of its own
        drag(&mut app, &ctx, "Property Editor", egui::pos2(700.0, 300.0));
        assert_eq!(app.prefs.dock.floating, [Pane::PropertyEditor]);
        let t = frame(&mut app, &ctx, vec![]);
        let bar = at(&t, "Property Editor");
        assert!((bar.center().y - 300.0).abs() < 40.0, "{bar:?}");
        at(&t, "Title / Notes"); // its contents came along
        // the Project Explorer to the left edge, below the Map Viewer
        drag(&mut app, &ctx, "Project Explorer", egui::pos2(30.0, 700.0));
        assert_eq!(
            app.prefs.dock.left,
            [Pane::MapViewer, Pane::ProjectExplorer]
        );
        assert!(app.prefs.dock.right.is_empty());
        let t = frame(&mut app, &ctx, vec![]);
        let (viewer, explorer) = (at(&t, "Map Viewer"), at(&t, "Project Explorer"));
        assert!(explorer.center().x < 400.0 && explorer.top() > viewer.bottom());
        // the Property Editor back, to the top of the right edge
        drag(&mut app, &ctx, "Property Editor", egui::pos2(1380.0, 200.0));
        assert_eq!(app.prefs.dock.right, [Pane::PropertyEditor]);
        assert!(
            at(&frame(&mut app, &ctx, vec![]), "Property Editor")
                .center()
                .x
                > 1000.0
        );
        // a click on a title bar moves nothing
        let before = app.prefs.dock.clone();
        let to = at(&frame(&mut app, &ctx, vec![]), "Map Viewer").center() + vec2(5.0, 0.0);
        drag(&mut app, &ctx, "Map Viewer", to);
        assert_eq!(app.prefs.dock, before);
    }

    #[test]
    fn layouts_are_repaired_and_changed() {
        let mut l = Layout {
            left: vec![Pane::MapViewer, Pane::MapViewer],
            right: vec![],
            floating: vec![Pane::PropertyEditor],
        };
        l.repair();
        assert_eq!(l.left, [Pane::MapViewer]);
        assert_eq!(l.right, [Pane::ProjectExplorer]);
        l.put(Pane::PropertyEditor, Place::Right(true));
        assert_eq!(l.right, [Pane::PropertyEditor, Pane::ProjectExplorer]);
        assert!(l.floating.is_empty());
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
        assert_eq!(target(&ctx, egui::pos2(100.0, 10.0)), Place::Left(true));
        assert_eq!(target(&ctx, egui::pos2(100.0, 700.0)), Place::Left(false));
        assert_eq!(target(&ctx, egui::pos2(500.0, 10.0)), Place::Floating);
        assert_eq!(target(&ctx, egui::pos2(900.0, 500.0)), Place::Right(false));
    }
}
