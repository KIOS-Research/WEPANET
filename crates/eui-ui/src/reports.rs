//! Reports, as in EPANET-UI's `reportframe`: each report appears in a framed
//! window over the map area, with a title bar holding a ☰ menu (copy, save
//! and report options) and a close button. Reports that need choices first
//! (time series, percentiles, profile, fire flow) have a selector panel above
//! the Map Viewer.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use egui_plot::{FilledArea, Line, LineStyle, Plot, PlotPoints, Text};
use eui_core::analysis::{self, EnergyBalance, SystemFlow};
use eui_core::props::Element;
use eui_core::{LinkKind, LinkVar, NodeKind, NodeVar, SimResults};

use crate::app::{EpanetApp, ReportKind, ViewMode};
use crate::icons::{self, Icon};

/// The selector panel shown above the Map Viewer, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Selector {
    #[default]
    None,
    TimeSeries,
    Percentiles,
    Profile,
    FireFlow,
}

/// Node variables offered in the report selectors.
pub const NODE_VARS: [NodeVar; 4] = [
    NodeVar::Demand,
    NodeVar::Head,
    NodeVar::Pressure,
    NodeVar::Quality,
];
/// Link variables offered in the report selectors.
pub const LINK_VARS: [LinkVar; 7] = [
    LinkVar::Flow,
    LinkVar::Velocity,
    LinkVar::Headloss,
    LinkVar::Quality,
    LinkVar::Setting,
    LinkVar::Reaction,
    LinkVar::Friction,
];

/// One curve of the Time Series Report.
#[derive(Clone, Debug, PartialEq)]
pub struct TsSeries {
    pub node: bool,
    pub id: String,
    pub var: usize,
    /// Text shown in the chart legend.
    pub legend: String,
    /// Plotted against the right-hand y-axis.
    pub right: bool,
}

/// Most curves a Time Series Report shows (EPANET-UI's `MaxSeries`).
const MAX_SERIES: usize = 6;

/// The series being added or edited in the Time Series Selector.
#[derive(Clone, Debug)]
pub struct TsEdit {
    /// Position in the list, or None when adding.
    index: Option<usize>,
    series: TsSeries,
    /// The map selection last copied into the form.
    seen: Option<Element>,
}

/// The Time Series Selector's working copy, applied by View.
#[derive(Clone, Debug, Default)]
pub struct TsDraft {
    series: Vec<TsSeries>,
    selected: usize,
    time_of_day: bool,
    edit: Option<TsEdit>,
    /// The report had no series when the selector opened (Cancel closes it).
    was_empty: bool,
}

/// What the cached percentile data was computed for: results, network,
/// node or link, parameter, percentiles.
type PercentileKey = (u64, u64, bool, usize, [u32; 3]);

/// Choices and cached data of the reports.
pub struct ReportState {
    pub selector: Selector,
    pub time_of_day: bool,
    // time series
    pub ts: Vec<TsSeries>,
    pub ts_draft: Option<TsDraft>,
    /// Show the Time Series Report as a table.
    pub ts_table: bool,
    /// Report window position (from the map area's corner) and size, once moved.
    pub window: Option<(egui::Vec2, egui::Vec2)>,
    /// Time of day on the System Flow Report's x-axis (on by default, as in EPANET-UI).
    pub sf_time_of_day: bool,
    // percentiles: [lower, mid range, upper]
    pub pc_node: bool,
    pub pc_var: usize,
    pub pc: [u32; 3],
    pub pc_shown: Option<(bool, usize, [u32; 3])>,
    // profile
    pub pf_start: String,
    pub pf_end: String,
    pub pf: Option<(usize, Vec<usize>)>,
    pub pf_elev: bool,
    pub pf_labels: bool,
    // tables
    pub sort_nodes: Option<(usize, bool)>,
    pub sort_links: Option<(usize, bool)>,
    pub sort_pumps: Option<(usize, bool)>,
    pub sort_ff: Option<(usize, bool)>,
    pub sf_data: bool,
    pub energy_tab: u8,
    pub calib: crate::calibration::CalibState,
    pub ff: crate::fireflow::FireFlowState,
    cache_key: (u64, u64),
    energy: Option<EnergyBalance>,
    flows: Vec<SystemFlow>,
    pc_cache: Option<(PercentileKey, Vec<[f64; 5]>)>,
}

impl Default for ReportState {
    fn default() -> Self {
        ReportState {
            selector: Selector::None,
            time_of_day: false,
            ts: Vec::new(),
            ts_draft: None,
            ts_table: false,
            window: None,
            sf_time_of_day: true,
            pc_node: true,
            pc_var: 2,
            pc: [5, 50, 95],
            pc_shown: None,
            pf_start: String::new(),
            pf_end: String::new(),
            pf: None,
            pf_elev: false,
            pf_labels: true,
            sort_nodes: None,
            sort_links: None,
            sort_pumps: None,
            sort_ff: None,
            sf_data: false,
            energy_tab: 0,
            calib: Default::default(),
            ff: Default::default(),
            cache_key: (u64::MAX, u64::MAX),
            energy: None,
            flows: Vec::new(),
            pc_cache: None,
        }
    }
}

// ----- labels ----------------------------------------------------------------------

pub fn quality_label(res: &SimResults) -> String {
    match res.quality_flag {
        1 if !res.chem_name.is_empty() => res.chem_name.clone(),
        1 => "Chemical".into(),
        2 => "Age".into(),
        3 => "Trace".into(),
        _ => "Quality".into(),
    }
}

pub fn node_var_label(v: NodeVar, res: Option<&SimResults>) -> String {
    match v {
        NodeVar::Demand => "Total Demand".into(),
        NodeVar::Head => "Hydraulic Head".into(),
        NodeVar::Pressure => "Pressure".into(),
        NodeVar::Quality => res.map_or("Quality".into(), quality_label),
    }
}

pub fn link_var_label(v: LinkVar, res: Option<&SimResults>) -> String {
    match v {
        LinkVar::Flow => "Flow Rate".into(),
        LinkVar::Velocity => "Velocity".into(),
        LinkVar::Headloss => "Unit Head Loss".into(),
        LinkVar::Quality => res.map_or("Quality".into(), quality_label),
        v => v.label().into(),
    }
}

fn hhmm(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

/// Title shown in the report window's bar.
pub fn title(app: &EpanetApp) -> String {
    let at = || {
        app.results
            .as_ref()
            .map(|r| format!(" at {} hrs", hhmm(r.period_time(app.period))))
            .unwrap_or_default()
    };
    match app.report_kind {
        ReportKind::Status => "Status Report".into(),
        ReportKind::Energy => "Energy Report".into(),
        ReportKind::Pumping => "Pumping Report".into(),
        ReportKind::SystemFlow => "System Flow Report".into(),
        ReportKind::TimeSeries => "Time Series Report".into(),
        ReportKind::Percentiles => "Percentiles Plot".into(),
        ReportKind::Profile => "Hydraulic Profile Plot".into(),
        ReportKind::NodeTable => format!("Node Results{}", at()),
        ReportKind::LinkTable => format!("Link Results{}", at()),
        ReportKind::Calibration => "Calibration Report".into(),
        ReportKind::FireFlow => "Fire Flow Report".into(),
    }
}

/// Opens a report the way EPANET-UI does: reports that need choices show
/// their selector first.
pub fn open(app: &mut EpanetApp, kind: ReportKind) {
    app.report_kind = kind;
    app.rpt.selector = Selector::None;
    match kind {
        // As in EPANET-UI, the selector opens with the report, over the map
        // (objects are picked there) until the series are viewed.
        ReportKind::TimeSeries => {
            app.rpt.selector = Selector::TimeSeries;
            app.rpt.ts_draft = None;
            if app.rpt.ts.is_empty() {
                app.view_mode = ViewMode::Map;
                return;
            }
        }
        ReportKind::Percentiles => {
            app.rpt.selector = Selector::Percentiles;
            if app.rpt.pc_shown.is_none() {
                app.rpt.pc_shown = Some((app.rpt.pc_node, app.rpt.pc_var, app.rpt.pc));
            }
        }
        ReportKind::Profile => {
            app.rpt.selector = Selector::Profile;
            if app.rpt.pf.is_none() {
                app.view_mode = ViewMode::Map;
                return;
            }
        }
        ReportKind::FireFlow => app.rpt.selector = Selector::FireFlow,
        _ => {}
    }
    app.view_mode = ViewMode::Report;
}

fn refresh_cache(app: &mut EpanetApp) {
    let key = (app.results_id, app.generation);
    if app.rpt.cache_key == key {
        return;
    }
    app.rpt.cache_key = key;
    app.rpt.pc_cache = None;
    match &app.results {
        Some(res) => {
            app.rpt.energy = Some(match &app.energy_sums {
                Some(s) => analysis::energy_balance_from_sums(&app.net, res, s),
                None => analysis::energy_balance(&app.net, res),
            });
            app.rpt.flows = analysis::system_flow(&app.net, res);
        }
        None => {
            app.rpt.energy = None;
            app.rpt.flows.clear();
        }
    }
}

// ----- window --------------------------------------------------------------------------

pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    refresh_cache(app);
    let pal = app.palette();
    let avail = ui.available_rect_before_wrap();
    ui.allocate_rect(avail, Sense::hover());
    let rect = report_window(app, ui, avail);
    let body_bg = if ui.visuals().dark_mode {
        ui.visuals().extreme_bg_color
    } else {
        Color32::WHITE
    };
    ui.painter().rect(
        rect,
        0.0,
        body_bg,
        Stroke::new(1.0, Color32::from_gray(170)),
        egui::StrokeKind::Outside,
    );
    let mut close = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        // Title bar: ☰ menu, title, close.
        let (bar, _) = ui.allocate_exact_size(vec2(rect.width(), 34.0), Sense::hover());
        ui.painter().rect_filled(bar, 0.0, pal.menu);
        ui.painter().text(
            bar.center(),
            Align2::CENTER_CENTER,
            title(app),
            crate::fonts::bold(14.0),
            Color32::WHITE,
        );
        let menu_rect = Rect::from_center_size(bar.left_center() + vec2(22.0, 0.0), vec2(28.0, 28.0));
        let menu = ui.interact(menu_rect, ui.id().with("rpt_menu"), Sense::click());
        white_button(ui, menu_rect, menu.hovered());
        icons::paint(ui.painter(), Icon::Menu, menu_rect.center(), 18.0, Color32::BLACK);
        let close_rect =
            Rect::from_center_size(bar.right_center() - vec2(22.0, 0.0), vec2(28.0, 28.0));
        let x = ui.interact(close_rect, ui.id().with("rpt_close"), Sense::click());
        white_button(ui, close_rect, x.hovered());
        ui.painter().text(
            close_rect.center(),
            Align2::CENTER_CENTER,
            "×",
            FontId::proportional(18.0),
            Color32::BLACK,
        );
        if x.on_hover_text("Close the report").clicked() {
            close = true;
        }
        egui::Popup::menu(&menu).show(|ui| report_menu(app, ui));

        egui::Frame::NONE.inner_margin(8).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            if app.results_stale() {
                ui.label(
                    RichText::new("Results need updating — run Analyze (F5)")
                        .color(Color32::from_rgb(0xb0, 0x6a, 0x00)),
                );
            }
            let needs_results = !matches!(
                app.report_kind,
                ReportKind::Status | ReportKind::Calibration | ReportKind::FireFlow
            );
            if needs_results && app.results.is_none() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(
                            "No results available. Run an analysis first (Project > Analyze or F5).",
                        )
                        .weak(),
                    );
                });
                return;
            }
            match app.report_kind {
                ReportKind::Status => status(app, ui),
                ReportKind::Energy => energy(app, ui),
                ReportKind::Pumping => pumping(app, ui),
                ReportKind::SystemFlow => system_flow(app, ui),
                ReportKind::TimeSeries => time_series(app, ui),
                ReportKind::Percentiles => percentiles(app, ui),
                ReportKind::Profile => profile(app, ui),
                ReportKind::NodeTable => node_table(app, ui),
                ReportKind::LinkTable => link_table(app, ui),
                ReportKind::Calibration => crate::calibration::show(app, ui),
                ReportKind::FireFlow => crate::fireflow::report(app, ui),
            }
        });
    });
    if close {
        app.view_mode = ViewMode::Map;
        app.rpt.selector = Selector::None;
    }
}

/// Where the report window goes: centered by default, then wherever the user
/// drags it (by the title bar) and as large as they make it (by the grips on
/// its sides and bottom), always inside `avail`. A double-click on the title
/// bar puts it back.
fn report_window(app: &mut EpanetApp, ui: &Ui, avail: Rect) -> Rect {
    const MARGIN: f32 = 12.0;
    const MIN: egui::Vec2 = vec2(320.0, 220.0);
    let w = (avail.width() - 2.0 * MARGIN).clamp(200.0, 1100.0);
    let default = Rect::from_min_size(
        pos2(avail.center().x - w / 2.0, avail.top() + MARGIN),
        vec2(w, (avail.height() - 2.0 * MARGIN).max(200.0)),
    );
    let fit = |r: Rect| {
        let size = r.size().min(avail.size()).max(MIN.min(avail.size()));
        let min = r.min.clamp(avail.min, (avail.max - size).max(avail.min));
        Rect::from_min_size(min, size)
    };
    let mut rect = match app.rpt.window {
        Some((offset, size)) => fit(Rect::from_min_size(avail.min + offset, size)),
        None => default,
    };
    let id = ui.id().with("report_window");
    // title bar (the ☰ and × buttons sit on top of it)
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 34.0));
    let drag = ui.interact(bar, id.with("move"), Sense::click_and_drag());
    let mut moved = false;
    if drag.double_clicked() {
        app.rpt.window = None;
        return default;
    }
    if drag.dragged() {
        rect = fit(rect.translate(drag.drag_delta()));
        moved = true;
    }
    if drag.hovered() || drag.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    // grips: left, right, bottom
    let grip = 7.0;
    let sides = [
        (
            Rect::from_min_max(
                pos2(rect.left() - grip, rect.top() + 34.0),
                pos2(rect.left() + 2.0, rect.bottom()),
            ),
            0,
        ),
        (
            Rect::from_min_max(
                pos2(rect.right() - 2.0, rect.top() + 34.0),
                pos2(rect.right() + grip, rect.bottom()),
            ),
            1,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - 2.0),
                pos2(rect.right(), rect.bottom() + grip),
            ),
            2,
        ),
    ];
    for (zone, side) in sides {
        let r = ui.interact(zone, id.with(("grip", side)), Sense::drag());
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(if side == 2 {
                egui::CursorIcon::ResizeVertical
            } else {
                egui::CursorIcon::ResizeHorizontal
            });
        }
        if r.dragged() {
            let d = r.drag_delta();
            match side {
                0 => rect.min.x = (rect.min.x + d.x).min(rect.max.x - MIN.x),
                1 => rect.max.x = (rect.max.x + d.x).max(rect.min.x + MIN.x),
                _ => rect.max.y = (rect.max.y + d.y).max(rect.min.y + MIN.y),
            }
            rect = fit(rect);
            moved = true;
        }
    }
    if moved {
        app.rpt.window = Some((rect.min - avail.min, rect.size()));
    }
    // the grips' dots, as EPANET-UI draws them
    let dots = Color32::from_gray(150);
    let dot_grid = |c: egui::Pos2, across: bool| {
        for i in 0..2 {
            for j in 0..6 {
                let (dx, dy) = (i as f32 * 3.0 - 1.5, j as f32 * 3.0 - 7.5);
                let p = if across {
                    c + vec2(dy, dx)
                } else {
                    c + vec2(dx, dy)
                };
                ui.painter().circle_filled(p, 0.7, dots);
            }
        }
    };
    dot_grid(pos2(rect.left() - 4.0, rect.center().y), false);
    dot_grid(pos2(rect.right() + 4.0, rect.center().y), false);
    dot_grid(pos2(rect.center().x, rect.bottom() + 4.0), true);
    rect
}

fn white_button(ui: &Ui, rect: Rect, hovered: bool) {
    ui.painter().rect(
        rect,
        CornerRadius::same(3),
        if hovered {
            Color32::from_gray(230)
        } else {
            Color32::WHITE
        },
        Stroke::new(1.0, Color32::from_gray(150)),
        egui::StrokeKind::Inside,
    );
}

/// The ☰ menu, laid out as EPANET-UI's report pop-up menus: the report's own
/// items first, then Copy to Clipboard and Save to File.
fn report_menu(app: &mut EpanetApp, ui: &mut Ui) {
    ui.set_min_width(190.0);
    let r = &mut app.rpt;
    let chart_table = |ui: &mut Ui, table: &mut bool| {
        if icons::menu_item(ui, Icon::TimeSeries, "View Chart", true).clicked() {
            *table = false;
        }
        if icons::menu_item(ui, Icon::NodeTable, "View Table", true).clicked() {
            *table = true;
        }
    };
    let mut own = true;
    match app.report_kind {
        ReportKind::SystemFlow => {
            chart_table(ui, &mut r.sf_data);
            if icons::check_item(ui, "Plot Time of Day", r.sf_time_of_day).clicked() {
                r.sf_time_of_day = !r.sf_time_of_day;
            }
        }
        ReportKind::TimeSeries => {
            chart_table(ui, &mut r.ts_table);
            ui.separator();
            if icons::menu_item(ui, Icon::TimeSeries, "Change Series Data", true).clicked() {
                r.selector = Selector::TimeSeries;
                r.ts_draft = None;
                app.view_mode = ViewMode::Map;
            }
        }
        ReportKind::Percentiles => {
            if icons::menu_item(ui, Icon::Percentiles, "Change Selection", true).clicked() {
                r.selector = Selector::Percentiles;
            }
            if icons::check_item(ui, "Plot Time of Day", r.time_of_day).clicked() {
                r.time_of_day = !r.time_of_day;
            }
        }
        ReportKind::Profile => {
            if icons::check_item(ui, "Show Elevation Profile", r.pf_elev).clicked() {
                r.pf_elev = !r.pf_elev;
            }
            if icons::check_item(ui, "Show Node Labels", r.pf_labels).clicked() {
                r.pf_labels = !r.pf_labels;
            }
            if icons::menu_item(ui, Icon::Profile, "Select Profile Path", true).clicked() {
                r.selector = Selector::Profile;
                app.view_mode = ViewMode::Map;
            }
        }
        ReportKind::FireFlow => {
            if icons::menu_item(ui, Icon::FireFlow, "Change Selection", true).clicked() {
                r.selector = Selector::FireFlow;
            }
        }
        _ => own = false,
    }
    if own {
        ui.separator();
    }
    let text = report_text(app);
    if icons::menu_item(ui, Icon::Copy, "Copy to Clipboard", text.is_some()).clicked()
        && let Some(t) = &text
    {
        ui.ctx().copy_text(t.clone());
        app.status = "Report copied to the clipboard".into();
    }
    if icons::menu_item(ui, Icon::Save, "Save to File", text.is_some()).clicked()
        && let Some(t) = &text
    {
        let name = format!("{}.txt", title(app).replace(' ', "_"));
        match crate::io::save_report(t, &name, ui.ctx()) {
            Ok(()) => app.status = format!("Saved {name}"),
            Err(e) => app.status = format!("Save failed: {e}"),
        }
    }
}

/// The report as plain text (tab-separated for tables), for copy / save.
fn report_text(app: &EpanetApp) -> Option<String> {
    let res = app.results.as_ref();
    match app.report_kind {
        ReportKind::Status => (!app.report.is_empty()).then(|| app.report.clone()),
        ReportKind::Energy => app.rpt.energy.as_ref().map(energy_text),
        ReportKind::Pumping => res.map(|r| tsv(&pump_rows(r))),
        ReportKind::SystemFlow => res.map(|r| tsv(&sysflow_rows(app, r))),
        ReportKind::TimeSeries => res.map(|r| tsv(&ts_rows(app, r))),
        ReportKind::Percentiles => res.map(|_| tsv(&pc_rows(app))),
        ReportKind::NodeTable => res.map(|r| tsv(&node_rows(app, r))),
        ReportKind::LinkTable => res.map(|r| tsv(&link_rows(app, r))),
        ReportKind::Calibration => crate::calibration::text(app),
        ReportKind::FireFlow => crate::fireflow::text(app),
        ReportKind::Profile => None,
    }
}

/// A table: header row then data rows.
pub type Rows = Vec<Vec<String>>;

fn tsv(rows: &Rows) -> String {
    rows.iter()
        .map(|r| {
            r.iter()
                .map(|c| c.replace('\n', " "))
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fmt2(v: f64) -> String {
    crate::style::fmt_fixed(v)
}

// ----- tables ---------------------------------------------------------------------------

/// A sortable results grid (EPANET-UI's report tables). `rows[0]` holds the
/// column headings ("name\nunits"). Clicking a heading sorts by that column.
pub fn table(ui: &mut Ui, id: &str, rows: &Rows, sort: &mut Option<(usize, bool)>, footer: &str) {
    let Some((head, body)) = rows.split_first() else {
        return;
    };
    let ncols = head.len();
    let mut order: Vec<usize> = (0..body.len()).collect();
    if let Some((col, asc)) = *sort {
        order.sort_by(|&a, &b| {
            let (x, y) = (&body[a][col], &body[b][col]);
            let o = match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                (Ok(p), Ok(q)) => p.total_cmp(&q),
                _ => natural_cmp(x, y),
            };
            if asc { o } else { o.reverse() }
        });
    }
    let v = ui.visuals().clone();
    let grid = Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color);
    let fixed_bg = if v.dark_mode {
        v.faint_bg_color
    } else {
        Color32::from_gray(0xf3)
    };
    let footer_h = 34.0;
    let avail_w = ui.available_width();
    let first_w = (avail_w * 0.2).clamp(110.0, 220.0);
    let other_w = ((avail_w - first_w) / (ncols - 1).max(1) as f32).max(92.0);
    let total_w = first_w + other_w * (ncols - 1) as f32;
    let col_x = |c: usize| {
        if c == 0 {
            0.0
        } else {
            first_w + other_w * (c - 1) as f32
        }
    };
    let col_w = |c: usize| if c == 0 { first_w } else { other_w };
    let font = FontId::proportional(13.5);
    let body_h = ui.available_height() - footer_h;
    egui::ScrollArea::horizontal()
        .id_salt((id, "h"))
        .max_height(body_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_width(total_w);
            // heading
            let (hr, _) = ui.allocate_exact_size(vec2(total_w, 50.0), Sense::hover());
            for (c, heading) in head.iter().enumerate() {
                let r =
                    Rect::from_min_size(hr.min + vec2(col_x(c), 0.0), vec2(col_w(c), hr.height()));
                let resp = ui.interact(r, ui.id().with((id, "head", c)), Sense::click());
                ui.painter().rect_filled(
                    r,
                    0.0,
                    if resp.hovered() {
                        v.widgets.hovered.weak_bg_fill
                    } else {
                        fixed_bg
                    },
                );
                ui.painter()
                    .rect_stroke(r, 0.0, grid, egui::StrokeKind::Inside);
                let (align, x) = if c == 0 {
                    (Align2::LEFT_BOTTOM, r.left() + 6.0)
                } else {
                    (Align2::CENTER_BOTTOM, r.center().x)
                };
                let mut y = r.bottom() - 6.0;
                let mut label = heading.clone();
                if let Some((col, asc)) = *sort
                    && col == c
                {
                    label.push_str(if asc { " ▲" } else { " ▼" });
                }
                for line in label.lines().rev() {
                    ui.painter()
                        .text(pos2(x, y), align, line, font.clone(), v.text_color());
                    y -= 17.0;
                }
                if resp.clicked() {
                    *sort = match *sort {
                        Some((col, asc)) if col == c => Some((c, !asc)),
                        _ => Some((c, true)),
                    };
                }
            }
            let row_h = 26.0;
            egui::ScrollArea::vertical()
                .id_salt((id, "v"))
                .auto_shrink(false)
                .show_rows(ui, row_h, order.len(), |ui, range| {
                    for k in range {
                        let row = &body[order[k]];
                        let (rr, _) = ui.allocate_exact_size(vec2(total_w, row_h), Sense::hover());
                        for c in 0..ncols {
                            let r = Rect::from_min_size(
                                rr.min + vec2(col_x(c), 0.0),
                                vec2(col_w(c), row_h),
                            );
                            if c == 0 {
                                ui.painter().rect_filled(r, 0.0, fixed_bg);
                                ui.painter().vline(r.right(), r.y_range(), grid);
                            }
                            ui.painter().hline(r.x_range(), r.bottom(), grid);
                            let text = row.get(c).map(String::as_str).unwrap_or("");
                            let (align, p) = if c == 0 {
                                (Align2::LEFT_CENTER, r.left_center() + vec2(6.0, 0.0))
                            } else {
                                (Align2::CENTER_CENTER, r.center())
                            };
                            ui.painter()
                                .text(p, align, text, font.clone(), v.text_color());
                        }
                    }
                });
        });
    // footer
    let (fr, _) = ui.allocate_exact_size(vec2(ui.available_width(), footer_h), Sense::hover());
    ui.painter()
        .rect(fr, 0.0, fixed_bg, grid, egui::StrokeKind::Inside);
    ui.painter().text(
        fr.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        footer,
        font.clone(),
        v.text_color(),
    );
    ui.painter().text(
        fr.right_center() - vec2(14.0, 0.0),
        Align2::RIGHT_CENTER,
        "Click a column header to sort it",
        font,
        v.text_color(),
    );
}

/// Compares IDs so that "10" sorts after "9".
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        _ => a.cmp(b),
    }
}

fn node_columns(res: &SimResults) -> Vec<NodeVar> {
    NODE_VARS
        .into_iter()
        .filter(|v| *v != NodeVar::Quality || res.quality_flag != 0)
        .collect()
}

fn node_rows(app: &EpanetApp, res: &SimResults) -> Rows {
    let vars = node_columns(res);
    let mut head = vec!["Node".to_string(), "Type".to_string()];
    for v in &vars {
        let name = match v {
            NodeVar::Head => "Head".to_string(),
            v => node_var_label(*v, Some(res)),
        };
        head.push(format!("{name}\n{}", res.node_units(*v)));
    }
    let mut rows = vec![head];
    for (i, n) in app.net.nodes.iter().enumerate() {
        let Some(r) = app.node_result_index(i) else {
            continue;
        };
        let mut row = vec![n.id.clone(), n.kind.label().to_string()];
        for v in &vars {
            row.push(fmt2(res.node_slice(app.period, *v)[r] as f64));
        }
        rows.push(row);
    }
    // EPANET-UI lists junctions, then reservoirs, then tanks.
    rows[1..].sort_by_key(|r| match r[1].as_str() {
        "Junction" => 0,
        "Reservoir" => 1,
        _ => 2,
    });
    rows
}

fn link_status(s: f32) -> &'static str {
    match s as i32 {
        0..=2 => "Closed",
        4 => "Active",
        _ => "Open",
    }
}

fn link_rows(app: &EpanetApp, res: &SimResults) -> Rows {
    let mut vars = vec![LinkVar::Flow, LinkVar::Velocity, LinkVar::Headloss];
    if res.quality_flag != 0 {
        vars.push(LinkVar::Quality);
    }
    let mut head = vec!["Link".to_string(), "Type".to_string()];
    for v in &vars {
        let name = match v {
            LinkVar::Headloss => "Head Loss".to_string(),
            v => link_var_label(*v, Some(res)),
        };
        head.push(format!("{name}\n{}", res.link_units(*v)));
    }
    head.push("Status".into());
    let mut rows = vec![head];
    for (i, l) in app.net.links.iter().enumerate() {
        let Some(r) = app.link_result_index(i) else {
            continue;
        };
        let mut row = vec![l.id.clone(), l.kind.label().to_string()];
        for v in &vars {
            row.push(fmt2(res.link_slice(app.period, *v)[r] as f64));
        }
        row.push(link_status(res.link_slice(app.period, LinkVar::Status)[r]).into());
        rows.push(row);
    }
    rows[1..].sort_by_key(|r| match r[1].as_str() {
        "Pipe" => 0,
        "Pump" => 1,
        _ => 2,
    });
    rows
}

fn node_table(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.as_ref().unwrap();
    let rows = node_rows(app, res);
    let footer = format!("Unfiltered: {} items", rows.len() - 1);
    let mut sort = app.rpt.sort_nodes;
    table(ui, "node_table", &rows, &mut sort, &footer);
    app.rpt.sort_nodes = sort;
}

fn link_table(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.as_ref().unwrap();
    let rows = link_rows(app, res);
    let footer = format!("Unfiltered: {} items", rows.len() - 1);
    let mut sort = app.rpt.sort_links;
    table(ui, "link_table", &rows, &mut sort, &footer);
    app.rpt.sort_links = sort;
}

// ----- status, energy, pumping ------------------------------------------------------------

fn status(app: &EpanetApp, ui: &mut Ui) {
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        if app.report.is_empty() {
            ui.label(RichText::new("Run an analysis to see EPANET's status report.").weak());
        }
        ui.add(egui::Label::new(RichText::new(&app.report).font(FontId::monospace(13.5))).extend());
    });
}

fn tabs(ui: &mut Ui, current: &mut u8, names: &[&str]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (k, n) in names.iter().enumerate() {
            let selected = *current as usize == k;
            let b = egui::Button::new(RichText::new(*n).size(13.5))
                .selected(selected)
                .corner_radius(CornerRadius {
                    nw: 3,
                    ne: 3,
                    sw: 0,
                    se: 0,
                });
            if ui.add(b).clicked() {
                *current = k as u8;
            }
        }
    });
}

const ENERGY_COLORS: [Color32; 7] = [
    Color32::from_rgb(0xf4, 0x6d, 0x43),
    Color32::from_rgb(0xfd, 0xae, 0x61),
    Color32::from_rgb(0xe6, 0xf5, 0x98),
    Color32::from_rgb(0xab, 0xdd, 0xa4),
    Color32::from_rgb(0x66, 0xc2, 0xa5),
    Color32::from_rgb(0xbd, 0xbd, 0xbd),
    Color32::from_rgb(0x32, 0x88, 0xbd),
];

/// Energy units: MWh/day when large, else kWh/day.
fn energy_units(e: &EnergyBalance) -> (f64, &'static str) {
    if e.supplied() > 1000.0 || e.consumed() > 1000.0 {
        (1000.0, "MwH/Day")
    } else {
        (1.0, "KwH/Day")
    }
}

fn energy_text(e: &EnergyBalance) -> String {
    let (f, u) = energy_units(e);
    let es = e.supplied();
    let mut s = String::new();
    s.push_str(&format!("  System Energy Balance ({u})\n"));
    for (name, v) in e.components() {
        s.push_str(&format!("  {name:<28}{:>8.2}\n", v / f));
    }
    s.push_str(&format!(
        "  Total Energy Supplied (Es):   {:.1} {u}\n",
        es / f
    ));
    s.push_str(&format!(
        "  Total Energy Consumed:       {:.1} {u}\n\n",
        e.consumed() / f
    ));
    s.push_str(&metrics_text(e));
    s
}

fn metrics_text(e: &EnergyBalance) -> String {
    let (f, u) = energy_units(e);
    let es = e.supplied();
    let na = || "N/A".to_string();
    let ratio = |a: f64, b: f64| {
        if b > 0.0 {
            format!("{:.2}", a / b)
        } else {
            na()
        }
    };
    let em = e.min_use;
    let rows = [
        (
            "Efficiency Index    (Ed/Es)".to_string(),
            ratio(e.demands, es),
        ),
        (
            "Friction Loss Index (Ef/Es)".to_string(),
            ratio(e.friction, es),
        ),
        (
            "Leakage Loss Index  (El/Es)".to_string(),
            ratio(e.leakage, es),
        ),
        ("Excess Supply Index (Es/Em)".to_string(), ratio(es, em)),
        (
            "Excess Usage Index  (Ed/Em)".to_string(),
            ratio(e.demands, em),
        ),
        (
            format!("Minimum Energy (Em, {u})"),
            if em > 0.0 {
                format!("{:.2}", em / f)
            } else {
                na()
            },
        ),
    ];
    let mut s = String::from("\n  Performance Metrics\n  -------------------\n\n");
    for (k, (name, v)) in rows.iter().enumerate() {
        s.push_str(&format!("  {name:<29}{v}\n"));
        if k < rows.len() - 1 {
            s.push('\n');
        }
    }
    s.push_str(&format!("  to meet demand at {}\n", e.required_pressure));
    s
}

fn energy(app: &mut EpanetApp, ui: &mut Ui) {
    let Some(e) = app.rpt.energy.clone() else {
        return;
    };
    tabs(
        ui,
        &mut app.rpt.energy_tab,
        &["Energy Balance", "Performance Metrics"],
    );
    let frame = egui::Frame::NONE
        .stroke(Stroke::new(1.0, Color32::from_gray(190)))
        .inner_margin(10);
    frame.show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        if app.rpt.energy_tab == 1 {
            ui.add(
                egui::Label::new(RichText::new(metrics_text(&e)).font(FontId::monospace(13.5)))
                    .extend(),
            );
        } else {
            donut(ui, &e);
        }
    });
}

/// The energy balance as a donut chart with a legend, as EPANET-UI draws it.
fn donut(ui: &mut Ui, e: &EnergyBalance) {
    let (f, u) = energy_units(e);
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, Sense::hover());
    let p = ui.painter_at(rect);
    let text = ui.visuals().text_color();
    p.text(
        rect.center_top() + vec2(0.0, 4.0),
        Align2::CENTER_TOP,
        format!("System Energy Balance ({u})"),
        crate::fonts::bold(13.0),
        Color32::from_rgb(0x10, 0x10, 0xe0),
    );
    let footer_h = 56.0;
    let legend_w = 250.0_f32.min(rect.width() * 0.4);
    let chart = Rect::from_min_max(
        rect.min + vec2(0.0, 26.0),
        pos2(rect.right() - legend_w, rect.bottom() - footer_h),
    );
    let radius = (chart.width().min(chart.height()) * 0.5 - 36.0).max(20.0);
    let inner = radius * 0.62;
    let c = chart.center();
    let comps = e.components();
    let total: f64 = comps.iter().map(|x| x.1).sum();
    if total > 0.0 {
        let mut a0 = 0.0f32;
        for (k, (_, v)) in comps.iter().enumerate() {
            if *v <= 0.0 {
                continue;
            }
            let span = (*v / total) as f32 * std::f32::consts::TAU;
            let n = ((span / 0.05).ceil() as usize).max(2);
            let pt = |a: f32, rad: f32| c + vec2(a.cos() * rad, -a.sin() * rad);
            for s in 0..n {
                let x0 = a0 + span * s as f32 / n as f32;
                let x1 = a0 + span * (s + 1) as f32 / n as f32;
                p.add(egui::Shape::convex_polygon(
                    vec![pt(x0, inner), pt(x0, radius), pt(x1, radius), pt(x1, inner)],
                    ENERGY_COLORS[k],
                    Stroke::NONE,
                ));
            }
            let edge = Stroke::new(1.0, Color32::BLACK);
            p.line_segment([pt(a0, inner), pt(a0, radius)], edge);
            p.line_segment([pt(a0 + span, inner), pt(a0 + span, radius)], edge);
            let mid = a0 + span / 2.0;
            let lp = pt(mid, radius + 20.0);
            p.text(
                lp,
                Align2::CENTER_CENTER,
                format!("{:.2}", v / f),
                FontId::proportional(13.5),
                text,
            );
            a0 += span;
        }
        let ring = |rad: f32| {
            let pts: Vec<egui::Pos2> = (0..=96)
                .map(|k| {
                    let a = std::f32::consts::TAU * k as f32 / 96.0;
                    c + vec2(a.cos() * rad, a.sin() * rad)
                })
                .collect();
            egui::Shape::line(pts, Stroke::new(1.0, Color32::BLACK))
        };
        p.add(ring(radius));
        p.add(ring(inner));
    }
    // legend
    let mut y = chart.center().y - 3.5 * 22.0;
    for (k, (name, _)) in comps.iter().enumerate() {
        let x = rect.right() - legend_w + 10.0;
        let mark = Rect::from_center_size(pos2(x + 12.0, y), vec2(24.0, 11.0));
        p.rect(
            mark,
            CornerRadius::same(5),
            ENERGY_COLORS[k],
            Stroke::new(1.0, Color32::BLACK),
            egui::StrokeKind::Inside,
        );
        p.text(
            pos2(x + 30.0, y),
            Align2::LEFT_CENTER,
            *name,
            FontId::proportional(13.5),
            text,
        );
        y += 22.0;
    }
    let fy = rect.bottom() - footer_h + 8.0;
    p.text(
        pos2(rect.left() + 6.0, fy),
        Align2::LEFT_TOP,
        format!("Total Energy Supplied (Es):   {:.1} {u}", e.supplied() / f),
        FontId::proportional(13.5),
        text,
    );
    p.text(
        pos2(rect.left() + 6.0, fy + 22.0),
        Align2::LEFT_TOP,
        format!("Total Energy Consumed:        {:.1} {u}", e.consumed() / f),
        FontId::proportional(13.5),
        text,
    );
}

fn pump_rows(res: &SimResults) -> Rows {
    let vol = if res.is_si() {
        "Kw-hrs/m3"
    } else {
        "Kw-hrs/Mgal"
    };
    let mut rows = vec![
        [
            "Pump",
            "% Utilized",
            "Efficiency",
            vol,
            "Avg. Kw",
            "Peak Kw",
            "Cost/day",
        ]
        .map(String::from)
        .to_vec(),
    ];
    for p in &res.pumps {
        let id = res
            .link_ids
            .get(p.link_index)
            .cloned()
            .unwrap_or_else(|| "?".into());
        rows.push(vec![
            id,
            fmt2(p.utilization_pct as f64),
            fmt2(p.efficiency_pct as f64),
            fmt2(p.kwh_per_volume as f64),
            fmt2(p.avg_kw as f64),
            fmt2(p.peak_kw as f64),
            fmt2(p.cost_per_day as f64),
        ]);
    }
    rows
}

fn pumping(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.as_ref().unwrap();
    if res.pumps.is_empty() {
        ui.label(RichText::new("There are no pumps with results to report on.").weak());
        return;
    }
    let rows = pump_rows(res);
    let total: f32 = res.pumps.iter().map(|p| p.cost_per_day).sum();
    let footer = format!(
        "Total Cost:   {total:.2}   Demand Charge:   {:.2}",
        res.demand_charge
    );
    let mut sort = app.rpt.sort_pumps;
    table(ui, "pump_table", &rows, &mut sort, &footer);
    app.rpt.sort_pumps = sort;
}

// ----- charts -------------------------------------------------------------------------------

/// A legend row above a chart: (sample, label).
enum Swatch {
    Line(Color32, bool),
    Box(Color32),
}

fn chart_legend(ui: &mut Ui, items: &[(Swatch, String)]) {
    let font = FontId::proportional(13.5);
    let widths: Vec<f32> = items
        .iter()
        .map(|(_, t)| {
            ui.fonts_mut(|f| {
                f.layout_no_wrap(t.clone(), font.clone(), Color32::WHITE)
                    .size()
                    .x
            }) + 34.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 28.0 * items.len().saturating_sub(1) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let mut x = rect.center().x - total / 2.0;
    let p = ui.painter();
    for ((swatch, text), w) in items.iter().zip(widths) {
        let y = rect.center().y;
        match swatch {
            Swatch::Line(c, dashed) => {
                if *dashed {
                    for k in 0..3 {
                        let x0 = x + 2.0 + k as f32 * 8.0;
                        p.line_segment([pos2(x0, y), pos2(x0 + 5.0, y)], Stroke::new(1.5, *c));
                    }
                } else {
                    p.line_segment([pos2(x + 2.0, y), pos2(x + 24.0, y)], Stroke::new(2.0, *c));
                }
            }
            Swatch::Box(c) => {
                p.rect(
                    Rect::from_min_size(pos2(x + 6.0, y - 10.0), vec2(16.0, 20.0)),
                    0.0,
                    *c,
                    Stroke::new(1.0, Color32::from_gray(80)),
                    egui::StrokeKind::Inside,
                );
            }
        }
        p.text(
            pos2(x + 28.0, y),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            ui.visuals().text_color(),
        );
        x += w + 28.0;
    }
}

fn chart_title(ui: &mut Ui, text: &str, size: f32) {
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(text)
                .font(crate::fonts::bold(size))
                .color(Color32::from_rgb(0x10, 0x10, 0xe0)),
        );
    });
}

/// The x-axis of a chart over time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TimeAxis {
    /// Not a time axis.
    None,
    /// Elapsed hours, titled "Hour".
    Hours,
    /// Clock time ("3:00 am"), untitled.
    TimeOfDay,
}

/// The first step in `steps` that puts at most `max` marks over `span`.
fn hour_step(span: f64, steps: &[f64], max: f64) -> f64 {
    steps
        .iter()
        .copied()
        .find(|s| span / s <= max)
        .unwrap_or(*steps.last().unwrap())
}

/// A 1-2-5 step that puts at most `max` marks over `span`.
fn nice_step(span: f64, max: f64) -> f64 {
    if span.is_nan() || span <= 0.0 || span.is_infinite() {
        return 1.0;
    }
    let mut p = 10f64.powf((span / max).log10().floor());
    loop {
        for m in [1.0, 2.0, 5.0] {
            if span / (m * p) <= max {
                return m * p;
            }
        }
        p *= 10.0;
    }
}

const HOUR_STEPS: [f64; 9] = [1.0, 2.0, 3.0, 4.0, 6.0, 12.0, 24.0, 48.0, 168.0];
/// Most grid lines on an axis (EPANET-UI's charts show one per unit or hour).
const MAX_LINES: f64 = 26.0;

/// Values of the grid lines across `bounds`, every `step`.
fn marks_every(bounds: (f64, f64), step: f64) -> Vec<f64> {
    let mut v = (bounds.0 / step).ceil() * step;
    let mut out = Vec::new();
    while v <= bounds.1 + step * 1e-9 && out.len() < 1000 {
        out.push(v);
        v += step;
    }
    out
}

/// The x grid: hours on time axes, otherwise round numbers.
fn x_marks(time: TimeAxis, bounds: (f64, f64)) -> (f64, Vec<f64>) {
    let span = bounds.1 - bounds.0;
    let step = match time {
        TimeAxis::None => nice_step(span, 12.0),
        _ => hour_step(span, &HOUR_STEPS, MAX_LINES),
    };
    (step, marks_every(bounds, step))
}

fn y_marks(bounds: (f64, f64)) -> (f64, Vec<f64>) {
    let step = nice_step(bounds.1 - bounds.0, MAX_LINES);
    (step, marks_every(bounds, step))
}

/// Tick label font of the charts.
fn tick_font() -> FontId {
    FontId::proportional(13.0)
}

/// A tick value with as many decimals as its step needs (0.05 steps give
/// "2.05", not two labels of "2.1").
fn tick_text(value: f64, step: f64) -> String {
    let digits = if step > 0.0 {
        (-step.log10() - 1e-9).ceil().clamp(0.0, 6.0) as usize
    } else {
        0
    };
    let v = if value.abs() < step * 1e-6 {
        0.0
    } else {
        value
    };
    format!("{v:.digits$}")
}

/// A y-axis with EPANET-UI's look: a bold title and a label on every grid line.
fn y_axis_hint<'a>(title: &str) -> egui_plot::AxisHints<'a> {
    egui_plot::AxisHints::new_y()
        .label(RichText::new(title).strong().size(14.0))
        .tick_label_font(tick_font())
        .label_spacing(14.0..=16.0)
        .formatter(|m, _| tick_text(m.value, m.step_size))
}

/// A plot with EPANET-UI's look: a light, even grid with a line at each tick,
/// bold axis titles and no background (see [`show_chart`] for the grid and
/// frame). A time axis has a line every hour (or every few hours on long
/// runs), labeled with the hour or, for clock times, every few hours.
fn base_plot<'a>(id: &str, app: &EpanetApp, time: TimeAxis, y_label: &str) -> Plot<'a> {
    let start = analysis::start_clocktime(&app.net) as f64 / 3600.0;
    let mut x_axis = egui_plot::AxisHints::new_x()
        .tick_label_font(tick_font())
        .label_spacing(14.0..=16.0);
    x_axis = match time {
        TimeAxis::None => x_axis,
        TimeAxis::Hours => x_axis
            .label(RichText::new("Hour").strong().size(14.0))
            .formatter(|m, _| format!("{}", m.value.round())),
        // A clock time on every few hours' line.
        TimeAxis::TimeOfDay => x_axis
            .label(RichText::new("Time of Day").strong().size(14.0))
            .formatter(move |m, r| {
                let label = hour_step(r.end() - r.start(), &HOUR_STEPS[2..], 9.0);
                let clock = m.value + start;
                if (clock / label - (clock / label).round()).abs() > 1e-6 {
                    return String::new();
                }
                analysis::clock_time((clock * 3600.0).rem_euclid(86_400.0).round() as u32)
            }),
    };
    Plot::new(id.to_string())
        .custom_x_axes(vec![x_axis])
        .custom_y_axes(vec![y_axis_hint(y_label)])
        .x_grid_spacer(move |input| {
            let (step, marks) = x_marks(time, input.bounds);
            // Clock times are labeled every few hours; those marks get the
            // label step as their spacing, or egui_plot hides labels wider
            // than one hour.
            let label = hour_step(input.bounds.1 - input.bounds.0, &HOUR_STEPS[2..], 9.0);
            marks
                .into_iter()
                .map(|value| {
                    let clock = (value + start) / label;
                    let labeled = (clock - clock.round()).abs() < 1e-6;
                    egui_plot::GridMark {
                        value,
                        step_size: if time == TimeAxis::TimeOfDay && labeled {
                            label.max(step)
                        } else {
                            step
                        },
                    }
                })
                .collect()
        })
        .y_grid_spacer(|input| {
            let (step, marks) = y_marks(input.bounds);
            marks
                .into_iter()
                .map(|value| egui_plot::GridMark {
                    value,
                    step_size: step,
                })
                .collect()
        })
        .show_grid(false)
        .allow_scroll(false)
        .show_background(false)
        .set_margin_fraction(vec2(0.0, 0.05))
}

/// Shows a [`base_plot`]: an even light-gray grid under the items `add`
/// puts on it, and a black frame around it, as EPANET-UI's charts.
fn show_chart<R>(
    plot: Plot<'_>,
    ui: &mut Ui,
    time: TimeAxis,
    add: impl FnOnce(&mut egui_plot::PlotUi<'_>) -> R,
) -> egui_plot::PlotResponse<R> {
    let grid = if ui.visuals().dark_mode {
        Color32::from_gray(70)
    } else {
        Color32::from_gray(225)
    };
    // The grid is painted, not added as plot lines: lines would count in the
    // plot's automatic bounds and widen them a little more every frame.
    let under = ui.painter().add(egui::Shape::Noop);
    let resp = plot.show(ui, add);
    let t = &resp.transform;
    let frame = *t.frame();
    let b = t.bounds();
    let stroke = Stroke::new(1.0, grid);
    let mut lines = Vec::new();
    for x in x_marks(time, (b.min()[0], b.max()[0])).1 {
        let sx = t.position_from_point_x(x);
        if (frame.left()..=frame.right()).contains(&sx) {
            lines.push(egui::Shape::vline(sx, frame.y_range(), stroke));
        }
    }
    for y in y_marks((b.min()[1], b.max()[1])).1 {
        let sy = t.position_from_point_y(y);
        if (frame.top()..=frame.bottom()).contains(&sy) {
            lines.push(egui::Shape::hline(frame.x_range(), sy, stroke));
        }
    }
    ui.painter().set(under, egui::Shape::Vec(lines));
    ui.painter().rect_stroke(
        frame,
        0.0,
        Stroke::new(1.0, ui.visuals().text_color()),
        egui::StrokeKind::Outside,
    );
    resp
}

/// The time axis of the Time Series and Percentiles charts.
fn time_axis(app: &EpanetApp) -> TimeAxis {
    if app.rpt.time_of_day {
        TimeAxis::TimeOfDay
    } else {
        TimeAxis::Hours
    }
}

fn hour_list(res: &SimResults) -> Vec<f64> {
    (0..res.n_periods)
        .map(|p| res.period_time(p) as f64 / 3600.0)
        .collect()
}

/// Moves the map to the reporting period nearest to `hours`.
fn go_to_hour(app: &mut EpanetApp, hours: f64) {
    if let Some(res) = &app.results {
        let target = hours * 3600.0;
        app.period = (0..res.n_periods)
            .min_by(|&a, &b| {
                (res.period_time(a) as f64 - target)
                    .abs()
                    .total_cmp(&(res.period_time(b) as f64 - target).abs())
            })
            .unwrap_or(0);
    }
}

const STORED: Color32 = Color32::from_rgb(0x84, 0xa2, 0xd8);
const PRODUCED: Color32 = Color32::from_rgb(0xb5, 0xcb, 0xf4);
const CONSUMED: Color32 = Color32::from_rgb(0xdd, 0xe7, 0xfa);

fn sysflow_rows(app: &EpanetApp, res: &SimResults) -> Rows {
    let u = if res.is_si() { "ML" } else { "MG" };
    let start = analysis::start_clocktime(&app.net);
    let mut rows = vec![vec![
        "Time\n(hrs)".to_string(),
        "Time\nof Day".to_string(),
        "Interval\n(hrs)".to_string(),
        format!("Volume Stored\n{u}"),
        format!("Volume Produced\n{u}"),
        format!("Volume Consumed\n{u}"),
    ]];
    for f in &app.rpt.flows {
        rows.push(vec![
            fmt2(f.hours),
            analysis::clock_time(start + (f.hours * 3600.0) as u32),
            fmt2(f.interval_hours),
            fmt2(f.stored),
            fmt2(f.produced),
            fmt2(f.consumed),
        ]);
    }
    rows
}

fn system_flow(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.as_ref().unwrap();
    if app.rpt.sf_data {
        let rows = sysflow_rows(app, res);
        let footer = format!("{} time periods", rows.len() - 1);
        let mut sort = None;
        table(ui, "sysflow_table", &rows, &mut sort, &footer);
        return;
    }
    let flows = &app.rpt.flows;
    if flows.is_empty() {
        return;
    }
    chart_legend(
        ui,
        &[
            (
                Swatch::Box(Color32::from_rgb(0x3d, 0x63, 0xb0)),
                "Stored".into(),
            ),
            (
                Swatch::Box(Color32::from_rgb(0x7f, 0xa7, 0xec)),
                "Produced".into(),
            ),
            (
                Swatch::Box(Color32::from_rgb(0xc4, 0xd6, 0xf7)),
                "Consumed".into(),
            ),
        ],
    );
    let xs: Vec<f64> = flows.iter().map(|f| f.hours).collect();
    let s: Vec<f64> = flows.iter().map(|f| f.stored).collect();
    let sp: Vec<f64> = flows.iter().map(|f| f.stored + f.produced).collect();
    let spc: Vec<f64> = flows
        .iter()
        .map(|f| f.stored + f.produced + f.consumed)
        .collect();
    let lo = s.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = spc.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let base = (lo - (hi - lo) * 0.08).max(0.0);
    let floor = vec![base; xs.len()];
    let u = if res.is_si() { "ML" } else { "MG" };
    let edge = Stroke::new(1.0, Color32::from_gray(140));
    let time = if app.rpt.sf_time_of_day {
        TimeAxis::TimeOfDay
    } else {
        TimeAxis::Hours
    };
    let plot = base_plot("sysflow", app, time, u).include_y(base);
    show_chart(plot, ui, time, |plot| {
        plot.add(
            FilledArea::new("Consumed", &xs, &sp, &spc)
                .fill_color(CONSUMED)
                .stroke(edge),
        );
        plot.add(
            FilledArea::new("Produced", &xs, &s, &sp)
                .fill_color(PRODUCED)
                .stroke(edge),
        );
        plot.add(
            FilledArea::new("Stored", &xs, &floor, &s)
                .fill_color(STORED)
                .stroke(edge),
        );
    });
}

/// EPANET-UI's time series colors (`SeriesColors`).
const SERIES_COLORS: [Color32; MAX_SERIES] = [
    Color32::from_rgb(0x33, 0xb5, 0xe5),
    Color32::from_rgb(0xaa, 0x66, 0xcc),
    Color32::from_rgb(0x99, 0xcc, 0x00),
    Color32::from_rgb(0xff, 0xbb, 0x33),
    Color32::from_rgb(0xff, 0x44, 0x44),
    Color32::BLACK,
];

/// One curve ready to plot.
struct TsData {
    title: String,
    legend: String,
    units: String,
    right: bool,
    values: Vec<f32>,
}

/// The values of a time series curve (None if it has no results).
fn ts_data(app: &EpanetApp, res: &SimResults, s: &TsSeries) -> Option<TsData> {
    let (title, units, values) = if s.node {
        let i = app.net.node_index(&s.id)?;
        let r = app.node_result_index(i)?;
        let v = NODE_VARS[s.var.min(NODE_VARS.len() - 1)];
        let name = match v {
            NodeVar::Demand if app.net.nodes[i].kind != NodeKind::Junction => "Inflow".into(),
            v => node_var_label(v, Some(res)),
        };
        (
            format!("{} {} {name}", app.net.nodes[i].kind.label(), s.id),
            res.node_units(v),
            res.node_series(r, v),
        )
    } else {
        let i = app.net.link_index(&s.id)?;
        let r = app.link_result_index(i)?;
        let v = LINK_VARS[s.var.min(LINK_VARS.len() - 1)];
        (
            format!(
                "{} {} {}",
                app.net.links[i].kind.label(),
                s.id,
                link_var_label(v, Some(res))
            ),
            res.link_units(v),
            res.link_series(r, v),
        )
    };
    let legend = if s.legend.trim().is_empty() {
        title.clone()
    } else {
        s.legend.clone()
    };
    Some(TsData {
        title,
        legend,
        units,
        right: s.right,
        values,
    })
}

fn ts_rows(app: &EpanetApp, res: &SimResults) -> Rows {
    let data: Vec<_> = app
        .rpt
        .ts
        .iter()
        .filter_map(|s| ts_data(app, res, s))
        .collect();
    let start = analysis::start_clocktime(&app.net);
    let mut head = vec!["Time\n(hrs)".to_string(), "Time\nof Day".to_string()];
    head.extend(data.iter().map(|d| format!("{}\n{}", d.title, d.units)));
    let mut rows = vec![head];
    for (p, t) in hour_list(res).into_iter().enumerate() {
        let mut row = vec![fmt2(t), analysis::clock_time(start + (t * 3600.0) as u32)];
        row.extend(data.iter().map(|d| fmt2(d.values[p] as f64)));
        rows.push(row);
    }
    rows
}

/// Units of the curves on one axis, as EPANET-UI titles it ("gpm, psi").
fn axis_title(data: &[TsData], right: bool) -> String {
    let mut units: Vec<&str> = Vec::new();
    for d in data.iter().filter(|d| d.right == right) {
        if !units.contains(&d.units.as_str()) {
            units.push(&d.units);
        }
    }
    units.join(", ")
}

fn time_series(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.as_ref().unwrap();
    if app.rpt.ts_table {
        let rows = ts_rows(app, res);
        let footer = format!("{} time periods", rows.len() - 1);
        let mut sort = None;
        table(ui, "ts_table", &rows, &mut sort, &footer);
        return;
    }
    let mut data: Vec<TsData> = app
        .rpt
        .ts
        .iter()
        .filter_map(|s| ts_data(app, res, s))
        .collect();
    if data.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Select objects to plot in the Time Series Selector on the left.")
                    .weak(),
            );
        });
        return;
    }
    // With every curve on the right there is just one axis.
    if data.iter().all(|d| d.right) {
        for d in &mut data {
            d.right = false;
        }
    }
    chart_legend(
        ui,
        &data
            .iter()
            .enumerate()
            .map(|(k, d)| {
                (
                    Swatch::Line(SERIES_COLORS[k % MAX_SERIES], false),
                    d.legend.clone(),
                )
            })
            .collect::<Vec<_>>(),
    );
    // Curves on the right axis are drawn scaled into the left axis' range,
    // and the right axis labels undo the scaling.
    let range = |right: bool| {
        data.iter()
            .filter(|d| d.right == right)
            .flat_map(|d| d.values.iter().map(|&v| v as f64))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            })
    };
    let (l0, l1) = range(false);
    let (r0, r1) = range(true);
    let has_right = r0.is_finite();
    let (l0, l1) = if l1 > l0 {
        (l0, l1)
    } else {
        (l0 - 1.0, l0 + 1.0)
    };
    let (r0, r1) = if r1 > r0 {
        (r0, r1)
    } else {
        (r0 - 1.0, r0 + 1.0)
    };
    let to_left = move |v: f64| l0 + (v - r0) * (l1 - l0) / (r1 - r0);
    let to_right = move |v: f64| r0 + (v - l0) * (r1 - r0) / (l1 - l0);

    let t = hour_list(res);
    let left_title = axis_title(&data, false);
    let mut plot = base_plot("ts", app, time_axis(app), &left_title);
    if has_right {
        plot = plot.custom_y_axes(vec![
            y_axis_hint(&left_title),
            y_axis_hint(&axis_title(&data, true))
                .placement(egui_plot::HPlacement::Right)
                .formatter(move |m, _| {
                    let v = to_right(m.value);
                    let digits = if (r1 - r0).abs() < 10.0 { 2 } else { 0 };
                    format!("{v:.digits$}")
                }),
        ]);
    }
    let resp = show_chart(plot, ui, time_axis(app), |plot| {
        for (k, d) in data.iter().enumerate() {
            let pts: PlotPoints = t
                .iter()
                .zip(&d.values)
                .map(|(&x, &y)| {
                    let y = y as f64;
                    [x, if d.right { to_left(y) } else { y }]
                })
                .collect();
            plot.line(
                Line::new(d.legend.clone(), pts)
                    .stroke(Stroke::new(1.5, SERIES_COLORS[k % MAX_SERIES])),
            );
        }
        plot.pointer_coordinate()
    });
    if resp.response.clicked()
        && let Some(c) = resp.inner
    {
        go_to_hour(app, c.x);
    }
}

/// Values of the percentile parameter at period `p`: junctions with demand,
/// or pipes (EPANET-UI's selection).
fn pc_values(app: &EpanetApp, res: &SimResults, node: bool, var: usize, p: usize) -> Vec<f32> {
    let mut v = Vec::new();
    if node {
        let nv = NODE_VARS[var.min(3)];
        let d = res.node_slice(p, NodeVar::Demand);
        let x = res.node_slice(p, nv);
        for (i, n) in app.net.nodes.iter().enumerate() {
            if n.kind != NodeKind::Junction {
                continue;
            }
            if let Some(r) = app.node_result_index(i)
                && d[r] >= 0.01
            {
                v.push(x[r]);
            }
        }
    } else {
        let lv = LINK_VARS[var.min(3)];
        let x = res.link_slice(p, lv);
        for (i, l) in app.net.links.iter().enumerate() {
            if l.kind != LinkKind::Pipe {
                continue;
            }
            if let Some(r) = app.link_result_index(i) {
                v.push(if lv == LinkVar::Flow {
                    x[r].abs()
                } else {
                    x[r]
                });
            }
        }
    }
    v.retain(|x| x.is_finite());
    v.sort_by(f32::total_cmp);
    v
}

/// Per plotted time: [hours, lower, mid-low, mid-high, upper].
fn pc_data(app: &mut EpanetApp) -> Vec<[f64; 5]> {
    let Some((node, var, pc)) = app.rpt.pc_shown else {
        return Vec::new();
    };
    let key = (app.results_id, app.generation, node, var, pc);
    if let Some((k, d)) = &app.rpt.pc_cache
        && *k == key
    {
        return d.clone();
    }
    let res = app.results.as_ref().unwrap();
    let mut out = Vec::new();
    let step = if res.duration > 6 * 3600 && res.report_step > 0 {
        (3600 / res.report_step).max(1) as usize
    } else {
        1
    };
    let (lo, mid, hi) = (pc[0] as f64, pc[1] as f64, pc[2] as f64);
    let mut p = 0;
    while p < res.n_periods {
        let v = pc_values(app, res, node, var, p);
        if !v.is_empty() {
            out.push([
                res.period_time(p) as f64 / 3600.0,
                analysis::percentile(&v, lo) as f64,
                analysis::percentile(&v, 50.0 - mid / 2.0) as f64,
                analysis::percentile(&v, 50.0 + mid / 2.0) as f64,
                analysis::percentile(&v, hi) as f64,
            ]);
        }
        p += step;
    }
    app.rpt.pc_cache = Some((key, out.clone()));
    out
}

fn pc_param_label(app: &EpanetApp, node: bool, var: usize) -> (String, String) {
    let res = app.results.as_ref();
    if node {
        let v = NODE_VARS[var.min(3)];
        (
            node_var_label(v, res),
            res.map(|r| r.node_units(v)).unwrap_or_default(),
        )
    } else {
        let v = LINK_VARS[var.min(3)];
        (
            link_var_label(v, res),
            res.map(|r| r.link_units(v)).unwrap_or_default(),
        )
    }
}

fn pc_rows(app: &EpanetApp) -> Rows {
    let Some((_, _, pc)) = app.rpt.pc_shown else {
        return Vec::new();
    };
    let mut rows = vec![vec![
        "Time (hrs)".to_string(),
        format!("{}-th", pc[0]),
        format!("{}-th", 50 - pc[1] / 2),
        format!("{}-th", 50 + pc[1] / 2),
        format!("{}-th", pc[2]),
    ]];
    if let Some((_, d)) = &app.rpt.pc_cache {
        for r in d {
            rows.push(r.iter().map(|v| fmt2(*v)).collect());
        }
    }
    rows
}

fn percentiles(app: &mut EpanetApp, ui: &mut Ui) {
    let Some((node, var, pc)) = app.rpt.pc_shown else {
        return;
    };
    let data = pc_data(app);
    let res = app.results.as_ref().unwrap();
    let (name, units) = pc_param_label(app, node, var);
    chart_title(ui, &format!("Variation in System {name}"), 17.0);
    let magenta = Color32::from_rgb(0xff, 0x00, 0xff);
    let blue = Color32::from_rgb(0x00, 0x00, 0xff);
    let band = Color32::from_rgb(0xc4, 0xd6, 0xf7);
    if res.n_periods <= 1 {
        // Single period: cumulative frequency of the values.
        let v = pc_values(app, res, node, var, 0);
        let n = v.len().max(1) as f64;
        let pts: PlotPoints = v
            .iter()
            .enumerate()
            .map(|(k, &y)| [(k + 1) as f64 / n * 100.0, y as f64])
            .collect();
        let plot = base_plot("pcntile1", app, TimeAxis::None, &units).x_axis_label(
            RichText::new("Percent of Network Less Than")
                .strong()
                .size(14.0),
        );
        show_chart(plot, ui, TimeAxis::None, |plot| {
            plot.line(Line::new(name.clone(), pts).stroke(Stroke::new(2.0, blue)));
        });
        return;
    }
    chart_legend(
        ui,
        &[
            (
                Swatch::Line(magenta, true),
                format!("{}-th Percentile", pc[0]),
            ),
            (Swatch::Line(blue, true), format!("{}-th Percentile", pc[2])),
            (
                Swatch::Box(band),
                format!("{}-th to {}-th Percentile", 50 - pc[1] / 2, 50 + pc[1] / 2),
            ),
        ],
    );
    let xs: Vec<f64> = data.iter().map(|d| d[0]).collect();
    let y1: Vec<f64> = data.iter().map(|d| d[2]).collect();
    let y2: Vec<f64> = data.iter().map(|d| d[3]).collect();
    let dash = LineStyle::Dashed { length: 6.0 };
    let plot = base_plot("pcntile", app, time_axis(app), &units);
    show_chart(plot, ui, time_axis(app), |plot| {
        plot.add(
            FilledArea::new("Mid range", &xs, &y1, &y2)
                .fill_color(band)
                .stroke(Stroke::new(1.0, Color32::from_gray(120))),
        );
        let lower: PlotPoints = data.iter().map(|d| [d[0], d[1]]).collect();
        let upper: PlotPoints = data.iter().map(|d| [d[0], d[4]]).collect();
        plot.line(
            Line::new("Lower", lower)
                .stroke(Stroke::new(1.5, magenta))
                .style(dash),
        );
        plot.line(
            Line::new("Upper", upper)
                .stroke(Stroke::new(1.5, blue))
                .style(dash),
        );
    });
}

fn profile(app: &mut EpanetApp, ui: &mut Ui) {
    let Some((start, path)) = app.rpt.pf.clone() else {
        ui.label(RichText::new("Choose a start and end node in the Profile Plot Selector.").weak());
        return;
    };
    if start >= app.net.nodes.len() || path.iter().any(|&k| k >= app.net.links.len()) {
        app.rpt.pf = None;
        return;
    }
    let res = app.results.as_ref().unwrap();
    let nodes = analysis::profile_nodes(&app.net, start, &path);
    let pts: Vec<(f64, String, f64, f64)> = nodes
        .iter()
        .filter_map(|&(x, n)| {
            let r = app.node_result_index(n)?;
            let h = res.node_slice(app.period, NodeVar::Head)[r] as f64;
            let el = app.net.nodes[n].num(0).unwrap_or(h);
            Some((x, app.net.nodes[n].id.clone(), h, el))
        })
        .collect();
    if pts.len() < 2 {
        ui.label(RichText::new("The profile path has no results.").weak());
        return;
    }
    let first = &pts[0].1;
    let last = &pts[pts.len() - 1].1;
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(format!("Hydraulic Profile: Node {first} - {last}"))
                .font(crate::fonts::bold(12.0))
                .color(Color32::from_rgb(0x10, 0x10, 0xe0)),
        );
    });
    let si = res.is_si();
    let len_u = eui_core::units::length(si);
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let heads: Vec<f64> = pts.iter().map(|p| p.2).collect();
    let elevs: Vec<f64> = pts.iter().map(|p| p.3).collect();
    let lo_val = if app.rpt.pf_elev {
        elevs.iter().cloned().fold(f64::INFINITY, f64::min)
    } else {
        heads.iter().cloned().fold(f64::INFINITY, f64::min)
    };
    let hi_val = heads.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let base = lo_val - (hi_val - lo_val).max(1.0) * 0.05;
    let floor = vec![base; xs.len()];
    let water = Color32::from_rgb(0xc7, 0xd5, 0xf5);
    let ground = Color32::from_rgb(0xd9, 0xc3, 0x9a);
    let edge = Stroke::new(1.5, Color32::from_gray(150));
    let time = hhmm(res.period_time(app.period));
    let avail = ui.available_height();
    let plot = base_plot("profile", app, TimeAxis::None, len_u)
        .x_axis_label(
            RichText::new(format!("Distance ({len_u})"))
                .strong()
                .size(14.0),
        )
        .height(avail - 26.0);
    show_chart(plot, ui, TimeAxis::None, |plot| {
        if app.rpt.pf_elev {
            plot.add(
                FilledArea::new("Elevation", &xs, &floor, &elevs)
                    .fill_color(ground)
                    .stroke(edge),
            );
            plot.add(
                FilledArea::new("Head", &xs, &elevs, &heads)
                    .fill_color(water)
                    .stroke(edge),
            );
        } else {
            plot.add(
                FilledArea::new("Head", &xs, &floor, &heads)
                    .fill_color(water)
                    .stroke(edge),
            );
        }
        if app.rpt.pf_labels {
            for (x, id, h, _) in &pts {
                plot.text(
                    Text::new(
                        id.clone(),
                        egui_plot::PlotPoint::new(*x, *h),
                        RichText::new(id).size(14.0),
                    )
                    .color(Color32::from_gray(110))
                    .anchor(Align2::CENTER_BOTTOM),
                );
            }
        }
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(
            RichText::new(format!("{time} hrs"))
                .font(crate::fonts::bold(17.0))
                .color(Color32::from_rgb(0x10, 0x10, 0xe0)),
        );
    });
}

// ----- selectors ----------------------------------------------------------------------------

/// Background of the selector panels (EPANET-UI's hint color).
pub const SELECTOR_BG: Color32 = Color32::from_rgb(0xff, 0xfb, 0xef);

/// Draws the active report selector at the top of the left panel.
pub fn selector(app: &mut EpanetApp, ui: &mut Ui) {
    let title = match app.rpt.selector {
        Selector::None => return,
        Selector::TimeSeries => "Time Series Selector",
        Selector::Percentiles => "Percentiles Plot Selector",
        Selector::Profile => "Profile Plot Selector",
        Selector::FireFlow => "Fire Flow Selector",
    };
    let pal = app.palette();
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().rect_filled(bar, 0.0, pal.menu);
    ui.painter().text(
        bar.center(),
        Align2::CENTER_CENTER,
        title,
        crate::fonts::bold(14.0),
        Color32::WHITE,
    );
    let close_rect = Rect::from_center_size(bar.right_center() - vec2(18.0, 0.0), vec2(26.0, 26.0));
    let x = ui.interact(close_rect, ui.id().with("sel_close"), Sense::click());
    white_button(ui, close_rect, x.hovered());
    ui.painter().text(
        close_rect.center(),
        Align2::CENTER_CENTER,
        "×",
        FontId::proportional(18.0),
        Color32::BLACK,
    );
    if x.clicked() {
        if app.rpt.selector == Selector::TimeSeries {
            ts_cancel(app);
        } else {
            app.rpt.selector = Selector::None;
        }
        return;
    }
    let bg = if ui.visuals().dark_mode {
        Color32::from_rgb(0x3a, 0x38, 0x2a)
    } else {
        SELECTOR_BG
    };
    egui::Frame::NONE
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            match app.rpt.selector {
                Selector::TimeSeries => ts_selector(app, ui),
                Selector::Percentiles => pc_selector(app, ui),
                Selector::Profile => pf_selector(app, ui),
                Selector::FireFlow => crate::fireflow::selector(app, ui),
                Selector::None => {}
            }
        });
}

/// A text box for an object ID with a "+" button that takes the object
/// selected on the map.
fn id_entry(app: &EpanetApp, ui: &mut Ui, text: &mut String, node: bool) {
    ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width() - 40.0));
    let pick = match (app.selection, node) {
        (Some(Element::Node(i)), true) => Some(app.net.nodes[i].id.clone()),
        (Some(Element::Link(i)), false) => Some(app.net.links[i].id.clone()),
        _ => None,
    };
    if ui
        .add_enabled(pick.is_some(), egui::Button::new("+"))
        .on_hover_text(if node {
            "Use the node selected on the map"
        } else {
            "Use the link selected on the map"
        })
        .clicked()
        && let Some(id) = pick
    {
        *text = id;
    }
}

fn var_combo(
    ui: &mut Ui,
    id: &str,
    node: bool,
    var: &mut usize,
    res: Option<&SimResults>,
    max: usize,
) {
    let names: Vec<String> = if node {
        NODE_VARS.iter().map(|v| node_var_label(*v, res)).collect()
    } else {
        LINK_VARS
            .iter()
            .take(max)
            .map(|v| link_var_label(*v, res))
            .collect()
    };
    *var = (*var).min(names.len() - 1);
    egui::ComboBox::from_id_salt(id)
        .selected_text(names[*var].clone())
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            for (k, n) in names.iter().enumerate() {
                ui.selectable_value(var, k, n);
            }
        });
}

/// The parameter a new series starts with: the map theme's when it is a
/// result, else pressure (nodes) or flow (links), as EPANET-UI does.
fn ts_default_var(app: &EpanetApp, node: bool) -> usize {
    use crate::style::{LinkTheme, NodeTheme};
    if node {
        match app.prefs.node_theme {
            NodeTheme::Demand => 0,
            NodeTheme::Head => 1,
            NodeTheme::Quality => 3,
            _ => 2,
        }
    } else {
        match app.prefs.link_theme {
            LinkTheme::Velocity => 1,
            LinkTheme::Headloss => 2,
            LinkTheme::Quality => 3,
            _ => 0,
        }
    }
}

/// "Pipe 112 Flow Rate": the object type, ID and parameter (EPANET-UI's
/// series title); empty if there is no such object.
fn ts_title(app: &EpanetApp, s: &TsSeries) -> String {
    let res = app.results.as_ref();
    let id = s.id.trim();
    if s.node {
        let Some(i) = app.net.node_index(id) else {
            return String::new();
        };
        let v = node_var_label(NODE_VARS[s.var.min(NODE_VARS.len() - 1)], res);
        format!("{} {id} {v}", app.net.nodes[i].kind.label())
    } else {
        let Some(i) = app.net.link_index(id) else {
            return String::new();
        };
        let v = link_var_label(LINK_VARS[s.var.min(LINK_VARS.len() - 1)], res);
        format!("{} {id} {v}", app.net.links[i].kind.label())
    }
}

/// A new series for the object selected on the map, if any.
fn ts_for_selection(app: &EpanetApp) -> Option<TsSeries> {
    let (node, id) = match app.selection? {
        Element::Node(i) => (true, app.net.nodes[i].id.clone()),
        Element::Link(i) => (false, app.net.links[i].id.clone()),
    };
    let mut s = TsSeries {
        node,
        id,
        var: ts_default_var(app, node),
        legend: String::new(),
        right: false,
    };
    s.legend = ts_title(app, &s);
    Some(s)
}

/// Leaves the Time Series Selector without applying its changes.
/// Closes the open selector panel as its close button does.
pub fn close_selector(app: &mut EpanetApp) {
    if app.rpt.selector == Selector::TimeSeries {
        ts_cancel(app);
    } else {
        app.rpt.selector = Selector::None;
    }
}

fn ts_cancel(app: &mut EpanetApp) {
    let was_empty = app.rpt.ts_draft.as_ref().is_some_and(|d| d.was_empty);
    app.rpt.selector = Selector::None;
    app.rpt.ts_draft = None;
    if was_empty || app.rpt.ts.is_empty() {
        app.view_mode = ViewMode::Map;
    } else {
        app.view_mode = ViewMode::Report;
    }
}

/// A square tool button of the series list (EPANET-UI's bit buttons).
fn ts_tool(
    ui: &mut Ui,
    enabled: bool,
    tip: &str,
    draw: impl Fn(&egui::Painter, egui::Pos2, Stroke),
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(40.0, 34.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let v = ui.visuals();
    let fill = if enabled && resp.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        v.widgets.inactive.weak_bg_fill
    };
    ui.painter().rect(
        rect,
        2.0,
        fill,
        v.widgets.inactive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let fg = if enabled {
        v.text_color()
    } else {
        v.weak_text_color().gamma_multiply(0.6)
    };
    draw(ui.painter(), rect.center(), Stroke::new(1.6, fg));
    resp.on_hover_text(tip).clicked()
}

fn ts_selector(app: &mut EpanetApp, ui: &mut Ui) {
    if app.rpt.ts_draft.is_none() {
        let mut series = app.rpt.ts.clone();
        let was_empty = series.is_empty();
        if was_empty && let Some(s) = ts_for_selection(app) {
            series.push(s);
        }
        app.rpt.ts_draft = Some(TsDraft {
            series,
            selected: 0,
            time_of_day: app.rpt.time_of_day,
            edit: None,
            was_empty,
        });
    }
    if app.rpt.ts_draft.as_ref().is_some_and(|d| d.edit.is_some()) {
        ts_edit_page(app, ui);
    } else {
        ts_list_page(app, ui);
    }
}

/// The list of series, with buttons to add, edit, delete and reorder them.
fn ts_list_page(app: &mut EpanetApp, ui: &mut Ui) {
    let titles: Vec<String> = app
        .rpt
        .ts_draft
        .as_ref()
        .map(|d| d.series.iter().map(|s| ts_title(app, s)).collect())
        .unwrap_or_default();
    let d = app.rpt.ts_draft.as_mut().unwrap();
    let n = d.series.len();
    d.selected = d.selected.min(n.saturating_sub(1));
    ui.label("Add/edit time series to view:");
    ui.add_space(6.0);
    let (mut add, mut edit) = (false, false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        add = ts_tool(ui, n < MAX_SERIES, "Add a series", |p, c, s| {
            p.line_segment([c + vec2(-7.0, 0.0), c + vec2(7.0, 0.0)], s);
            p.line_segment([c + vec2(0.0, -7.0), c + vec2(0.0, 7.0)], s);
        });
        edit = ts_tool(ui, n > 0, "Edit the selected series", |p, c, s| {
            crate::icons::paint(p, crate::icons::Icon::Pencil, c, 18.0, s.color);
        });
        if ts_tool(ui, n > 0, "Delete the selected series", |p, c, s| {
            p.line_segment([c + vec2(-6.0, -6.0), c + vec2(6.0, 6.0)], s);
            p.line_segment([c + vec2(6.0, -6.0), c + vec2(-6.0, 6.0)], s);
        }) {
            d.series.remove(d.selected);
            d.selected = d.selected.min(d.series.len().saturating_sub(1));
        }
        let arrow = |up: f32| {
            move |p: &egui::Painter, c: egui::Pos2, s: Stroke| {
                p.line_segment([c + vec2(0.0, -7.0 * up), c + vec2(0.0, 7.0 * up)], s);
                p.line_segment([c + vec2(0.0, -7.0 * up), c + vec2(-5.0, -2.0 * up)], s);
                p.line_segment([c + vec2(0.0, -7.0 * up), c + vec2(5.0, -2.0 * up)], s);
            }
        };
        if ts_tool(ui, n > 1, "Move up", arrow(1.0)) && d.selected > 0 {
            d.series.swap(d.selected, d.selected - 1);
            d.selected -= 1;
        }
        if ts_tool(ui, n > 1, "Move down", arrow(-1.0)) && d.selected + 1 < n {
            d.series.swap(d.selected, d.selected + 1);
            d.selected += 1;
        }
    });
    ui.add_space(4.0);
    // the list box
    let white = if ui.visuals().dark_mode {
        ui.visuals().extreme_bg_color
    } else {
        Color32::WHITE
    };
    egui::Frame::NONE
        .fill(white)
        .stroke(Stroke::new(1.0, Color32::from_gray(150)))
        .inner_margin(2)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(180.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for (k, t) in titles.iter().enumerate() {
                let (r, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                let sel = k == d.selected;
                if sel {
                    ui.painter()
                        .rect_filled(r, 0.0, Color32::from_rgb(0x00, 0x78, 0xd7));
                }
                ui.painter().text(
                    r.left_center() + vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                    if t.is_empty() { "(missing object)" } else { t },
                    FontId::proportional(13.5),
                    if sel {
                        Color32::WHITE
                    } else {
                        ui.visuals().text_color()
                    },
                );
                if resp.clicked() {
                    d.selected = k;
                }
                if resp.double_clicked() {
                    edit = true;
                }
            }
        });
    ui.add_space(8.0);
    crate::widgets::checkbox(ui, &mut d.time_of_day, "Plot Time Of Day");
    ui.add_space(8.0);
    let (mut view, mut cancel) = (false, false);
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - 2.0 * 84.0 - 12.0) / 2.0).max(0.0));
        ui.spacing_mut().item_spacing.x = 12.0;
        view = ui
            .add_sized([84.0, 28.0], egui::Button::new("View"))
            .clicked();
        cancel = ui
            .add_sized([84.0, 28.0], egui::Button::new("Cancel"))
            .clicked();
    });
    if add {
        let node = !matches!(app.selection, Some(Element::Link(_)));
        let mut series = TsSeries {
            node,
            id: String::new(),
            var: ts_default_var(app, node),
            legend: String::new(),
            right: false,
        };
        if let Some(s) = ts_for_selection(app) {
            series = s;
        }
        let d = app.rpt.ts_draft.as_mut().unwrap();
        d.edit = Some(TsEdit {
            index: None,
            series,
            seen: app.selection,
        });
    } else if edit
        && let Some(d) = app.rpt.ts_draft.as_mut()
        && let Some(s) = d.series.get(d.selected)
    {
        d.edit = Some(TsEdit {
            index: Some(d.selected),
            series: s.clone(),
            seen: app.selection,
        });
    }
    if view {
        let d = app.rpt.ts_draft.take().unwrap();
        app.rpt.ts = d.series;
        app.rpt.time_of_day = d.time_of_day;
        app.rpt.selector = Selector::None;
        app.report_kind = ReportKind::TimeSeries;
        app.view_mode = if app.rpt.ts.is_empty() {
            ViewMode::Map
        } else {
            ViewMode::Report
        };
    } else if cancel {
        ts_cancel(app);
    }
}

/// The form for one series: object, parameter, legend text and y-axis.
fn ts_edit_page(app: &mut EpanetApp, ui: &mut Ui) {
    let res = app.results.clone();
    let selection = app.selection;
    let pick = selection.map(|e| match e {
        Element::Node(i) => (true, app.net.nodes[i].id.clone()),
        Element::Link(i) => (false, app.net.links[i].id.clone()),
    });
    let mut e = app
        .rpt
        .ts_draft
        .as_mut()
        .and_then(|d| d.edit.take())
        .unwrap();
    let before = e.series.clone();
    // Clicking an object on the map fills in the form (EPANET-UI's SetSelectedObjectProps).
    if selection != e.seen {
        e.seen = selection;
        if let Some((node, id)) = pick {
            if node != e.series.node {
                e.series.node = node;
                e.series.var = ts_default_var(app, node);
            }
            e.series.id = id;
        }
    }
    ui.label("Select an object to plot:");
    ui.add_space(6.0);
    let mut accept = false;
    let mut cancel = false;
    egui::Grid::new("ts_edit")
        .num_columns(2)
        .spacing([12.0, 12.0])
        .show(ui, |ui| {
            let w = ui.available_width().max(150.0);
            ui.label("Object Type");
            egui::ComboBox::from_id_salt("ts_type")
                .selected_text(if e.series.node { "Node" } else { "Link" })
                .width(w)
                .show_ui(ui, |ui| {
                    for (node, name) in [(true, "Node"), (false, "Link")] {
                        if ui.selectable_label(e.series.node == node, name).clicked()
                            && e.series.node != node
                        {
                            e.series.node = node;
                            e.series.id.clear();
                            e.series.var = ts_default_var(app, node);
                        }
                    }
                });
            ui.end_row();
            ui.label("Object ID");
            ui.add(egui::TextEdit::singleline(&mut e.series.id).desired_width(w));
            ui.end_row();
            ui.label("Parameter");
            let mut var = e.series.var;
            var_combo(
                ui,
                "ts_param",
                e.series.node,
                &mut var,
                res.as_ref(),
                LINK_VARS.len(),
            );
            e.series.var = var;
            ui.end_row();
            ui.label("Legend");
            ui.add(egui::TextEdit::singleline(&mut e.series.legend).desired_width(w));
            ui.end_row();
            ui.label("Y-Axis");
            ui.horizontal(|ui| {
                crate::widgets::radio_value(ui, &mut e.series.right, false, "Left");
                ui.add_space(16.0);
                crate::widgets::radio_value(ui, &mut e.series.right, true, "Right");
            });
            ui.end_row();
        });
    // The legend follows the object and parameter until they change no more.
    if (e.series.node, &e.series.id, e.series.var) != (before.node, &before.id, before.var) {
        e.series.legend = ts_title(app, &e.series);
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - 2.0 * 84.0 - 12.0) / 2.0).max(0.0));
        ui.spacing_mut().item_spacing.x = 12.0;
        accept = ui
            .add_sized([84.0, 28.0], egui::Button::new("Accept"))
            .clicked();
        cancel = ui
            .add_sized([84.0, 28.0], egui::Button::new("Cancel"))
            .clicked();
    });
    let exists = !ts_title(app, &e.series).is_empty();
    if accept && !exists {
        app.status = format!(
            "There is no {} \"{}\"",
            if e.series.node { "node" } else { "link" },
            e.series.id.trim()
        );
        accept = false;
    }
    if accept {
        e.series.id = e.series.id.trim().to_string();
        if e.series.legend.trim().is_empty() {
            e.series.legend = ts_title(app, &e.series);
        }
    }
    let d = app.rpt.ts_draft.as_mut().unwrap();
    if accept {
        match e.index {
            Some(k) if k < d.series.len() => {
                d.series[k] = e.series;
                d.selected = k;
            }
            _ => {
                d.series.push(e.series);
                d.selected = d.series.len() - 1;
            }
        }
    } else if !cancel {
        d.edit = Some(e);
    }
}

fn pc_selector(app: &mut EpanetApp, ui: &mut Ui) {
    ui.label("Select a parameter to plot:");
    ui.horizontal(|ui| {
        if crate::widgets::radio(ui, app.rpt.pc_node, "Node").clicked() {
            app.rpt.pc_node = true;
            app.rpt.pc_var = 2;
        }
        ui.add_space(20.0);
        if crate::widgets::radio(ui, !app.rpt.pc_node, "Link").clicked() {
            app.rpt.pc_node = false;
            app.rpt.pc_var = 0;
        }
    });
    let mut var = app.rpt.pc_var;
    var_combo(
        ui,
        "pc_var",
        app.rpt.pc_node,
        &mut var,
        app.results.as_ref(),
        4,
    );
    app.rpt.pc_var = var;
    ui.add_space(8.0);
    ui.label("Select which percentiles to plot:");
    egui::Grid::new("pc_grid")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            for (k, name) in [
                "Lower Percentile",
                "Mid Percentile Range",
                "Upper Percentile",
            ]
            .iter()
            .enumerate()
            {
                ui.label(*name);
                let range = match k {
                    0 => 0..=49,
                    1 => 2..=98,
                    _ => 51..=100,
                };
                ui.add(egui::DragValue::new(&mut app.rpt.pc[k]).range(range));
                ui.end_row();
            }
        });
    app.rpt.pc[1] -= app.rpt.pc[1] % 2;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        crate::widgets::checkbox(ui, &mut app.rpt.time_of_day, "Plot Time Of Day");
        if ui.button("   View   ").clicked() {
            app.rpt.pc_shown = Some((app.rpt.pc_node, app.rpt.pc_var, app.rpt.pc));
            app.report_kind = ReportKind::Percentiles;
            app.view_mode = ViewMode::Report;
        }
    });
}

fn pf_selector(app: &mut EpanetApp, ui: &mut Ui) {
    egui::Grid::new("pf_grid")
        .num_columns(2)
        .spacing([10.0, 10.0])
        .show(ui, |ui| {
            ui.label("Start Node");
            let mut s = std::mem::take(&mut app.rpt.pf_start);
            ui.horizontal(|ui| id_entry(app, ui, &mut s, true));
            app.rpt.pf_start = s;
            ui.end_row();
            ui.label("End Node");
            let mut e = std::mem::take(&mut app.rpt.pf_end);
            ui.horizontal(|ui| id_entry(app, ui, &mut e, true));
            app.rpt.pf_end = e;
            ui.end_row();
        });
    ui.label(
        RichText::new("Select a node on the map, then press + to use it.")
            .small()
            .weak(),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("     View     ").clicked() {
            let a = app.net.node_index(app.rpt.pf_start.trim());
            let b = app.net.node_index(app.rpt.pf_end.trim());
            match (a, b) {
                (Some(a), Some(b)) => match analysis::profile_path(&app.net, a, b) {
                    Some(path) if !path.is_empty() => {
                        app.rpt.pf = Some((a, path));
                        app.report_kind = ReportKind::Profile;
                        app.view_mode = ViewMode::Report;
                    }
                    _ => app.status = "No path connects these two nodes".into(),
                },
                _ => app.status = "Enter the IDs of two existing nodes".into(),
            }
        }
        if ui.button("    Cancel    ").clicked() {
            app.rpt.selector = Selector::None;
        }
    });
}
