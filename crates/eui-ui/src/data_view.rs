//! The Project Data view (Project ▸ Details), as in EPANET-UI's
//! `projectviewer`: the INP sections listed on the left, the selected
//! section shown as a table on the right.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};
use eui_core::DataTable;

use crate::app::EpanetApp;

/// Tables of the project, rebuilt when the network changes.
#[derive(Default)]
pub struct DataViewState {
    pub open: bool,
    pub section: usize,
    cache: Option<(u64, Vec<DataTable>)>,
}

const ROW_H: f32 = 26.0;

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    let generation = app.generation;
    if app
        .data_view
        .cache
        .as_ref()
        .is_none_or(|(g, _)| *g != generation)
    {
        app.data_view.cache = Some((generation, app.net.data_tables()));
    }
    let pal = app.palette();
    let v = ui.visuals().clone();
    let white = if v.dark_mode {
        v.extreme_bg_color
    } else {
        Color32::WHITE
    };
    let grid = Stroke::new(1.0, Color32::from_gray(if v.dark_mode { 70 } else { 205 }));
    let font = FontId::monospace(13.0);
    let mut close = false;

    // Title bar.
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    ui.painter().rect_filled(bar, 0.0, pal.form);
    ui.painter().text(
        bar.left_center() + vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        "Project Data",
        FontId::proportional(14.0),
        v.text_color(),
    );
    let x = Rect::from_center_size(bar.right_center() - vec2(18.0, 0.0), vec2(24.0, 24.0));
    let xr = ui.interact(x, ui.id().with("data_close"), Sense::click());
    if xr.hovered() {
        ui.painter()
            .rect_filled(x, 3.0, v.widgets.hovered.weak_bg_fill);
    }
    ui.painter().text(
        x.center(),
        Align2::CENTER_CENTER,
        "×",
        FontId::proportional(18.0),
        v.text_color(),
    );
    if xr.on_hover_text("Close the Project Data view").clicked() {
        close = true;
    }

    let Some((_, tables)) = &app.data_view.cache else {
        return;
    };
    let sel = app.data_view.section.min(tables.len().saturating_sub(1));
    let mut new_sel = sel;
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, Sense::hover());
    ui.painter().rect_filled(rect, 0.0, white);

    // Category list.
    let list_w = 180.0;
    let list = Rect::from_min_size(rect.min, vec2(list_w, rect.height()));
    let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
    let (head, _) = list_ui.allocate_exact_size(vec2(list_w, 34.0), Sense::hover());
    list_ui
        .painter()
        .rect(head, 0.0, pal.form, grid, egui::StrokeKind::Inside);
    list_ui.painter().text(
        head.center(),
        Align2::CENTER_CENTER,
        "Category",
        FontId::proportional(13.5),
        v.text_color(),
    );
    egui::ScrollArea::vertical()
        .id_salt("data_sections")
        .auto_shrink(false)
        .show(&mut list_ui, |ui| {
            for (k, t) in tables.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(list_w, ROW_H), Sense::click());
                let (bg, fg) = if k == sel {
                    (Color32::from_rgb(0x00, 0x78, 0xd7), Color32::WHITE)
                } else if resp.hovered() {
                    (v.widgets.hovered.weak_bg_fill, v.text_color())
                } else {
                    (white, v.text_color())
                };
                ui.painter().rect_filled(r, 0.0, bg);
                ui.painter().text(
                    r.left_center() + vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("[{}]", t.name),
                    font.clone(),
                    fg,
                );
                if resp.clicked() {
                    new_sel = k;
                }
            }
        });
    ui.painter().vline(
        list.right(),
        list.y_range(),
        Stroke::new(1.5, Color32::from_gray(170)),
    );

    // Selected section.
    let body = Rect::from_min_max(pos2(list.right() + 2.0, rect.top()), rect.max);
    let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
    if let Some(t) = tables.get(sel) {
        section_table(&mut body_ui, t, &font, white, grid, pal.form);
    }
    app.data_view.section = new_sel;
    if close {
        app.data_view.open = false;
    }
}

fn section_table(
    ui: &mut Ui,
    t: &DataTable,
    font: &FontId,
    white: Color32,
    grid: Stroke,
    head_bg: Color32,
) {
    let ncols = t
        .rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(t.columns.len())
        .max(1);
    // Column widths from the longest text in each column.
    let char_w = ui.fonts_mut(|f| f.glyph_width(font, '0'));
    let mut widths = vec![60.0f32; ncols];
    let measure = |s: &str| s.chars().count() as f32 * char_w + 18.0;
    for (c, w) in widths.iter_mut().enumerate() {
        if let Some(h) = t.columns.get(c) {
            *w = w.max(measure(h));
        }
        for r in t.rows.iter().take(2000) {
            if let Some(s) = r.get(c) {
                *w = w
                    .max(measure(s))
                    .min(if t.name == "TITLE" { 2000.0 } else { 360.0 });
            }
        }
    }
    let avail = ui.available_width();
    let total: f32 = widths.iter().sum();
    if total < avail {
        // Spread the spare width so the grid fills the page.
        let extra = (avail - total) / ncols as f32;
        for w in &mut widths {
            *w += extra;
        }
    }
    let total: f32 = widths.iter().sum();
    let text = ui.visuals().text_color();
    let stripe = if ui.visuals().dark_mode {
        ui.visuals().faint_bg_color
    } else {
        Color32::from_rgb(0xf2, 0xf5, 0xf8)
    };
    egui::ScrollArea::horizontal()
        .id_salt(("data_h", &t.name))
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.set_width(total);
            // Heading row (empty for sections without column names, as in EPANET-UI).
            let (hr, _) = ui.allocate_exact_size(vec2(total, 34.0), Sense::hover());
            ui.painter().rect_filled(hr, 0.0, head_bg);
            let mut x = hr.left();
            for (c, w) in widths.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(x, hr.top()), vec2(*w, hr.height()));
                ui.painter()
                    .rect_stroke(cell, 0.0, grid, egui::StrokeKind::Inside);
                if let Some(h) = t.columns.get(c) {
                    ui.painter().text(
                        cell.left_center() + vec2(6.0, 0.0),
                        Align2::LEFT_CENTER,
                        h,
                        font.clone(),
                        text,
                    );
                }
                x += w;
            }
            egui::ScrollArea::vertical()
                .id_salt(("data_v", &t.name))
                .auto_shrink(false)
                .show_rows(ui, ROW_H, t.rows.len(), |ui, range| {
                    for k in range {
                        let (rr, _) = ui.allocate_exact_size(vec2(total, ROW_H), Sense::hover());
                        ui.painter()
                            .rect_filled(rr, 0.0, if k % 2 == 1 { stripe } else { white });
                        ui.painter().hline(rr.x_range(), rr.bottom(), grid);
                        let mut x = rr.left();
                        for (c, w) in widths.iter().enumerate() {
                            if let Some(s) = t.rows[k].get(c) {
                                ui.painter().text(
                                    pos2(x + 6.0, rr.center().y),
                                    Align2::LEFT_CENTER,
                                    s,
                                    font.clone(),
                                    text,
                                );
                            }
                            x += w;
                            if c + 1 < ncols && !t.columns.is_empty() {
                                ui.painter().vline(x, rr.y_range(), grid);
                            }
                        }
                    }
                });
        });
}
