//! The Calibration Report (EPANET-UI's `calibrationrpt`): measured data
//! entered or loaded by the user, compared with simulated results in a time
//! series plot, a correlation plot and a table of error statistics.

use egui::{Color32, FontId, RichText, Stroke, Ui};
use egui_plot::{Line, MarkerShape, Plot, PlotPoints, Points};
use eui_core::analysis::{self, ErrorSums, Measurement};
use eui_core::{LinkVar, NodeVar, SimResults};

use crate::app::EpanetApp;
use crate::io::Inbox;

/// Parameters that can be calibrated: (label, node variable or link variable).
const PARAMS: [(&str, Option<NodeVar>, Option<LinkVar>); 5] = [
    ("Head", Some(NodeVar::Head), None),
    ("Pressure", Some(NodeVar::Pressure), None),
    ("Flow", None, Some(LinkVar::Flow)),
    ("Velocity", None, Some(LinkVar::Velocity)),
    ("Quality", Some(NodeVar::Quality), None),
];

const MARKERS: [Color32; 6] = [
    Color32::from_rgb(0xed, 0x1c, 0x24),
    Color32::from_rgb(0x00, 0x72, 0xbd),
    Color32::from_rgb(0x22, 0xb1, 0x4c),
    Color32::from_rgb(0xff, 0x7f, 0x27),
    Color32::from_rgb(0xa3, 0x49, 0xa4),
    Color32::from_rgb(0x00, 0xa2, 0xe8),
];

pub struct CalibState {
    /// Rows of the data grid: location, time, value.
    pub rows: Vec<[String; 3]>,
    pub param: usize,
    pub tab: u8,
    pub location: usize,
    inbox: Inbox,
}

impl Default for CalibState {
    fn default() -> Self {
        CalibState {
            rows: vec![Default::default()],
            param: 1,
            tab: 0,
            location: 0,
            inbox: Inbox::default(),
        }
    }
}

fn param_name(app: &EpanetApp, k: usize) -> String {
    match (k, &app.results) {
        (4, Some(r)) => crate::reports::quality_label(r),
        _ => PARAMS[k].0.to_string(),
    }
}

fn is_node(k: usize) -> bool {
    PARAMS[k].1.is_some()
}

/// Measurements read from the grid (a blank location repeats the one above).
fn measurements(app: &EpanetApp) -> Vec<Measurement> {
    let mut text = String::new();
    let mut loc = String::new();
    for r in &app.rpt.calib.rows {
        if !r[0].trim().is_empty() {
            loc = r[0].trim().to_string();
        }
        if r[1].trim().is_empty() || r[2].trim().is_empty() || loc.is_empty() {
            continue;
        }
        text.push_str(&format!("{loc} {} {}\n", r[1].trim(), r[2].trim()));
    }
    analysis::parse_calibration(&text)
}

/// Locations with data that exist in the network, in order of appearance.
fn locations(app: &EpanetApp, data: &[Measurement]) -> Vec<String> {
    let node = is_node(app.rpt.calib.param);
    let mut out: Vec<String> = Vec::new();
    for m in data {
        let exists = if node {
            app.net.node_index(&m.location).is_some()
        } else {
            app.net.link_index(&m.location).is_some()
        };
        if exists && !out.contains(&m.location) {
            out.push(m.location.clone());
        }
    }
    out
}

/// Simulated series of the calibration parameter at a location.
fn simulated(app: &EpanetApp, res: &SimResults, loc: &str) -> Option<Vec<f32>> {
    let (_, nv, lv) = PARAMS[app.rpt.calib.param];
    if let Some(v) = nv {
        let r = app.node_result_index(app.net.node_index(loc)?)?;
        Some(res.node_series(r, v))
    } else {
        let r = app.link_result_index(app.net.link_index(loc)?)?;
        Some(res.link_series(r, lv?))
    }
}

/// Error sums for the whole network (index 0) and per location.
fn statistics(
    app: &EpanetApp,
    res: &SimResults,
    data: &[Measurement],
    locs: &[String],
) -> Vec<ErrorSums> {
    let mut sums = vec![ErrorSums::default(); locs.len() + 1];
    for (k, loc) in locs.iter().enumerate() {
        let Some(series) = simulated(app, res, loc) else {
            continue;
        };
        for m in data.iter().filter(|m| &m.location == loc) {
            if let Some(y) = analysis::interpolate_series(res, &series, m.hours) {
                sums[0].add(m.value, y);
                sums[k + 1].add(m.value, y);
            }
        }
    }
    sums
}

fn stats_text(app: &EpanetApp) -> Option<String> {
    let res = app.results.as_ref()?;
    let data = measurements(app);
    let locs = locations(app, &data);
    if locs.is_empty() {
        return None;
    }
    let sums = statistics(app, res, &data, &locs);
    let mut s = format!(
        "\n Calibration Report for {}\n\n",
        param_name(app, app.rpt.calib.param)
    );
    s.push_str("                Num    Observed    Computed    Mean     RMS\n");
    s.push_str("  Location      Obs        Mean        Mean   Error   Error\n");
    s.push_str("  ---------------------------------------------------------\n");
    let row = |name: &str, x: &ErrorSums| {
        format!(
            "  {:<14}{:>3}{:>12.2}{:>12.2}{:>8.3}{:>8.3}\n",
            name,
            x.n,
            x.mean_observed(),
            x.mean_computed(),
            x.mean_error(),
            x.rms_error()
        )
    };
    let mut means = ErrorSums::default();
    for (k, loc) in locs.iter().enumerate() {
        let x = &sums[k + 1];
        if x.n == 0 {
            continue;
        }
        s.push_str(&row(loc, x));
        means.add(x.mean_observed(), x.mean_computed());
    }
    s.push_str("  ---------------------------------------------------------\n");
    s.push_str(&row("Network", &sums[0]));
    s.push_str(&format!(
        "\n  Correlation Between Means: {:.3}\n",
        means.correlation()
    ));
    Some(s)
}

/// The report as text, for copy / save.
pub fn text(app: &EpanetApp) -> Option<String> {
    stats_text(app)
}

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    // Data loaded from a file arrives asynchronously.
    for item in app.rpt.calib.inbox.drain() {
        match item {
            Ok((name, bytes)) => {
                let data = analysis::parse_calibration(&crate::io::decode_text(&bytes));
                let mut rows: Vec<[String; 3]> = Vec::new();
                let mut last = String::new();
                for m in data {
                    let loc = if m.location == last {
                        String::new()
                    } else {
                        m.location.clone()
                    };
                    last = m.location.clone();
                    rows.push([loc, format!("{}", m.hours), format!("{}", m.value)]);
                }
                rows.push(Default::default());
                app.rpt.calib.rows = rows;
                app.status = format!("Loaded calibration data from {name}");
            }
            Err(e) => app.status = e,
        }
    }
    let mut tab = app.rpt.calib.tab;
    tabs(ui, &mut tab);
    app.rpt.calib.tab = tab;
    egui::Frame::NONE
        .stroke(Stroke::new(1.0, Color32::from_gray(190)))
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            if tab > 0 && app.results.is_none() {
                ui.label(RichText::new("Run an analysis first (Project > Analyze or F5).").weak());
                return;
            }
            match tab {
                0 => data_page(app, ui),
                1 => series_page(app, ui),
                2 => correlation_page(app, ui),
                _ => stats_page(app, ui),
            }
        });
}

fn tabs(ui: &mut Ui, current: &mut u8) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (k, n) in [
            "Calibration Data",
            "Time Series Plot",
            "Correlation Plot",
            "Error Statistics",
        ]
        .iter()
        .enumerate()
        {
            if ui
                .add(
                    egui::Button::new(RichText::new(*n).size(13.5))
                        .selected(*current as usize == k),
                )
                .clicked()
            {
                *current = k as u8;
            }
        }
    });
}

fn data_page(app: &mut EpanetApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let param = app.rpt.calib.param;
    let names: Vec<String> = (0..PARAMS.len()).map(|k| param_name(app, k)).collect();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new("Enter measured values grouped by location:").size(13.5));
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 10.0)
                .max_width(420.0)
                .show(ui, |ui| {
                    let rows = &mut app.rpt.calib.rows;
                    egui::Grid::new("calib_grid")
                        .num_columns(3)
                        .striped(true)
                        .spacing([4.0, 4.0])
                        .show(ui, |ui| {
                            ui.strong(if is_node(param) { "Node ID" } else { "Link ID" });
                            ui.strong("Time (Hrs)");
                            ui.strong(&names[param]);
                            ui.end_row();
                            for r in rows.iter_mut() {
                                for c in r.iter_mut() {
                                    ui.add(egui::TextEdit::singleline(c).desired_width(120.0));
                                }
                                ui.end_row();
                            }
                        });
                    // Keep one empty row at the end for new entries.
                    if rows.last().is_none_or(|r| r.iter().any(|c| !c.trim().is_empty())) {
                        rows.push(Default::default());
                    }
                });
        });
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("Parameter to calibrate").size(13.5));
            let mut p = param;
            egui::ComboBox::from_id_salt("calib_param")
                .selected_text(&names[p])
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (k, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut p, k, n);
                    }
                });
            app.rpt.calib.param = p;
            ui.add_space(14.0);
            let w = [210.0, 34.0];
            if ui.add_sized(w, egui::Button::new("Load")).on_hover_text("Load measured data from a file").clicked() {
                crate::io::open_file(&app.rpt.calib.inbox, &ctx, "Calibration data", &["dat", "txt", "csv"]);
            }
            ui.add_space(6.0);
            if ui.add_sized(w, egui::Button::new("Save")).on_hover_text("Save the measured data to a file").clicked() {
                let data = measurements(app);
                match crate::io::save_report(&analysis::format_calibration(&data), "calibration.dat", ui.ctx()) {
                    Ok(()) => app.status = "Saved calibration data".into(),
                    Err(e) => app.status = format!("Save failed: {e}"),
                }
            }
            ui.add_space(6.0);
            if ui.add_sized(w, egui::Button::new("Clear")).clicked() {
                app.rpt.calib.rows = vec![Default::default()];
            }
            ui.add_space(14.0);
            ui.label(
                RichText::new("Each line: location, time (hours or H:MM)\nand measured value. Leave the\nlocation blank to repeat the one above.")
                    .small()
                    .weak(),
            );
        });
    });
}

fn no_data(ui: &mut Ui) {
    ui.label(RichText::new("There are no calibration data to process.").weak());
}

fn series_page(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.as_ref().unwrap();
    let data = measurements(app);
    let locs = locations(app, &data);
    if locs.is_empty() {
        return no_data(ui);
    }
    let mut sel = app.rpt.calib.location.min(locs.len() - 1);
    let node = is_node(app.rpt.calib.param);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(130.0);
            ui.strong(if node { "Node" } else { "Link" });
            egui::ScrollArea::vertical()
                .id_salt("calib_locs")
                .show(ui, |ui| {
                    for (k, l) in locs.iter().enumerate() {
                        ui.selectable_value(&mut sel, k, l);
                    }
                });
        });
        ui.vertical(|ui| {
            let loc = &locs[sel];
            let title = format!(
                "{} for {} {loc}",
                param_name(app, app.rpt.calib.param),
                if node { "Node" } else { "Link" }
            );
            ui.vertical_centered(|ui| ui.label(RichText::new(title).strong().size(13.5)));
            let sim: PlotPoints = simulated(app, res, loc)
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(p, &y)| [res.period_time(p) as f64 / 3600.0, y as f64])
                .collect();
            let obs: PlotPoints = data
                .iter()
                .filter(|m| &m.location == loc)
                .map(|m| [m.hours, m.value])
                .collect();
            Plot::new("calib_ts")
                .show_crosshair(false)
                .legend(egui_plot::Legend::default())
                .x_axis_label("Hour")
                .allow_scroll(false)
                .show(ui, |plot| {
                    plot.line(
                        Line::new("Computed", sim)
                            .stroke(Stroke::new(2.0, Color32::from_rgb(0x33, 0xb5, 0xe5))),
                    );
                    plot.points(
                        Points::new("Measured", obs)
                            .shape(MarkerShape::Cross)
                            .radius(5.0)
                            .color(Color32::from_rgb(0xed, 0x1c, 0x24)),
                    );
                });
        });
    });
    app.rpt.calib.location = sel;
}

fn correlation_page(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.as_ref().unwrap();
    let data = measurements(app);
    let locs = locations(app, &data);
    if locs.is_empty() {
        return no_data(ui);
    }
    let sums = statistics(app, res, &data, &locs);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(format!(
                "Correlation Plot for {}",
                param_name(app, app.rpt.calib.param)
            ))
            .strong()
            .size(13.5),
        );
    });
    let mut series: Vec<(String, Vec<[f64; 2]>)> = Vec::new();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for loc in &locs {
        let Some(sim) = simulated(app, res, loc) else {
            continue;
        };
        let pts: Vec<[f64; 2]> = data
            .iter()
            .filter(|m| &m.location == loc)
            .filter_map(|m| Some([m.value, analysis::interpolate_series(res, &sim, m.hours)?]))
            .collect();
        for p in &pts {
            lo = lo.min(p[0].min(p[1]));
            hi = hi.max(p[0].max(p[1]));
        }
        series.push((loc.clone(), pts));
    }
    let footer = 26.0;
    Plot::new("calib_corr")
        .show_crosshair(false)
        .legend(egui_plot::Legend::default())
        .x_axis_label("Measured")
        .y_axis_label("Computed")
        .height(ui.available_height() - footer)
        .allow_scroll(false)
        .show(ui, |plot| {
            if lo.is_finite() {
                plot.line(
                    Line::new("Perfect fit", PlotPoints::from(vec![[lo, lo], [hi, hi]]))
                        .stroke(Stroke::new(1.0, Color32::from_gray(120))),
                );
            }
            for (k, (loc, pts)) in series.into_iter().enumerate() {
                plot.points(
                    Points::new(loc, PlotPoints::from(pts))
                        .shape(MarkerShape::Cross)
                        .radius(5.0)
                        .color(MARKERS[k % MARKERS.len()]),
                );
            }
        });
    ui.vertical_centered(|ui| {
        ui.label(format!("Correlation Coeff. = {:.2}", sums[0].correlation()));
    });
}

fn stats_page(app: &mut EpanetApp, ui: &mut Ui) {
    match stats_text(app) {
        Some(t) => {
            egui::ScrollArea::both().show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(t).font(FontId::monospace(13.0))).extend());
            });
        }
        None => no_data(ui),
    }
}
