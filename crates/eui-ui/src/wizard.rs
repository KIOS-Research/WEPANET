//! The pieces shared by the import dialogs (Shapefile, DXF and CSV), drawn as
//! EPANET-UI's: page tabs, the yellow note, a file row, the property grid,
//! the preview and the Back / Next / Import / Cancel buttons.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::i18n::{tr, trf};
use crate::icons::{self, Icon};

/// Size of an importer's pages.
pub const PAGE: egui::Vec2 = vec2(600.0, 470.0);
pub const ROW_H: f32 = 26.0;

pub fn border(ui: &Ui) -> Stroke {
    Stroke::new(
        1.0,
        Color32::from_gray(if ui.visuals().dark_mode { 90 } else { 160 }),
    )
}

pub fn white(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        ui.visuals().extreme_bg_color
    } else {
        Color32::WHITE
    }
}

pub fn gray(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        ui.visuals().faint_bg_color
    } else {
        Color32::from_gray(0xf0)
    }
}

/// The page tabs, as the Windows tab control of EPANET-UI.
pub fn tabs(ui: &mut Ui, tab: &mut usize, names: &[&str]) {
    let font = FontId::proportional(14.5);
    let (row, _) = ui.allocate_exact_size(vec2(PAGE.x, 30.0), Sense::hover());
    let mut x = row.left();
    let mut selected_rect = None;
    for (k, name) in names.iter().enumerate() {
        let name = tr(name);
        let w = ui.fonts_mut(|f| {
            f.layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        }) + 36.0;
        let sel = *tab == k;
        let r = Rect::from_min_max(
            pos2(x, row.top() + if sel { 0.0 } else { 3.0 }),
            pos2(x + w, row.bottom() + if sel { 1.0 } else { 0.0 }),
        );
        let resp = ui.interact(r, ui.id().with(("import_tab", k)), Sense::click());
        let fill = if sel {
            white(ui)
        } else if resp.hovered() {
            Color32::from_rgb(0xe5, 0xf1, 0xfb)
        } else {
            gray(ui)
        };
        ui.painter().rect_filled(r, 0.0, fill);
        let b = border(ui);
        ui.painter().vline(r.left(), r.y_range(), b);
        ui.painter().vline(r.right(), r.y_range(), b);
        ui.painter().hline(r.x_range(), r.top(), b);
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            name,
            font.clone(),
            ui.visuals().text_color(),
        );
        if sel {
            selected_rect = Some(r);
        }
        if resp.clicked() {
            *tab = k;
        }
        x += w;
    }
    // the page's top border, broken under the selected tab
    let y = row.bottom();
    let b = border(ui);
    match selected_rect {
        Some(r) => {
            ui.painter().hline(row.left()..=r.left(), y, b);
            ui.painter().hline(r.right()..=row.right(), y, b);
        }
        None => {
            ui.painter().hline(row.x_range(), y, b);
        }
    }
}

/// The white page under the tabs; `add` lays out its content.
pub fn page(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let page = Rect::from_min_size(ui.cursor().min, PAGE);
    ui.allocate_rect(page, Sense::hover());
    ui.painter().rect_filled(page, 0.0, white(ui));
    let b = border(ui);
    ui.painter().vline(page.left(), page.y_range(), b);
    ui.painter().vline(page.right(), page.y_range(), b);
    ui.painter().hline(page.x_range(), page.bottom(), b);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(page.shrink2(vec2(1.0, 4.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(page);
    add(&mut child);
}

/// The light yellow note at the top of a page.
pub fn note(ui: &mut Ui, text: &str) {
    let fill = if ui.visuals().dark_mode {
        Color32::from_rgb(0x45, 0x42, 0x2a)
    } else {
        Color32::from_rgb(0xff, 0xff, 0xe1)
    };
    egui::Frame::NONE
        .fill(fill)
        .stroke(border(ui))
        .inner_margin(egui::Margin::symmetric(6, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr(text)).size(14.5));
        });
}

/// A square button with an icon (or a cross).
pub fn square_button(ui: &mut Ui, icon: Option<Icon>, tip: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 34.0), Sense::click());
    let w = if resp.hovered() {
        Color32::from_rgb(0xe5, 0xf1, 0xfb)
    } else {
        white(ui)
    };
    ui.painter().rect(
        r,
        3.0,
        w,
        Stroke::new(
            1.0,
            if resp.hovered() {
                Color32::from_rgb(0x00, 0x78, 0xd7)
            } else {
                Color32::from_gray(190)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let fg = ui.visuals().text_color();
    match icon {
        Some(i) => icons::paint(ui.painter(), i, r.center(), 24.0, fg),
        None => {
            let c = r.center();
            let d = 7.0;
            let st = Stroke::new(1.6, fg);
            ui.painter()
                .line_segment([c + vec2(-d, -d), c + vec2(d, d)], st);
            ui.painter()
                .line_segment([c + vec2(-d, d), c + vec2(d, -d)], st);
        }
    }
    resp.on_hover_text(tr(tip)).clicked()
}

/// A blue, underlined label that acts as a link.
pub fn link_label(ui: &mut Ui, text: &str, enabled: bool) -> bool {
    let color = if enabled {
        Color32::from_rgb(0x00, 0x66, 0xcc)
    } else {
        ui.visuals().weak_text_color()
    };
    let resp = ui.add(
        egui::Label::new(RichText::new(tr(text)).size(14.5).color(color).underline())
            .sense(Sense::click()),
    );
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    enabled && resp.clicked()
}

/// What was done on a [`file_row`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    None,
    Open,
    Clear,
}

/// "Links File:" with the chosen file's name, an open and a clear button.
pub fn file_row(ui: &mut Ui, label: &str, name: &str, open_tip: &str) -> FileAction {
    let mut action = FileAction::None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (lr, _) = ui.allocate_exact_size(vec2(170.0, 34.0), Sense::hover());
        ui.painter().text(
            lr.left_center() + vec2(0.0, 4.0),
            Align2::LEFT_CENTER,
            tr(label),
            FontId::proportional(14.5),
            ui.visuals().text_color(),
        );
        let w = ui.available_width() - 2.0 * 36.0 - 16.0;
        let (fr, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let fr = fr.shrink2(vec2(0.0, 4.0));
        ui.painter().rect_filled(fr, 0.0, white(ui));
        ui.painter().hline(
            fr.x_range(),
            fr.bottom(),
            Stroke::new(1.0, Color32::from_gray(120)),
        );
        ui.painter().text(
            fr.left_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(14.0),
            ui.visuals().text_color(),
        );
        if square_button(ui, Some(Icon::Open), open_tip) {
            action = FileAction::Open;
        }
        if square_button(ui, None, "Clear the file") {
            action = FileAction::Clear;
        }
    });
    action
}

/// The property grid of an import page: property, file column, units;
/// empty until a file is chosen.
pub struct Mapping<'a> {
    pub id: &'a str,
    /// Heading of the first column ("Link Property").
    pub title: &'a str,
    /// Heading of the second column ("Shapefile Attribute").
    pub column: &'a str,
    pub props: &'a [&'a str],
    /// The file's field names, once a file is chosen.
    pub fields: Option<&'a [String]>,
    pub map: &'a mut [Option<usize>],
    pub units: &'a mut [usize],
    /// Unit choices of each property ("" = as in the project).
    pub choices: &'a dyn Fn(usize) -> &'static [&'static str],
    pub height: f32,
}

pub fn mapping(ui: &mut Ui, m: Mapping) {
    let width = ui.available_width();
    let cols = [width * 0.34, width * 0.33, width * 0.33];
    let rows = m.props.len() + 1;
    let height = m.height.max(ROW_H * rows as f32 + 1.0);
    let (outer, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(outer, 0.0, white(ui));
    let b = border(ui);
    let text = ui.visuals().text_color();
    let font = FontId::proportional(14.5);
    let xs = [
        outer.left(),
        outer.left() + cols[0],
        outer.left() + cols[0] + cols[1],
        outer.right(),
    ];
    let bottom = outer.top() + ROW_H * rows as f32;
    // fixed header row and first column
    p.rect_filled(
        Rect::from_min_max(outer.min, pos2(outer.right(), outer.top() + ROW_H)),
        0.0,
        gray(ui),
    );
    p.rect_filled(
        Rect::from_min_max(outer.min, pos2(xs[1], bottom)),
        0.0,
        gray(ui),
    );
    for r in 1..=rows {
        p.hline(xs[0]..=xs[3], outer.top() + ROW_H * r as f32, b);
    }
    for x in &xs[1..3] {
        p.vline(*x, outer.top()..=bottom, b);
    }
    for (k, h) in [m.title, m.column, "Units"].iter().enumerate() {
        let r = Rect::from_x_y_ranges(xs[k]..=xs[k + 1], outer.top()..=outer.top() + ROW_H);
        if k == 0 {
            p.text(
                r.left_center() + vec2(6.0, 0.0),
                Align2::LEFT_CENTER,
                tr(h),
                font.clone(),
                text,
            );
        } else {
            p.text(r.center(), Align2::CENTER_CENTER, tr(h), font.clone(), text);
        }
    }
    for (k, prop) in m.props.iter().enumerate() {
        let y = outer.top() + ROW_H * (k + 1) as f32;
        let cell = |c: usize| {
            Rect::from_x_y_ranges(xs[c] + 1.0..=xs[c + 1] - 1.0, y + 1.0..=y + ROW_H - 1.0)
        };
        p.text(
            cell(0).left_center() + vec2(5.0, 0.0),
            Align2::LEFT_CENTER,
            tr(prop),
            font.clone(),
            text,
        );
        let Some(fields) = m.fields else { continue };
        let current = m.map[k]
            .and_then(|f| fields.get(f))
            .cloned()
            .unwrap_or_default();
        let r1 = cell(1);
        ui.scope_builder(egui::UiBuilder::new().max_rect(r1), |ui| {
            egui::ComboBox::from_id_salt((m.id, k))
                .selected_text(current)
                .width(r1.width() - 2.0)
                .height(300.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut m.map[k], None, "");
                    for (f, name) in fields.iter().enumerate() {
                        ui.selectable_value(&mut m.map[k], Some(f), name);
                    }
                });
        });
        let choices = (m.choices)(k);
        if !choices.is_empty() {
            let r2 = cell(2);
            ui.scope_builder(egui::UiBuilder::new().max_rect(r2), |ui| {
                egui::ComboBox::from_id_salt((m.id, "u", k))
                    .selected_text(choices[m.units[k].min(choices.len() - 1)])
                    .width(r2.width() - 2.0)
                    .show_ui(ui, |ui| {
                        for (c, name) in choices.iter().enumerate() {
                            ui.selectable_value(&mut m.units[k], c, *name);
                        }
                    });
            });
        }
    }
    p.rect_stroke(outer, 0.0, b, egui::StrokeKind::Inside);
}

/// A file's records under its field names (EPANET-UI's "View Attribute
/// Table" and "View CSV File").
pub fn records(ui: &mut Ui, id: &str, fields: &[String], records: &[Vec<String>], height: f32) {
    let mut rows = vec![fields.to_vec()];
    rows.extend(records.iter().take(1000).cloned());
    let mut sort = None;
    egui::ScrollArea::both()
        .id_salt(id)
        .max_height(height)
        .auto_shrink(false)
        .show(ui, |ui| {
            crate::reports::table(
                ui,
                id,
                &rows,
                &mut sort,
                &trf("{} records", &[&records.len()]),
            );
        });
}

/// A drawing of the lines and points to import.
pub fn preview(ui: &mut Ui, lines: &[&[[f64; 2]]], points: &[[f64; 2]], empty: &str, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect(
        rect,
        0.0,
        Color32::WHITE,
        border(ui),
        egui::StrokeKind::Inside,
    );
    let all = || lines.iter().flat_map(|l| l.iter()).chain(points.iter());
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for [x, y] in all() {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    if x0 > x1 {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            tr(empty),
            FontId::proportional(14.0),
            Color32::from_gray(120),
        );
        return;
    }
    let inner = rect.shrink(12.0);
    let sc = (inner.width() as f64 / (x1 - x0).max(1e-9))
        .min(inner.height() as f64 / (y1 - y0).max(1e-9));
    let c = inner.center();
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let at = |[x, y]: [f64; 2]| pos2(c.x + ((x - cx) * sc) as f32, c.y - ((y - cy) * sc) as f32);
    for l in lines {
        p.add(egui::Shape::line(
            l.iter().map(|&q| at(q)).collect(),
            Stroke::new(1.0, Color32::from_gray(60)),
        ));
    }
    for q in points {
        p.circle_filled(at(*q), 2.0, Color32::from_rgb(0x30, 0x60, 0xc0));
    }
}

/// EPANET-UI's picture of two line ends merged within the snap tolerance.
pub fn snap_picture(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(240.0, 54.0), Sense::hover());
    let p = ui.painter();
    let y = r.center().y;
    let x = |f: f32| r.left() + 12.0 + f;
    let black = Stroke::new(2.0, Color32::BLACK);
    p.hline(x(0.0)..=x(80.0), y, black);
    p.hline(x(120.0)..=x(200.0), y, black);
    // dashed ellipse around the two ends
    let (cx, rx, ry) = (x(100.0), 40.0, 22.0);
    let n = 48;
    for k in (0..n).step_by(2) {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        p.line_segment(
            [
                pos2(cx + rx * a0.cos(), y + ry * a0.sin()),
                pos2(cx + rx * a1.cos(), y + ry * a1.sin()),
            ],
            Stroke::new(1.0, Color32::from_gray(60)),
        );
    }
    let dot = |cx: f32, c: Color32| {
        p.circle(pos2(cx, y), 8.0, c, Stroke::new(1.0, Color32::BLACK));
    };
    dot(x(0.0), Color32::from_rgb(0xff, 0x00, 0xff));
    dot(x(80.0), Color32::from_rgb(0x00, 0xff, 0x00));
    dot(x(120.0), Color32::from_rgb(0x00, 0xff, 0x00));
    dot(x(200.0), Color32::from_rgb(0x00, 0xff, 0xff));
}

/// An options page row: a label and its control, or (`label` empty) a
/// control on its own, centered as EPANET-UI's options pages.
pub fn option_row(ui: &mut Ui, label: &str, h: f32, add: impl FnOnce(&mut Ui)) {
    let left = (ui.available_width() - 370.0).max(0.0) / 2.0;
    ui.horizontal(|ui| {
        ui.add_space(left);
        if !label.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(200.0, h), Sense::hover());
            ui.painter().text(
                r.left_center(),
                Align2::LEFT_CENTER,
                tr(label),
                FontId::proportional(14.5),
                ui.visuals().text_color(),
            );
        }
        add(ui);
    });
}

/// What the wizard's buttons asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    None,
    Import,
    Cancel,
}

/// Back / Next (Import on the last page) / Cancel, at the bottom right.
pub fn buttons(ui: &mut Ui, tab: &mut usize, last: usize, can_import: bool) -> Nav {
    let mut nav = Nav::None;
    ui.horizontal(|ui| {
        let (w, gap) = (100.0, 18.0);
        ui.set_width(PAGE.x);
        ui.add_space((PAGE.x - 3.0 * w - 2.0 * gap - 4.0).max(0.0));
        ui.spacing_mut().item_spacing.x = gap;
        if ui
            .add_enabled(
                *tab > 0,
                egui::Button::new(tr("Back")).min_size(vec2(w, 32.0)),
            )
            .clicked()
        {
            *tab -= 1;
        }
        if *tab == last {
            if ui
                .add_enabled(
                    can_import,
                    egui::Button::new(tr("Import")).min_size(vec2(w, 32.0)),
                )
                .clicked()
            {
                nav = Nav::Import;
            }
        } else if ui
            .add(egui::Button::new(tr("Next")).min_size(vec2(w, 32.0)))
            .clicked()
        {
            *tab += 1;
        }
        if ui
            .add(egui::Button::new(tr("Cancel")).min_size(vec2(w, 32.0)))
            .clicked()
        {
            nav = Nav::Cancel;
        }
    });
    nav
}

/// The error line under the page.
pub fn error(ui: &mut Ui, e: &Option<String>) {
    if let Some(e) = e {
        ui.add_space(6.0);
        ui.label(RichText::new(tr(e)).color(Color32::from_rgb(0xc0, 0x39, 0x2b)));
    }
}
