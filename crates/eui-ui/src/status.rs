//! Status bar, as in EPANET-UI's `statusframe.lfm`: fixed-width cells for
//! Auto-Length | Flow | Pressure | Head Loss | Demands | Quality | results | X,Y.

use egui::{Align2, Color32, FontId, Sense, Stroke, Ui, vec2};

use crate::app::EpanetApp;
use crate::i18n::tr;
use crate::style;

/// One fixed-width cell with centered text and a divider on its right.
fn cell(ui: &mut Ui, width: f32, text: &str, fill: Option<Color32>) -> egui::Response {
    let h = ui.available_height();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let v = ui.visuals();
    let mut fg = v.text_color();
    if let Some(f) = fill {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(0.0, 1.0)), 0.0, f);
        fg = Color32::BLACK;
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        tr(text),
        FontId::proportional(13.0),
        fg,
    );
    ui.painter().vline(
        rect.right(),
        rect.y_range(),
        Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color),
    );
    resp
}

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    egui::Panel::bottom("status")
        .exact_size(28.0)
        .frame(
            egui::Frame::NONE
                .fill(ui.visuals().panel_fill)
                .inner_margin(egui::Margin::symmetric(6, 0)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal_centered(|ui| {
                ui.allocate_ui(vec2(120.0, ui.available_height()), |ui| {
                    crate::widgets::checkbox(ui, &mut app.prefs.auto_length, "Auto-Length")
                        .on_hover_text("Compute pipe lengths from map coordinates when nodes move");
                });
                let (r, _) =
                    ui.allocate_exact_size(vec2(1.0, ui.available_height()), Sense::hover());
                ui.painter().vline(
                    r.left(),
                    r.y_range(),
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                );

                cell(
                    ui,
                    112.0,
                    &format!("{}: {}", tr("Flow"), app.net.flow_units().to_lowercase()),
                    None,
                );
                cell(
                    ui,
                    128.0,
                    &format!("{}: {}", tr("Pressure"), app.pressure_units()),
                    None,
                );
                cell(
                    ui,
                    140.0,
                    &format!(
                        "{}: {}",
                        tr("Head Loss"),
                        app.net
                            .option("OPTIONS", "HEADLOSS")
                            .unwrap_or_else(|| "H-W".into())
                    ),
                    None,
                );
                cell(
                    ui,
                    140.0,
                    &format!(
                        "{}: {}",
                        tr("Demands"),
                        app.net
                            .option("OPTIONS", "DEMAND MODEL")
                            .unwrap_or_else(|| "DDA".into())
                    ),
                    None,
                );
                let (model, ..) = eui_core::props::quality_parts(&app.net);
                let q = match model.as_str() {
                    "Trace" => "Source Trace".to_string(),
                    m => m.to_string(),
                };
                // EPANET-MSX replaces the single species quality when it runs
                let msx = app.net.msx.as_ref().filter(|_| app.msx.enabled);
                let resp = match msx {
                    Some(_) => cell(
                        ui,
                        190.0,
                        &format!("{}: EPANET-MSX", tr("Quality")),
                        Some(Color32::from_rgb(0xd4, 0xed, 0xda)),
                    )
                    .on_hover_text(tr("EPANET-MSX runs with each analysis — click to edit")),
                    None => cell(ui, 190.0, &format!("{}: {}", tr("Quality"), tr(&q)), None),
                };
                if msx.is_some() && resp.interact(Sense::click()).clicked() {
                    crate::msx::open_editor(app);
                }
                let (text, fill) = if app.is_running() {
                    ("Analyzing…", Some(Color32::from_gray(205)))
                } else if app.run_error.is_some() {
                    ("Analysis Failed", Some(Color32::from_rgb(0xff, 0x8a, 0x80)))
                } else if app.results_stale() {
                    ("Results Need Updating", Some(style::RESULTS_STALE))
                } else if app.results.is_some() {
                    ("Results are Current", Some(style::RESULTS_CURRENT))
                } else {
                    ("No Results", None)
                };
                cell(ui, 210.0, text, fill);
                let xy = match app.cursor {
                    Some([x, y]) => format!("  X,Y: {x:.3}, {y:.3}"),
                    None => "  X,Y:".into(),
                };
                ui.label(xy);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&app.status).weak());
                });
            });
        });
}
