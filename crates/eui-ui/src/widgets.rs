//! Check boxes and radio buttons drawn as on Windows 11, as EPANET-UI shows
//! them: a blue box with a white check mark, a blue ring with a white dot.

use egui::{Color32, Response, Sense, Stroke, Ui, Widget, WidgetText, pos2, vec2};

/// Windows 11 accent blue.
pub const ACCENT: Color32 = Color32::from_rgb(0x00, 0x5f, 0xb8);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0x19, 0x6e, 0xbf);

/// Size of the box or circle.
const MARK: f32 = 16.0;
const GAP: f32 = 7.0;

fn layout(
    ui: &mut Ui,
    text: WidgetText,
    sense: Sense,
) -> (Response, egui::Rect, Option<std::sync::Arc<egui::Galley>>) {
    let galley = (!text.is_empty()).then(|| {
        text.into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        )
    });
    let text_w = galley.as_ref().map_or(0.0, |g| g.size().x + GAP);
    let h = ui.spacing().interact_size.y.max(MARK + 4.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(MARK + text_w, h), sense);
    let mark = egui::Rect::from_center_size(
        pos2(rect.left() + MARK / 2.0, rect.center().y),
        vec2(MARK, MARK),
    );
    (resp, mark, galley)
}

fn text_color(ui: &Ui, enabled: bool) -> Color32 {
    if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color().gamma_multiply(0.7)
    }
}

/// A Windows-style check box.
pub struct Checkbox<'a> {
    checked: &'a mut bool,
    text: WidgetText,
}

impl<'a> Checkbox<'a> {
    pub fn new(checked: &'a mut bool, text: impl Into<WidgetText>) -> Self {
        Checkbox {
            checked,
            text: crate::i18n::tr_widget(text.into()),
        }
    }
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let enabled = ui.is_enabled();
        let (mut resp, mark, galley) = layout(ui, self.text, Sense::click());
        if resp.clicked() {
            *self.checked = !*self.checked;
            resp.mark_changed();
        }
        let p = ui.painter();
        let dark = ui.visuals().dark_mode;
        if *self.checked {
            let fill = if !enabled {
                Color32::from_gray(160)
            } else if resp.hovered() {
                ACCENT_HOVER
            } else {
                ACCENT
            };
            p.rect_filled(mark, 3.0, fill);
            let s = Stroke::new(1.6, Color32::WHITE);
            let c = mark.center();
            p.line_segment([c + vec2(-4.0, 0.2), c + vec2(-1.2, 3.0)], s);
            p.line_segment([c + vec2(-1.2, 3.0), c + vec2(4.2, -3.2)], s);
        } else {
            let fill = if dark {
                ui.visuals().extreme_bg_color
            } else if resp.hovered() && enabled {
                Color32::from_gray(0xee)
            } else {
                Color32::from_gray(0xf8)
            };
            let border = if enabled {
                Color32::from_gray(0x8a)
            } else {
                Color32::from_gray(0xc0)
            };
            p.rect(
                mark,
                3.0,
                fill,
                Stroke::new(1.0, border),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(g) = galley {
            let pos = pos2(mark.right() + GAP, resp.rect.center().y - g.size().y / 2.0);
            ui.painter().galley(pos, g, text_color(ui, enabled));
        }
        resp
    }
}

/// A Windows-style radio button.
pub struct RadioButton {
    selected: bool,
    text: WidgetText,
}

impl RadioButton {
    pub fn new(selected: bool, text: impl Into<WidgetText>) -> Self {
        RadioButton {
            selected,
            text: crate::i18n::tr_widget(text.into()),
        }
    }
}

impl Widget for RadioButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let enabled = ui.is_enabled();
        let (resp, mark, galley) = layout(ui, self.text, Sense::click());
        let p = ui.painter();
        let c = mark.center();
        let r = MARK / 2.0;
        if self.selected {
            let fill = if !enabled {
                Color32::from_gray(160)
            } else if resp.hovered() {
                ACCENT_HOVER
            } else {
                ACCENT
            };
            p.circle_filled(c, r, fill);
            p.circle_filled(c, r * 0.42, Color32::WHITE);
        } else {
            let fill = if ui.visuals().dark_mode {
                ui.visuals().extreme_bg_color
            } else {
                Color32::from_gray(0xf8)
            };
            let border = if enabled {
                Color32::from_gray(0x8a)
            } else {
                Color32::from_gray(0xc0)
            };
            p.circle(c, r - 0.5, fill, Stroke::new(1.0, border));
        }
        if let Some(g) = galley {
            let pos = pos2(mark.right() + GAP, resp.rect.center().y - g.size().y / 2.0);
            ui.painter().galley(pos, g, text_color(ui, enabled));
        }
        resp
    }
}

pub fn checkbox(ui: &mut Ui, checked: &mut bool, text: impl Into<WidgetText>) -> Response {
    ui.add(Checkbox::new(checked, text))
}

pub fn radio(ui: &mut Ui, selected: bool, text: impl Into<WidgetText>) -> Response {
    ui.add(RadioButton::new(selected, text))
}

pub fn radio_value<T: PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    text: impl Into<WidgetText>,
) -> Response {
    let mut resp = radio(ui, *current == value, text);
    if resp.clicked() && *current != value {
        *current = value;
        resp.mark_changed();
    }
    resp
}

/// A slider as Windows 11 draws it: a thin gray rail and a blue handle.
/// `log` spaces the values logarithmically; `step` rounds them.
pub fn slider(
    ui: &mut Ui,
    enabled: bool,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    log: bool,
    step: Option<f64>,
) -> Response {
    let (lo, hi) = (*range.start(), *range.end());
    let to_t = |v: f64| {
        let t = if log {
            (v.ln() - lo.ln()) / (hi.ln() - lo.ln())
        } else {
            (v - lo) / (hi - lo)
        };
        t.clamp(0.0, 1.0)
    };
    let from_t = |t: f64| {
        if log {
            (lo.ln() + t * (hi.ln() - lo.ln())).exp()
        } else {
            lo + t * (hi - lo)
        }
    };
    let w = ui.available_width().max(40.0);
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, 24.0), sense);
    let (thumb_w, thumb_h) = (9.0, 20.0);
    let rail = egui::Rect::from_min_max(
        pos2(rect.left() + thumb_w / 2.0, rect.center().y - 2.0),
        pos2(rect.right() - thumb_w / 2.0, rect.center().y + 2.0),
    );
    if enabled
        && (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let t = ((p.x - rail.left()) / rail.width()).clamp(0.0, 1.0) as f64;
        let mut v = from_t(t);
        if let Some(s) = step {
            v = (v / s).round() * s;
        }
        let v = v.clamp(lo, hi);
        if v != *value {
            *value = v;
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    p.rect_filled(
        rail,
        2.0,
        Color32::from_gray(if ui.visuals().dark_mode { 90 } else { 0xd6 }),
    );
    let x = rail.left() + rail.width() * to_t(*value) as f32;
    let thumb = egui::Rect::from_center_size(pos2(x, rect.center().y), vec2(thumb_w, thumb_h));
    let fill = if !enabled {
        Color32::from_gray(160)
    } else if resp.hovered() || resp.dragged() {
        ACCENT_HOVER
    } else {
        ACCENT
    };
    p.rect_filled(thumb, 2.0, fill);
    resp
}
