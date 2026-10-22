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
    // the arrow keys move a slider that was clicked, a step at a time
    if enabled && (resp.clicked() || resp.drag_started()) {
        resp.request_focus();
    }
    if enabled && resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
        let v = match slider_keys(ui) {
            SliderMove::None => *value,
            SliderMove::Start => lo,
            SliderMove::End => hi,
            SliderMove::Steps(k) => match step {
                Some(s) => *value + k as f64 * s,
                None => from_t((to_t(*value) + k as f64 * 0.02).clamp(0.0, 1.0)),
            }
            .clamp(lo, hi),
        };
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
    if enabled && resp.has_focus() {
        p.rect_stroke(
            thumb.expand(2.0),
            3.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Outside,
        );
    }
    resp
}

/// How the keys pressed this frame move a slider.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SliderMove {
    None,
    Start,
    End,
    Steps(i32),
}

/// Right/Up a step forward, Left/Down a step back, Home and End to the ends.
pub fn slider_keys(ui: &Ui) -> SliderMove {
    ui.input(|i| {
        if i.key_pressed(egui::Key::Home) {
            return SliderMove::Start;
        }
        if i.key_pressed(egui::Key::End) {
            return SliderMove::End;
        }
        let n = |k| i.num_presses(k) as i32;
        match n(egui::Key::ArrowRight) + n(egui::Key::ArrowUp)
            - n(egui::Key::ArrowLeft)
            - n(egui::Key::ArrowDown)
        {
            0 => SliderMove::None,
            k => SliderMove::Steps(k),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clicked slider takes the keyboard: the arrow keys move it a step
    /// at a time, Home and End to its ends.
    #[test]
    fn the_arrow_keys_move_a_clicked_slider() {
        let ctx = egui::Context::default();
        let mut v = 3.0;
        let frame = |events: Vec<egui::Event>, v: &mut f64| {
            let mut rect = egui::Rect::NOTHING;
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                rect = slider(ui, true, v, 0.0..=10.0, false, Some(1.0)).rect;
            });
            out.textures_delta.clear();
            rect
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        let click = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        // not yet clicked: the keys do nothing
        let rect = frame(vec![key(egui::Key::ArrowRight)], &mut v);
        assert_eq!(v, 3.0);
        // click on the thumb
        let thumb = egui::pos2(
            rect.left() + 4.5 + (rect.width() - 9.0) * 0.3,
            rect.center().y,
        );
        frame(vec![egui::Event::PointerMoved(thumb)], &mut v);
        frame(vec![click(thumb, true)], &mut v);
        frame(vec![click(thumb, false)], &mut v);
        assert_eq!(v, 3.0);
        frame(vec![key(egui::Key::ArrowRight)], &mut v);
        frame(vec![key(egui::Key::ArrowRight)], &mut v);
        assert_eq!(v, 5.0);
        frame(vec![key(egui::Key::ArrowLeft)], &mut v);
        assert_eq!(v, 4.0);
        frame(vec![key(egui::Key::End)], &mut v);
        assert_eq!(v, 10.0);
        frame(vec![key(egui::Key::ArrowUp)], &mut v);
        assert_eq!(v, 10.0);
        frame(vec![key(egui::Key::Home)], &mut v);
        assert_eq!(v, 0.0);
    }
}
