//! Toolbars the user moves by their grip. The small ones, the speed bar
//! (New, Open, Save, zoom, Analyze…) and the Add toolbar (the Add menu's
//! objects), sit in the menu bar, against an edge of the map (vertical on
//! the left and right edges) or float over the map. The ribbon (the selected
//! menu's toolbar) goes to any edge of the window. A double-click on a grip
//! puts the toolbar back in its first place.

use egui::{Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use eui_core::{LinkKind, NodeKind};

use crate::app::{Dialog, EpanetApp, Tool, ViewMode};
use crate::i18n::tr;
use crate::icons::{self, Icon};

/// The movable toolbars.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Bar {
    Speed,
    Add,
}

/// An edge of the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// Where a toolbar is.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Place {
    /// In the menu bar, at its right end.
    MenuBar,
    /// Against an edge of the map, `along` pixels from the edge's start.
    Edge { side: Side, along: f32 },
    /// Over the map, `at` pixels from its top-left corner.
    Float { at: [f32; 2] },
}

/// Where the toolbars are, saved with the preferences.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct Layout {
    pub speed: Place,
    pub add: Place,
    /// The Add toolbar is shown (Add ▸ Show as Toolbar).
    pub show_add: bool,
    /// The window edge the ribbon (the selected menu's toolbar) is on.
    pub ribbon: Side,
    /// The ribbon is hidden, leaving the menu tabs (its arrow, Ctrl+F1).
    pub ribbon_collapsed: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            speed: Bar::Speed.home(),
            add: Bar::Add.home(),
            show_add: true,
            ribbon: Side::Top,
            ribbon_collapsed: false,
        }
    }
}

impl Layout {
    pub(crate) fn place(&self, bar: Bar) -> Place {
        match bar {
            Bar::Speed => self.speed,
            Bar::Add => self.add,
        }
    }
    fn place_mut(&mut self, bar: Bar) -> &mut Place {
        match bar {
            Bar::Speed => &mut self.speed,
            Bar::Add => &mut self.add,
        }
    }
    fn shown(&self, bar: Bar) -> bool {
        bar == Bar::Speed || self.show_add
    }
}

impl Bar {
    /// Where the toolbar starts, and goes back to on a double-click.
    fn home(self) -> Place {
        match self {
            Bar::Speed => Place::MenuBar,
            Bar::Add => Place::Edge {
                side: Side::Top,
                along: 8.0,
            },
        }
    }
}

/// What the toolbars need between frames.
pub(crate) struct State {
    /// The toolbar being dragged and where the pointer holds it.
    drag: Option<(Bar, Vec2)>,
    /// Each toolbar's size when last drawn.
    size: [Vec2; 2],
    /// The menu bar, where a toolbar can be dropped.
    pub menubar: Rect,
    /// Where the pointer took the ribbon's grip, while the ribbon is dragged.
    ribbon_drag: Option<Pos2>,
    /// The ribbon was shown again by the first click of a double-click on a
    /// menu tab, which the double-click must not hide again.
    pub ribbon_opened: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            drag: None,
            size: [Vec2::ZERO; 2],
            menubar: Rect::NOTHING,
            ribbon_drag: None,
            ribbon_opened: false,
        }
    }
}

/// The ribbon's grip, first in the ribbon.
pub(crate) fn ribbon_grip(app: &mut EpanetApp, ui: &mut Ui, vertical: bool) {
    let len = if vertical {
        ui.available_width()
    } else {
        RIBBON_HEIGHT - 4.0
    };
    match grip(ui, vertical, len, RIBBON_TIP) {
        Some(Grip::Drag(p)) => app.bars.ribbon_drag = Some(p),
        Some(Grip::Reset) => {
            app.bars.ribbon_drag = None;
            app.prefs.toolbars.ribbon = Side::Top;
        }
        None => {}
    }
}

/// The arrow that hides the ribbon, pointing to the edge it goes into; or,
/// while it is hidden, the one in the menu bar that shows it again, `color`
/// and `size` square.
pub(crate) fn ribbon_button(app: &mut EpanetApp, ui: &mut Ui, size: Vec2, color: Color32) {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), color.gamma_multiply(0.2));
    }
    let hidden = app.prefs.toolbars.ribbon_collapsed;
    let c = rect.center();
    // a chevron pointing up, turned to the ribbon's edge (from it, to show it)
    let turn = |v: Vec2| {
        let v = if hidden { -v } else { v };
        match app.prefs.toolbars.ribbon {
            Side::Top => v,
            Side::Bottom => vec2(v.x, -v.y),
            Side::Left => vec2(v.y, v.x),
            Side::Right => vec2(-v.y, v.x),
        }
    };
    ui.painter().line(
        vec![
            c + turn(vec2(-4.5, 2.5)),
            c + turn(vec2(0.0, -2.0)),
            c + turn(vec2(4.5, 2.5)),
        ],
        Stroke::new(1.6, color),
    );
    let tip = if hidden {
        "Show the ribbon (Ctrl+F1)"
    } else {
        "Collapse the ribbon (Ctrl+F1)"
    };
    if resp.on_hover_text(tr(tip)).clicked() {
        app.prefs.toolbars.ribbon_collapsed = !hidden;
    }
}

/// The window edge the ribbon goes to when dropped at `p`, if `p` is near one.
fn ribbon_side(p: Pos2, screen: Rect) -> Option<Side> {
    [
        (Side::Top, p.y - screen.top()),
        (Side::Bottom, screen.bottom() - p.y),
        (Side::Left, p.x - screen.left()),
        (Side::Right, screen.right() - p.x),
    ]
    .into_iter()
    .filter(|&(_, d)| d < RIBBON_SNAP)
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(side, _)| side)
}

/// While the ribbon is dragged, shades the window edge it would go to;
/// when the button is released, moves it there.
pub(crate) fn follow_ribbon(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(start) = app.bars.ribbon_drag else {
        return;
    };
    let (down, pointer) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
    let screen = ctx.content_rect();
    // a short drag from the grip, in the window's corner, goes nowhere
    let target = pointer
        .filter(|p| p.distance(start) > 30.0)
        .and_then(|p| ribbon_side(p, screen));
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    if !down {
        if let Some(side) = target {
            app.prefs.toolbars.ribbon = side;
        }
        app.bars.ribbon_drag = None;
        return;
    }
    if let Some(side) = target {
        let below_menu = screen.with_min_y(app.bars.menubar.bottom());
        let r = match side {
            Side::Top => below_menu.with_max_y(below_menu.top() + RIBBON_HEIGHT),
            Side::Bottom => screen.with_min_y(screen.bottom() - RIBBON_HEIGHT),
            Side::Left => below_menu.with_max_x(screen.left() + RIBBON_WIDTH),
            Side::Right => below_menu.with_min_x(screen.right() - RIBBON_WIDTH),
        };
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            Id::new("ribbon_drop"),
        ))
        .rect_filled(r, 0.0, crate::style::ACCENT.gamma_multiply(0.25));
    }
}

/// Distance from a map edge within which a dropped toolbar sticks to it.
const SNAP: f32 = 28.0;
/// Gap between a toolbar and the map edge it is against.
const GAP: f32 = 4.0;
const BUTTON: f32 = 23.0;
const BAR_TIP: &str = "Drag to move the toolbar: to the menu bar, an edge of the map or over the map. Double-click to put it back.";
const RIBBON_TIP: &str =
    "Drag to move the ribbon to an edge of the window. Double-click to put it back at the top.";
/// The ribbon's height at the top or bottom of the window.
pub(crate) const RIBBON_HEIGHT: f32 = 60.0;
/// The ribbon's width at the left or right of the window.
pub(crate) const RIBBON_WIDTH: f32 = 128.0;
/// Distance from a window edge within which the dragged ribbon goes there.
const RIBBON_SNAP: f32 = 140.0;
const BORDER: Color32 = Color32::from_rgb(0xa9, 0xb8, 0xc8);
/// The dots of a grip.
const GRIP: Color32 = Color32::from_rgb(0x6e, 0x7f, 0x92);
const FILL: Color32 = Color32::from_rgb(0xdf, 0xe8, 0xf3);

#[derive(Clone, Copy)]
enum Item {
    Button(Icon, &'static str),
    /// Report an issue on GitHub: apart and in red.
    Issue,
    Separator,
}

fn items(bar: Bar) -> &'static [Item] {
    use Item::*;
    match bar {
        Bar::Speed => &[
            Button(Icon::New, "New"),
            Button(Icon::Open, "Open"),
            Button(Icon::Save, "Save"),
            Button(Icon::ZoomIn, "Zoom In"),
            Button(Icon::ZoomOut, "Zoom Out"),
            Button(Icon::FullExtent, "Full Extent"),
            Button(Icon::Analyze, "Analyze (F5)"),
            Button(Icon::Report, "Report"),
            Button(Icon::Help, "Help"),
            Separator,
            Issue,
        ],
        Bar::Add => &[
            Button(Icon::Pointer, "Select Object"),
            Button(Icon::Region, "Select a Region"),
            Separator,
            Button(Icon::Junction, "Junction Node"),
            Button(Icon::Reservoir, "Reservoir Node"),
            Button(Icon::Tank, "Tank Node"),
            Separator,
            Button(Icon::Pipe, "Pipe Link"),
            Button(Icon::Pump, "Pump Link"),
            Button(Icon::Valve, "Valve Link"),
            Separator,
            Button(Icon::Label, "Map Label"),
            Separator,
            Button(Icon::Undo, "Undo (Ctrl+Z)"),
            Button(Icon::Redo, "Redo (Ctrl+Y)"),
        ],
    }
}

/// The drawing tool an Add toolbar button picks.
fn tool_of(icon: Icon) -> Option<Tool> {
    Some(match icon {
        Icon::Junction => Tool::AddNode(NodeKind::Junction),
        Icon::Reservoir => Tool::AddNode(NodeKind::Reservoir),
        Icon::Tank => Tool::AddNode(NodeKind::Tank),
        Icon::Pipe => Tool::AddLink(LinkKind::Pipe),
        Icon::Pump => Tool::AddLink(LinkKind::Pump),
        Icon::Valve => Tool::AddLink(LinkKind::Valve),
        Icon::Label => Tool::AddLabel,
        Icon::Region => Tool::Region,
        _ => return None,
    })
}

fn run(app: &mut EpanetApp, ctx: &egui::Context, icon: Icon) {
    if let Some(tool) = tool_of(icon) {
        // a second click on the tool in use puts it down
        app.set_tool(if app.tool == tool { Tool::Select } else { tool });
        return;
    }
    match icon {
        Icon::Pointer => {
            app.group = None;
            app.set_tool(Tool::Select);
        }
        Icon::New => app.new_project(),
        Icon::Open => app.open(ctx),
        Icon::Save => app.save(false),
        Icon::ZoomIn => app.view.zoom_steps(1.0),
        Icon::ZoomOut => app.view.zoom_steps(-1.0),
        Icon::FullExtent => {
            app.view_mode = ViewMode::Map;
            app.view.fit_pending = true;
        }
        Icon::Analyze => app.run_simulation(ctx),
        Icon::Report => {
            let kind = app.report_kind;
            crate::reports::open(app, kind);
        }
        Icon::Help => app.dialog = Dialog::About,
        Icon::Undo => app.undo(),
        Icon::Redo => app.redo(),
        _ => {}
    }
}

/// Draws the toolbars that are in the menu bar; `ui` lays out right to left.
pub(crate) fn menubar(app: &mut EpanetApp, ui: &mut Ui) {
    for bar in [Bar::Speed, Bar::Add] {
        if app.prefs.toolbars.shown(bar) && app.prefs.toolbars.place(bar) == Place::MenuBar {
            draw(app, ui, bar, false, true);
        }
    }
}

/// Draws the toolbars that are over the map, and moves the one being dragged.
pub(crate) fn show(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(map) = app.export.map_rect else {
        return;
    };
    follow_drag(app, ctx, map);
    for bar in [Bar::Speed, Bar::Add] {
        if !app.prefs.toolbars.shown(bar) {
            continue;
        }
        let place = app.prefs.toolbars.place(bar);
        let vertical = matches!(
            place,
            Place::Edge {
                side: Side::Left | Side::Right,
                ..
            }
        );
        let size = app.bars.size[bar as usize];
        let Some(pos) = position(place, size, map, ctx.content_rect()) else {
            continue;
        };
        egui::Area::new(Id::new(("toolbar", bar)))
            .fixed_pos(pos)
            .constrain(false)
            .order(egui::Order::Middle)
            .show(ctx, |ui| draw(app, ui, bar, vertical, false));
    }
}

/// The top-left corner of a toolbar over the map, or None in the menu bar.
fn position(place: Place, size: Vec2, map: Rect, screen: Rect) -> Option<Pos2> {
    let along = |a: f32, len: f32, extent: f32| a.clamp(0.0, (len - extent).max(0.0));
    let p = match place {
        Place::MenuBar => return None,
        Place::Edge { side, along: a } => match side {
            Side::Top => pos2(map.left() + along(a, map.width(), size.x), map.top() + GAP),
            Side::Bottom => pos2(
                map.left() + along(a, map.width(), size.x),
                map.bottom() - GAP - size.y,
            ),
            Side::Left => pos2(map.left() + GAP, map.top() + along(a, map.height(), size.y)),
            Side::Right => pos2(
                map.right() - GAP - size.x,
                map.top() + along(a, map.height(), size.y),
            ),
        },
        Place::Float { at: [x, y] } => map.min + vec2(x, y),
    };
    let max = (screen.max - size).max(screen.min);
    Some(p.clamp(screen.min, max))
}

/// Where a toolbar held at `offset` from its corner lands with the pointer
/// at `p`: the menu bar, an edge of the map near the pointer, or over the map.
fn drop_place(p: Pos2, offset: Vec2, map: Rect, menubar: Rect) -> Place {
    if menubar.contains(p) {
        return Place::MenuBar;
    }
    let corner = p - offset;
    let near = |d: f32| d.abs() < SNAP;
    let across_x = map.x_range().expand(SNAP).contains(p.x);
    let across_y = map.y_range().expand(SNAP).contains(p.y);
    let edge = if across_y && near(p.x - map.left()) {
        Some((Side::Left, corner.y - map.top()))
    } else if across_y && near(p.x - map.right()) {
        Some((Side::Right, corner.y - map.top()))
    } else if across_x && near(p.y - map.top()) {
        Some((Side::Top, corner.x - map.left()))
    } else if across_x && near(p.y - map.bottom()) {
        Some((Side::Bottom, corner.x - map.left()))
    } else {
        None
    };
    match edge {
        Some((side, along)) => Place::Edge { side, along },
        None => Place::Float {
            at: [corner.x - map.left(), corner.y - map.top()],
        },
    }
}

fn follow_drag(app: &mut EpanetApp, ctx: &egui::Context, map: Rect) {
    let Some((bar, offset)) = app.bars.drag else {
        return;
    };
    let (down, pointer) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
    if let Some(p) = pointer {
        *app.prefs.toolbars.place_mut(bar) = drop_place(p, offset, map, app.bars.menubar);
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if !down {
        app.bars.drag = None;
    }
}

/// One toolbar: its grip, then its buttons (in reverse when `rtl`).
fn draw(app: &mut EpanetApp, ui: &mut Ui, bar: Bar, vertical: bool, rtl: bool) {
    let ctx = ui.ctx().clone();
    let frame = egui::Frame::NONE
        .fill(FILL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(egui::Margin::symmetric(3, 1))
        .show(ui, |ui| {
            let add = |ui: &mut Ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                let mut buttons = |ui: &mut Ui| {
                    let list = items(bar);
                    let order: Box<dyn Iterator<Item = &Item>> = if rtl {
                        Box::new(list.iter().rev())
                    } else {
                        Box::new(list.iter())
                    };
                    for item in order {
                        match *item {
                            Item::Button(icon, tip) => {
                                let active = match icon {
                                    Icon::Pointer => {
                                        app.tool == Tool::Select && app.group.is_none()
                                    }
                                    _ => tool_of(icon).is_some_and(|t| app.tool == t),
                                };
                                let enabled = match icon {
                                    Icon::Undo => app.can_undo(),
                                    Icon::Redo => app.can_redo(),
                                    _ => true,
                                };
                                let fg = Color32::from_gray(if enabled { 0x18 } else { 0xa0 });
                                if button(ui, active, enabled, tip, |p, c| {
                                    icons::paint(p, icon, c, 16.0, fg)
                                }) {
                                    run(app, &ctx, icon);
                                }
                            }
                            Item::Issue => {
                                let tip =
                                    "Report a problem or suggest a feature on WEPANET's GitHub";
                                if button(ui, false, true, tip, |p, c| {
                                    crate::menu::issue_icon(
                                        p,
                                        c + vec2(0.0, 1.0),
                                        0.72,
                                        crate::menu::ISSUE_RED,
                                        Some(crate::menu::ISSUE_RED),
                                    )
                                }) {
                                    crate::help::report_issue(&ctx);
                                }
                            }
                            Item::Separator => separator(ui, vertical),
                        }
                    }
                };
                if rtl {
                    buttons(ui);
                    grip(ui, vertical, BUTTON, BAR_TIP)
                } else {
                    let grabbed = grip(ui, vertical, BUTTON, BAR_TIP);
                    buttons(ui);
                    grabbed
                }
            };
            if vertical {
                ui.vertical(add).inner
            } else {
                ui.horizontal(add).inner
            }
        });
    let rect = frame.response.rect;
    app.bars.size[bar as usize] = rect.size();
    match frame.inner {
        Some(Grip::Drag(p)) => app.bars.drag = Some((bar, p - rect.min)),
        Some(Grip::Reset) => {
            app.bars.drag = None;
            *app.prefs.toolbars.place_mut(bar) = bar.home();
        }
        None => {}
    }
}

enum Grip {
    /// Dragging started with the pointer here.
    Drag(Pos2),
    /// Double-clicked: back to its first place.
    Reset,
}

/// The dotted handle a toolbar is moved by, `len` long across the toolbar.
fn grip(ui: &mut Ui, vertical: bool, len: f32, tip: &str) -> Option<Grip> {
    let size = if vertical {
        vec2(len, 8.0)
    } else {
        vec2(8.0, len)
    };
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text(tr(tip));
    for k in -2..=2 {
        for j in [-1.5_f32, 1.5] {
            let d = if vertical {
                vec2(k as f32 * 3.5, j)
            } else {
                vec2(j, k as f32 * 3.5)
            };
            ui.painter().circle_filled(rect.center() + d, 1.0, GRIP);
        }
    }
    if resp.double_clicked() {
        Some(Grip::Reset)
    } else if resp.drag_started() {
        resp.interact_pointer_pos().map(Grip::Drag)
    } else {
        None
    }
}

fn button(
    ui: &mut Ui,
    active: bool,
    enabled: bool,
    tip: &str,
    paint: impl FnOnce(&egui::Painter, Pos2),
) -> bool {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(BUTTON, BUTTON), sense);
    if active {
        ui.painter().rect(
            rect,
            CornerRadius::same(3),
            Color32::WHITE,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), Color32::WHITE);
    }
    paint(ui.painter(), rect.center());
    resp.on_hover_text(tr(tip)).clicked()
}

fn separator(ui: &mut Ui, vertical: bool) {
    if vertical {
        let (r, _) = ui.allocate_exact_size(vec2(BUTTON, 5.0), Sense::hover());
        ui.painter().hline(
            r.x_range().shrink(3.0),
            r.center().y,
            Stroke::new(1.0, BORDER),
        );
    } else {
        let (r, _) = ui.allocate_exact_size(vec2(5.0, BUTTON), Sense::hover());
        ui.painter().vline(
            r.center().x,
            r.y_range().shrink(3.0),
            Stroke::new(1.0, BORDER),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App as _;

    fn frame(app: &mut EpanetApp, ctx: &egui::Context, events: Vec<egui::Event>) {
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
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    fn drag(app: &mut EpanetApp, ctx: &egui::Context, from: Pos2, to: Pos2) {
        frame(app, ctx, vec![egui::Event::PointerMoved(from)]);
        frame(app, ctx, vec![button(from, true)]);
        for k in 1..=6 {
            let p = from + (to - from) * (k as f32 / 6.0);
            frame(app, ctx, vec![egui::Event::PointerMoved(p)]);
        }
        frame(app, ctx, vec![button(to, false)]);
        frame(app, ctx, vec![]);
    }

    /// A toolbar dropped on the menu bar goes there, one dropped near an
    /// edge of the map sticks to that edge, and anywhere else it floats.
    #[test]
    fn dropped_toolbars_stick_to_edges() {
        let map = Rect::from_min_max(pos2(200.0, 100.0), pos2(1200.0, 800.0));
        let menubar = Rect::from_min_max(pos2(0.0, 0.0), pos2(1400.0, 34.0));
        let off = vec2(5.0, 5.0);
        assert_eq!(
            drop_place(pos2(900.0, 20.0), off, map, menubar),
            Place::MenuBar
        );
        let edge = |p| match drop_place(p, off, map, menubar) {
            Place::Edge { side, .. } => Some(side),
            _ => None,
        };
        assert_eq!(edge(pos2(210.0, 400.0)), Some(Side::Left));
        assert_eq!(edge(pos2(1190.0, 400.0)), Some(Side::Right));
        assert_eq!(edge(pos2(600.0, 110.0)), Some(Side::Top));
        assert_eq!(edge(pos2(600.0, 790.0)), Some(Side::Bottom));
        assert_eq!(
            drop_place(pos2(600.0, 400.0), off, map, menubar),
            Place::Float { at: [395.0, 295.0] }
        );
    }

    /// The speed bar is dragged by its grip from the menu bar to the left
    /// edge of the map, where it stands upright, and a double-click on the
    /// grip puts it back. The Add toolbar's buttons pick the drawing tools.
    #[test]
    fn toolbars_move_and_add_objects() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let grip_of = |app: &EpanetApp, bar: Bar| {
            let r = ctx.memory(|m| m.area_rect(Id::new(("toolbar", bar))));
            match (app.prefs.toolbars.place(bar), r) {
                (Place::MenuBar, _) => {
                    // the grip is the bar's left end, which ends 6 px from the right
                    let size = app.bars.size[bar as usize];
                    let bar = &app.bars.menubar;
                    pos2(bar.right() - 6.0 - size.x + 7.0, bar.center().y)
                }
                (_, Some(r)) => r.min + vec2(7.0, 7.0),
                _ => panic!("no toolbar"),
            }
        };
        assert_eq!(app.prefs.toolbars.speed, Place::MenuBar);
        let map = app.export.map_rect.unwrap();
        let from = grip_of(&app, Bar::Speed);
        drag(&mut app, &ctx, from, pos2(map.left() + 5.0, map.center().y));
        assert!(
            matches!(
                app.prefs.toolbars.speed,
                Place::Edge {
                    side: Side::Left,
                    ..
                }
            ),
            "{:?}",
            app.prefs.toolbars.speed
        );
        frame(&mut app, &ctx, vec![]);
        let size = app.bars.size[Bar::Speed as usize];
        assert!(size.y > 4.0 * size.x, "upright: {size:?}");
        let r = ctx
            .memory(|m| m.area_rect(Id::new(("toolbar", Bar::Speed))))
            .unwrap();
        assert!((r.left() - map.left() - GAP).abs() < 1.0, "{r:?} {map:?}");

        // a double-click on the grip puts it back in the menu bar
        let g = grip_of(&app, Bar::Speed);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(g)]);
        for _ in 0..2 {
            frame(&mut app, &ctx, vec![button(g, true)]);
            frame(&mut app, &ctx, vec![button(g, false)]);
        }
        frame(&mut app, &ctx, vec![]);
        assert_eq!(app.prefs.toolbars.speed, Place::MenuBar);

        // the Add toolbar, at the top of the map: its Pipe button picks
        // the pipe tool, and a second click puts it down
        app.prefs.toolbars.show_add = true;
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![]);
        let r = ctx
            .memory(|m| m.area_rect(Id::new(("toolbar", Bar::Add))))
            .unwrap();
        assert!((r.top() - map.top() - GAP).abs() < 1.0, "{r:?} {map:?}");
        let click = |app: &mut EpanetApp, icon: Icon| {
            let p = button_at(r, Bar::Add, icon);
            frame(app, &ctx, vec![egui::Event::PointerMoved(p)]);
            frame(app, &ctx, vec![button(p, true)]);
            frame(app, &ctx, vec![button(p, false)]);
        };
        click(&mut app, Icon::Pipe);
        assert_eq!(app.tool, Tool::AddLink(LinkKind::Pipe));
        click(&mut app, Icon::Pipe);
        assert_eq!(app.tool, Tool::Select);

        // its Undo button undoes the last edit
        let links = app.net.links.len();
        app.checkpoint();
        app.net.delete_link(0);
        click(&mut app, Icon::Undo);
        assert_eq!(app.net.links.len(), links);
        assert!(!app.can_undo());

        // Select a Region and Select Object pick their tools
        click(&mut app, Icon::Region);
        assert_eq!(app.tool, Tool::Region);
        click(&mut app, Icon::Pointer);
        assert_eq!(app.tool, Tool::Select);
    }

    /// The middle of a toolbar's button, the toolbar at `r` laid out left
    /// to right: its margin, grip, then the items.
    fn button_at(r: Rect, bar: Bar, icon: Icon) -> Pos2 {
        let mut x = r.left() + 3.0 + 8.0 + 3.0;
        for item in items(bar) {
            match *item {
                Item::Button(i, _) if i == icon => {
                    return pos2(x + BUTTON / 2.0, r.center().y);
                }
                Item::Separator => x += 5.0 + 3.0,
                _ => x += BUTTON + 3.0,
            }
        }
        panic!("no {icon:?}")
    }

    /// The ribbon is dragged by its grip to the left edge of the window,
    /// where it stands upright beside the map, and a double-click on the
    /// grip puts it back at the top.
    #[test]
    fn the_ribbon_moves() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let top = app.export.map_rect.unwrap();
        let grip = pos2(10.0, app.bars.menubar.bottom() + RIBBON_HEIGHT / 2.0);
        drag(&mut app, &ctx, grip, pos2(20.0, 500.0));
        assert_eq!(app.prefs.toolbars.ribbon, Side::Left);
        frame(&mut app, &ctx, vec![]);
        let left = app.export.map_rect.unwrap();
        assert!(left.top() < top.top() - 30.0, "{top:?} -> {left:?}");
        assert!(
            left.left() > top.left() + RIBBON_WIDTH - 10.0,
            "{top:?} -> {left:?}"
        );

        // its grip is now across its top
        let g = pos2(RIBBON_WIDTH / 2.0, app.bars.menubar.bottom() + 6.0);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(g)]);
        for _ in 0..2 {
            frame(&mut app, &ctx, vec![button(g, true)]);
            frame(&mut app, &ctx, vec![button(g, false)]);
        }
        frame(&mut app, &ctx, vec![]);
        assert_eq!(app.prefs.toolbars.ribbon, Side::Top);
        assert_eq!(app.export.map_rect.unwrap(), top);
    }

    /// The ribbon's arrow hides it, a click on a menu tab shows it again,
    /// a double-click on a tab hides it, and so does Ctrl+F1 both ways.
    #[test]
    fn the_ribbon_collapses() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        let mut f = eframe::Frame::_new_kittest();
        let mut t = 0.0;
        // a frame a moment after the last, or `gap` seconds after it
        let mut run = |app: &mut EpanetApp, events: Vec<egui::Event>, gap: f64| {
            t += gap.max(0.02);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
                time: Some(t),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx(), &mut f);
                app.ui(ui, &mut f);
            });
            out.textures_delta.clear();
            out.shapes.into_iter().find_map(|c| match c.shape {
                egui::Shape::Text(t) if t.galley.text() == "Help" => Some(t.visual_bounding_rect()),
                _ => None,
            })
        };
        let mut help = None;
        for _ in 0..3 {
            help = run(&mut app, vec![], 0.0);
        }
        let open = app.export.map_rect.unwrap();
        let bar = app.bars.menubar;
        let mut click = |app: &mut EpanetApp, p: Pos2, gap: f64| {
            run(app, vec![egui::Event::PointerMoved(p)], gap);
            run(app, vec![button(p, true)], 0.0);
            run(app, vec![button(p, false)], 0.0);
            run(app, vec![], 0.0);
        };
        // the arrow, at the ribbon's bottom right
        let arrow = pos2(
            bar.right() - 6.0 - 10.0,
            bar.bottom() + RIBBON_HEIGHT - 12.0,
        );
        click(&mut app, arrow, 1.0);
        assert!(app.prefs.toolbars.ribbon_collapsed);
        let closed = app.export.map_rect.unwrap();
        assert!(
            (open.top() - closed.top() - RIBBON_HEIGHT).abs() < 1.0,
            "{open:?} {closed:?}"
        );

        // the arrow after the menu tabs shows it again
        // (a tab is 76 px wide, with its name in the middle)
        let help = help.expect("the Help tab");
        click(
            &mut app,
            pos2(help.center().x + 38.0 + 4.0 + 14.0, help.center().y),
            1.0,
        );
        assert!(!app.prefs.toolbars.ribbon_collapsed);
        assert_eq!(app.export.map_rect.unwrap(), open);
        click(&mut app, arrow, 1.0);
        assert!(app.prefs.toolbars.ribbon_collapsed);

        // and so does a click on the Project tab
        let tab = pos2(310.0, bar.center().y);
        click(&mut app, tab, 1.0);
        assert!(!app.prefs.toolbars.ribbon_collapsed);
        assert_eq!(app.export.map_rect.unwrap(), open);
        // a double-click on it hides it
        click(&mut app, tab, 1.0);
        click(&mut app, tab, 0.0);
        assert!(app.prefs.toolbars.ribbon_collapsed);
        // a double-click while hidden shows it, and it stays
        click(&mut app, tab, 1.0);
        click(&mut app, tab, 0.0);
        assert!(!app.prefs.toolbars.ribbon_collapsed);
        // Ctrl+F1 both ways
        let f1 = egui::Event::Key {
            key: egui::Key::F1,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        click(&mut app, pos2(700.0, 500.0), 1.0);
        run(&mut app, vec![f1.clone()], 1.0);
        assert!(app.prefs.toolbars.ribbon_collapsed);
        run(&mut app, vec![f1], 1.0);
        assert!(!app.prefs.toolbars.ribbon_collapsed);
    }

    /// A drop-down menu of the ribbon is a menu's width, however much room
    /// there is for it.
    #[test]
    fn ribbon_menus_are_narrow() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        app.prefs.menu_tab = crate::app::MenuTab::Project;
        let mut f = eframe::Frame::_new_kittest();
        let mut run = |app: &mut EpanetApp, events: Vec<egui::Event>| {
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
                .into_iter()
                .filter_map(|c| match c.shape {
                    egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let mut t = Vec::new();
        for _ in 0..3 {
            t = run(&mut app, vec![]);
        }
        let add = t.iter().find(|(s, _)| s == "Add").unwrap().1 + vec2(5.0, -20.0);
        run(&mut app, vec![egui::Event::PointerMoved(add)]);
        run(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: add,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        run(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: add,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        for _ in 0..3 {
            run(&mut app, vec![]);
        }
        let t = run(&mut app, vec![]);
        let j = t.iter().find(|(s, _)| s == "Junction Node").unwrap().1;
        let popup = ctx.layer_id_at(j + vec2(2.0, 2.0)).unwrap();
        let r = ctx.memory(|m| m.area_rect(popup.id)).unwrap();
        assert!(r.width() < 230.0, "{r:?}");
    }
}
