//! Fire flow analysis (EPANET-UI's `fireflowcalc` / `fireflowrpt`).
//!
//! For each selected junction it finds the largest fire flow that keeps the
//! pressure at that node, or in a chosen pressure zone, above a target.
//! Every trial flow is one run of the engine on a copy of the network that
//! adds the fire demand at the time of day of the fire. The runs are chained
//! from the UI loop, so the analysis works the same on the desktop and in the
//! browser.

use std::collections::BTreeSet;

use egui::{Color32, FontId, Pos2, Rect, RichText, Stroke, Ui};
use eui_core::analysis::{self, FLOW_UCF, PRESSURE_UCF};
use eui_core::props::{Element, PropKey};
use eui_core::{Network, NodeKind, NodeVar, SimResults};
use eui_engine::RunHandle;

use crate::app::{EpanetApp, ReportKind, Tool, ViewMode};
use crate::i18n::{tr, trf};
use crate::reports::{Rows, Selector};

/// Which nodes must stay above the target pressure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// Only the node receiving the fire flow.
    Individual,
    /// All the nodes selected for fire flow analysis.
    FireSet,
    /// Every junction in the network.
    All,
}

impl Zone {
    fn label(self) -> &'static str {
        match self {
            Zone::Individual => "Individual fire flow nodes",
            Zone::FireSet => "All fire flow nodes",
            Zone::All => "All network nodes",
        }
    }
}

/// Result for one fire flow node, in fire flow units (gpm & psi, or lpm & kPa).
#[derive(Clone, Debug)]
pub struct FfResult {
    pub node: String,
    pub static_p: f64,
    pub target_q: f64,
    pub residual_p: f64,
    pub available_q: f64,
    pub critical_p: f64,
    pub critical_node: String,
}

/// Settings and outputs, used for the report.
#[derive(Clone, Debug)]
struct Design {
    flow: f64,
    pressure: f64,
    hour: u32,
    zone: Zone,
    nodes: usize,
}

pub struct FireFlowState {
    pub target_flow: String,
    pub target_pressure: String,
    /// Hour of the day of the fire (0-23).
    pub hour: u32,
    pub nodes: Vec<String>,
    pub zone: Zone,
    pub tab: u8,
    /// The Map tab's zoom (1 = the whole network) and its center (world
    /// coordinates; none: the network's center).
    pub map_zoom: f64,
    pub map_center: Option<[f64; 2]>,
    pub results: Vec<FfResult>,
    pub log: Vec<String>,
    /// The Group Selector, while the fire flow nodes are picked on the map.
    pub group: Option<GroupSel>,
    design: Option<Design>,
    job: Option<Job>,
}

/// EPANET-UI's Group Selector: junctions toggled on the map, or picked by
/// Tag or by a polygon drawn on the map.
#[derive(Default)]
pub struct GroupSel {
    nodes: BTreeSet<usize>,
    tag: String,
    /// The corners (world coordinates) of the polygon being drawn.
    polygon: Option<Vec<[f64; 2]>>,
}

impl Default for FireFlowState {
    fn default() -> Self {
        FireFlowState {
            target_flow: String::new(),
            target_pressure: String::new(),
            hour: 0,
            nodes: Vec::new(),
            zone: Zone::Individual,
            tab: 0,
            map_zoom: 1.0,
            map_center: None,
            results: Vec::new(),
            log: Vec::new(),
            group: None,
            design: None,
            job: None,
        }
    }
}

enum Phase {
    Static,
    Design,
    Search {
        lo: f64,
        f_lo: f64,
        hi: f64,
        f_hi: f64,
        q: f64,
        iter: u32,
        side: i8,
    },
}

/// Pressures of one trial run, by network node index.
struct Probe {
    pressure: Vec<Option<f64>>,
}

struct Job {
    base: Network,
    fire_time: u32,
    /// Project flow units per fire flow unit, project pressure units per fire pressure unit.
    qcf: f64,
    pcf: f64,
    design_q: f64,
    design_p: f64,
    zone: Zone,
    fire_nodes: Vec<usize>,
    statics: Vec<Option<f64>>,
    k: usize,
    phase: Phase,
    run: Option<RunHandle>,
    /// (residual pressure at design flow, best (q, critical p, critical node))
    residual: f64,
    best: Option<(f64, f64, usize)>,
}

/// Fire flow units for the project: (flow label, pressure label, flow units per
/// cfs index, pressure units per psi index).
fn fire_units(net: &Network) -> (&'static str, &'static str, usize, usize) {
    if net.is_si() {
        ("lpm", "kPa", 6, 1)
    } else {
        ("gpm", "psi", 1, 0)
    }
}

pub fn selector(app: &mut EpanetApp, ui: &mut Ui) {
    if app.rpt.ff.group.is_some() {
        group_selector(app, ui);
        return;
    }
    let (qu, pu, ..) = fire_units(&app.net);
    let mut open_group = false;
    let running = app.rpt.ff.job.is_some();
    let ff = &mut app.rpt.ff;
    if ff.target_flow.is_empty() {
        ff.target_flow = if app.net.is_si() { "2000" } else { "500" }.into();
        ff.target_pressure = if app.net.is_si() { "140" } else { "20" }.into();
    }
    ui.add_enabled_ui(!running, |ui| {
        egui::Grid::new("ff_grid")
            .num_columns(3)
            .spacing([10.0, 12.0])
            .show(ui, |ui| {
                ui.label("Target Fire Flow");
                ui.add(egui::TextEdit::singleline(&mut ff.target_flow).desired_width(80.0));
                ui.label(qu);
                ui.end_row();
                ui.label("Target Pressure");
                ui.add(egui::TextEdit::singleline(&mut ff.target_pressure).desired_width(80.0));
                ui.label(pu);
                ui.end_row();
                ui.label("Time of Day");
                egui::ComboBox::from_id_salt("ff_hour")
                    .selected_text(analysis::clock_time(ff.hour * 3600))
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for h in 0..24 {
                            ui.selectable_value(&mut ff.hour, h, analysis::clock_time(h * 3600));
                        }
                    });
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            open_group = ui.button(tr("Select Nodes")).clicked();
            ui.label(trf("{} selected", &[&ff.nodes.len()]));
        });
        ui.add_space(8.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Pressure Zone Set");
            for z in [Zone::Individual, Zone::FireSet, Zone::All] {
                crate::widgets::radio_value(ui, &mut ff.zone, z, z.label());
            }
        });
    });
    if open_group {
        let nodes = app
            .rpt
            .ff
            .nodes
            .iter()
            .filter_map(|id| app.net.node_index(id))
            .collect();
        app.rpt.ff.group = Some(GroupSel {
            nodes,
            ..Default::default()
        });
        app.set_tool(Tool::Select);
        app.view_mode = ViewMode::Map;
        return;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Help").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(
                "https://github.com/OpenWaterAnalytics/EPANET-UI/blob/main/manual/fireflow.md",
            ));
        }
        if running {
            if ui.button("Cancel").clicked() {
                app.rpt.ff.job = None;
                app.status = "Fire flow analysis cancelled".into();
            }
        } else if ui.button("Compute Fire Flows").clicked() {
            start(app, ui.ctx());
        }
    });
    if let Some(job) = &app.rpt.ff.job {
        ui.add_space(6.0);
        let n = job.fire_nodes.len().max(1);
        ui.add(
            egui::ProgressBar::new(job.k as f32 / n as f32).text(format!(
                "Node {} of {}",
                (job.k + 1).min(n),
                n
            )),
        );
    }
}

/// The Group Selector panel, in place of the Fire Flow Selector's settings.
fn group_selector(app: &mut EpanetApp, ui: &mut Ui) {
    let net = &app.net;
    let Some(g) = app.rpt.ff.group.as_mut() else {
        return;
    };
    let junctions = || {
        net.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == NodeKind::Junction)
            .map(|(i, _)| i)
    };
    let w = ui.available_width();
    let wide = |ui: &mut Ui, text: &str| {
        ui.add_sized([w, 26.0], egui::Button::new(tr(text)))
            .clicked()
    };
    ui.label(tr(
        "Click nodes on the map to toggle their selection on or off.",
    ));
    ui.add_space(10.0);
    if wide(ui, "Select nodes with Tag") {
        let tag = g.tag.trim();
        if tag.is_empty() {
            app.status = tr("Enter the Tag of the nodes to select").into();
        } else {
            let before = g.nodes.len();
            g.nodes.extend(junctions().filter(|&i| {
                net.get_prop(Element::Node(i), PropKey::Tag)
                    .trim()
                    .eq_ignore_ascii_case(tag)
            }));
            app.status = trf(
                "{} nodes added with Tag {}",
                &[&(g.nodes.len() - before), &tag],
            );
        }
    }
    ui.add(egui::TextEdit::singleline(&mut g.tag).desired_width(w));
    ui.add_space(10.0);
    let drawing = g.polygon.is_some();
    let label = if drawing {
        "Cancel the polygon"
    } else {
        "Select a polygon region"
    };
    if wide(ui, label) {
        g.polygon = if drawing { None } else { Some(Vec::new()) };
    }
    if g.polygon.is_some() {
        ui.label(
            RichText::new(
                tr("Click the corners of the region on the map; right-click, or click the first corner, to close it."),
            )
            .small(),
        );
    }
    ui.add_space(10.0);
    if wide(ui, "Select all nodes") {
        g.nodes.extend(junctions());
    }
    ui.add_space(10.0);
    if wide(ui, "Deselect all nodes") {
        g.nodes.clear();
    }
    ui.add_space(10.0);
    ui.label(trf("{} nodes selected", &[&g.nodes.len()]));
    ui.add_space(10.0);
    let (mut cancel, mut accept) = (false, false);
    ui.columns(2, |cols| {
        cancel = cols[0].button(tr("Cancel")).clicked();
        cols[1].with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            accept = ui.button(tr("Accept")).clicked();
        });
    });
    if accept {
        app.rpt.ff.nodes = g.nodes.iter().map(|&i| net.nodes[i].id.clone()).collect();
    }
    if accept || cancel {
        app.rpt.ff.group = None;
    }
}

/// The Group Selector is open (on top of the Fire Flow Selector).
pub fn grouping(app: &EpanetApp) -> bool {
    app.rpt.selector == Selector::FireFlow && app.rpt.ff.group.is_some()
}

/// Esc: drops the polygon being drawn, or else cancels the Group Selector.
/// False when the Group Selector isn't open.
pub fn cancel_group(app: &mut EpanetApp) -> bool {
    let Some(g) = app.rpt.ff.group.as_mut() else {
        return false;
    };
    if g.polygon.take().is_none() {
        app.rpt.ff.group = None;
    }
    true
}

/// Clicks on the map while the Group Selector is open: a click toggles a
/// junction, or adds a corner to the polygon being drawn, which a right-click
/// or a click on its first corner closes. True when the click was taken.
pub fn map_input(app: &mut EpanetApp, rect: Rect, response: &egui::Response) -> bool {
    if !grouping(app) {
        return false;
    }
    let hit = response
        .interact_pointer_pos()
        .and_then(|p| crate::map::hit_node(app, rect, p));
    let view = &app.view;
    let net = &app.net;
    let Some(g) = app.rpt.ff.group.as_mut() else {
        return false;
    };
    if let Some(poly) = &mut g.polygon {
        if response.hovered() {
            response.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let Some(p) = response.interact_pointer_pos() else {
            return false;
        };
        let near_first = poly
            .first()
            .is_some_and(|&f| view.to_screen(rect, f).distance(p) < 8.0);
        if poly.len() >= 3 && (response.secondary_clicked() || (response.clicked() && near_first)) {
            let before = g.nodes.len();
            g.nodes
                .extend(net.nodes.iter().enumerate().filter_map(|(i, n)| {
                    (n.kind == NodeKind::Junction && n.pos.is_some_and(|w| inside(poly, w)))
                        .then_some(i)
                }));
            app.status = trf(
                "{} nodes added from the region",
                &[&(g.nodes.len() - before)],
            );
            g.polygon = None;
            return true;
        }
        if response.clicked() {
            poly.push(view.to_world(rect, p));
            return true;
        }
        return response.secondary_clicked();
    }
    if !response.clicked() {
        return false;
    }
    match hit {
        Some(i) if net.nodes[i].kind == NodeKind::Junction => {
            if !g.nodes.remove(&i) {
                g.nodes.insert(i);
            }
        }
        Some(_) => app.status = tr("Only junctions can be fire flow nodes").into(),
        None => {}
    }
    true
}

/// Marks the Group Selector's nodes on the map, and the polygon being drawn.
pub fn draw_group(app: &EpanetApp, painter: &egui::Painter, rect: Rect, hover: Option<Pos2>) {
    if !grouping(app) {
        return;
    }
    let Some(g) = &app.rpt.ff.group else { return };
    let r = app.prefs.node_size + 2.0;
    for &i in &g.nodes {
        if let Some(w) = app.net.nodes.get(i).and_then(|n| n.pos) {
            let s = app.view.to_screen(rect, w);
            if rect.expand(10.0).contains(s) {
                painter.circle(
                    s,
                    r,
                    crate::style::SELECTED,
                    Stroke::new(1.0, Color32::BLACK),
                );
            }
        }
    }
    if let Some(poly) = &g.polygon {
        let stroke = Stroke::new(1.5, crate::style::ACCENT);
        let mut pts: Vec<Pos2> = poly.iter().map(|&w| app.view.to_screen(rect, w)).collect();
        for &s in &pts {
            painter.rect_filled(
                Rect::from_center_size(s, egui::vec2(6.0, 6.0)),
                0.0,
                stroke.color,
            );
        }
        if let Some(h) = hover
            && !pts.is_empty()
        {
            pts.push(h);
        }
        if pts.len() >= 3 {
            painter.add(egui::Shape::dashed_line(
                &[pts[pts.len() - 1], pts[0]],
                stroke,
                6.0,
                4.0,
            ));
        }
        painter.add(egui::Shape::line(pts, stroke));
    }
}

/// Is a point inside a polygon (even-odd rule)?
fn inside(poly: &[[f64; 2]], [x, y]: [f64; 2]) -> bool {
    let mut odd = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ([xi, yi], [xj, yj]) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < xi + (y - yi) / (yj - yi) * (xj - xi) {
            odd = !odd;
        }
        j = i;
    }
    odd
}

fn start(app: &mut EpanetApp, ctx: &egui::Context) {
    let (_, _, qi, pi) = fire_units(&app.net);
    let ff = &app.rpt.ff;
    let (Some(q), Some(p)) = (
        eui_core::num::parse(&ff.target_flow),
        eui_core::num::parse(&ff.target_pressure),
    ) else {
        app.status = "Enter numbers for the target fire flow and pressure".into();
        return;
    };
    let fire_nodes: Vec<usize> = ff
        .nodes
        .iter()
        .filter_map(|id| app.net.node_index(id))
        .filter(|&i| app.net.nodes[i].kind == NodeKind::Junction)
        .collect();
    if fire_nodes.is_empty() {
        app.status = "Select the junctions to analyze first (Select Nodes)".into();
        return;
    }
    let flow_idx = analysis::flow_units_index(&app.net.flow_units());
    let p_idx = analysis::pressure_units_index(&app.net);
    let qcf = FLOW_UCF[flow_idx] / FLOW_UCF[qi];
    let pcf = PRESSURE_UCF[p_idx] / PRESSURE_UCF[pi];
    let start_clock = analysis::start_clocktime(&app.net);
    let fire_time = (ff.hour * 3600 + 86_400 - start_clock) % 86_400;
    let design = Design {
        flow: q,
        pressure: p,
        hour: ff.hour,
        zone: ff.zone,
        nodes: fire_nodes.len(),
    };
    let mut job = Job {
        base: app.net.clone(),
        fire_time,
        qcf,
        pcf,
        design_q: q * qcf,
        design_p: p * pcf,
        zone: ff.zone,
        fire_nodes,
        statics: Vec::new(),
        k: 0,
        phase: Phase::Static,
        run: None,
        residual: 0.0,
        best: None,
    };
    spawn(&mut job, None, 0.0, ctx);
    let ff = &mut app.rpt.ff;
    ff.results.clear();
    ff.log.clear();
    ff.design = Some(design);
    ff.job = Some(job);
    ff.tab = 0;
    ff.map_zoom = 1.0;
    ff.map_center = None;
    app.report_kind = ReportKind::FireFlow;
    app.view_mode = ViewMode::Report;
    app.status = "Computing fire flows…".into();
}

fn spawn(job: &mut Job, node: Option<usize>, q: f64, ctx: &egui::Context) {
    let net = analysis::fire_flow_network(&job.base, node, q, job.fire_time);
    let ctx = ctx.clone();
    job.run = Some(eui_engine::spawn_run(net.to_inp(), None, move || {
        ctx.request_repaint()
    }));
}

fn read_probe(net: &Network, out: &[u8]) -> Option<Probe> {
    let res = SimResults::from_out(out).ok()?;
    let ix = analysis::ResultIndex::new(net, &res);
    let p = res.node_slice(res.n_periods - 1, NodeVar::Pressure);
    Some(Probe {
        pressure: ix.nodes.iter().map(|r| r.map(|r| p[r] as f64)).collect(),
    })
}

/// Lowest pressure in the pressure zone (and the node where it occurs).
fn critical(job: &Job, probe: &Probe, fire: usize) -> (f64, f64, usize) {
    let residual = probe.pressure[fire].unwrap_or(f64::NAN);
    let (mut crit, mut node) = (residual, fire);
    let zone: Vec<usize> = match job.zone {
        Zone::Individual => Vec::new(),
        Zone::FireSet => job.fire_nodes.clone(),
        Zone::All => (0..job.base.nodes.len())
            .filter(|&i| job.base.nodes[i].kind == NodeKind::Junction)
            .collect(),
    };
    for i in zone {
        // Nodes already below the target without a fire are not counted.
        if job
            .statics
            .get(i)
            .copied()
            .flatten()
            .is_none_or(|s| s < job.design_p)
        {
            continue;
        }
        if let Some(p) = probe.pressure[i]
            && p < crit
        {
            crit = p;
            node = i;
        }
    }
    (residual, crit, node)
}

/// Advances the analysis when a run finishes. Called every frame.
pub fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(job) = app.rpt.ff.job.as_mut() else {
        return;
    };
    let Some(handle) = job.run.as_mut() else {
        return;
    };
    let Some(out) = handle.try_take() else {
        return;
    };
    job.run = None;
    let probe = match out {
        Ok(o) if !o.is_error() => read_probe(&job.base, &o.out),
        Ok(o) => {
            app.rpt
                .ff
                .log
                .push(format!("Solver error {} — see the Error Log", o.code));
            None
        }
        Err(e) => {
            app.rpt.ff.log.push(e.to_string());
            None
        }
    };
    let ff = &mut app.rpt.ff;
    let job = ff.job.as_mut().unwrap();
    let Some(probe) = probe else {
        if matches!(job.phase, Phase::Static) {
            ff.log
                .push("Solver failure analyzing static pressures".into());
            ff.job = None;
            app.status = "Fire flow analysis failed — see the Error Log".into();
            return;
        }
        let fire = job.fire_nodes[job.k];
        ff.log.push(format!(
            "Solver failure analyzing node {}",
            job.base.nodes[fire].id
        ));
        let r = finish_node(job, 0.0, job.residual, fire);
        ff.results.push(r);
        next_node(ff, ctx);
        return;
    };
    let fire = job.fire_nodes.get(job.k).copied().unwrap_or(0);
    match job.phase {
        Phase::Static => {
            job.statics = probe.pressure;
            begin_node(ff, ctx);
        }
        Phase::Design => {
            let (residual, crit, node) = critical(job, &probe, fire);
            job.residual = residual;
            let target = job.design_p;
            if crit >= target {
                let r = finish_node(job, job.design_q, crit, node);
                ff.results.push(r);
                next_node(ff, ctx);
            } else {
                let st = job.statics[fire].unwrap_or(0.0);
                let (lo, f_lo, hi, f_hi) = (0.0, st - target, job.design_q, crit - target);
                let q = lo + (hi - lo) * f_lo / (f_lo - f_hi);
                job.phase = Phase::Search {
                    lo,
                    f_lo,
                    hi,
                    f_hi,
                    q,
                    iter: 0,
                    side: 0,
                };
                spawn(job, Some(fire), q, ctx);
            }
        }
        Phase::Search {
            mut lo,
            mut f_lo,
            mut hi,
            mut f_hi,
            q,
            iter,
            mut side,
        } => {
            let (_, crit, node) = critical(job, &probe, fire);
            let f = crit - job.design_p;
            if f >= 0.0 {
                job.best = Some((q, crit, node));
            }
            // Illinois variant of the false position method.
            if f >= 0.0 {
                lo = q;
                f_lo = f;
                if side == 1 {
                    f_hi /= 2.0;
                }
                side = 1;
            } else {
                hi = q;
                f_hi = f;
                if side == -1 {
                    f_lo /= 2.0;
                }
                side = -1;
            }
            let tol_q = job.qcf.max(1e-6);
            if f.abs() < 0.1 * job.pcf || iter >= 14 || (hi - lo).abs() < tol_q {
                // The flow found, or else the largest trial flow that met the target.
                let (q, crit, node) = if f.abs() < 0.1 * job.pcf {
                    (q, crit, node)
                } else {
                    job.best.unwrap_or((0.0, crit, node))
                };
                let r = finish_node(job, q, crit, node);
                ff.results.push(r);
                next_node(ff, ctx);
            } else {
                let nq = lo + (hi - lo) * f_lo / (f_lo - f_hi);
                job.phase = Phase::Search {
                    lo,
                    f_lo,
                    hi,
                    f_hi,
                    q: nq,
                    iter: iter + 1,
                    side,
                };
                spawn(job, Some(fire), nq, ctx);
            }
        }
    }
    if ff.job.is_none() {
        let met = ff
            .results
            .iter()
            .filter(|r| r.available_q >= r.target_q - 0.5)
            .count();
        app.status = format!(
            "Fire flow analysis complete: {met} of {} nodes meet the target",
            ff.results.len()
        );
    }
}

/// Starts the analysis of the current node, or skips it if its static
/// pressure is already below the target.
fn begin_node(ff: &mut FireFlowState, ctx: &egui::Context) {
    let Some(job) = ff.job.as_mut() else { return };
    loop {
        let Some(&fire) = job.fire_nodes.get(job.k) else {
            ff.job = None;
            return;
        };
        job.best = None;
        let st = job.statics.get(fire).copied().flatten().unwrap_or(0.0);
        job.residual = st;
        if st <= job.design_p {
            let r = finish_node(job, 0.0, st, fire);
            ff.results.push(r);
            job.k += 1;
            continue;
        }
        job.phase = Phase::Design;
        spawn(job, Some(fire), job.design_q, ctx);
        return;
    }
}

fn next_node(ff: &mut FireFlowState, ctx: &egui::Context) {
    if let Some(job) = ff.job.as_mut() {
        job.k += 1;
    }
    begin_node(ff, ctx);
}

/// Builds the result row of the current node (converted to fire flow units).
fn finish_node(job: &Job, q: f64, crit: f64, node: usize) -> FfResult {
    let fire = job.fire_nodes[job.k];
    let st = job.statics.get(fire).copied().flatten().unwrap_or(0.0);
    FfResult {
        node: job.base.nodes[fire].id.clone(),
        static_p: st / job.pcf,
        target_q: job.design_q / job.qcf,
        residual_p: job.residual / job.pcf,
        available_q: q / job.qcf,
        critical_p: crit / job.pcf,
        critical_node: job.base.nodes[node].id.clone(),
    }
}

// ----- report -----------------------------------------------------------------------

fn detail_rows(app: &EpanetApp) -> Rows {
    let (qu, pu, ..) = fire_units(&app.net);
    let mut rows = vec![vec![
        "\n\nNode".to_string(),
        format!("Static\nPressure\n{pu}"),
        format!("Target\nFire Flow\n{qu}"),
        format!("Residual\nPressure\n{pu}"),
        format!("Available\nFire Flow\n{qu}"),
        format!("Critical\nPressure\n{pu}"),
        "\nCritical\nNode".to_string(),
    ]];
    for r in &app.rpt.ff.results {
        rows.push(vec![
            r.node.clone(),
            format!("{:.1}", r.static_p),
            format!("{:.0}", r.target_q),
            format!("{:.1}", r.residual_p),
            format!("{:.0}", r.available_q),
            format!("{:.1}", r.critical_p),
            r.critical_node.clone(),
        ]);
    }
    rows
}

fn summary_rows(app: &EpanetApp) -> Vec<(&'static str, String)> {
    let (qu, pu, ..) = fire_units(&app.net);
    let ff = &app.rpt.ff;
    let Some(d) = &ff.design else {
        return Vec::new();
    };
    let n = ff.results.len();
    let met = ff
        .results
        .iter()
        .filter(|r| r.available_q >= r.target_q - 0.5)
        .count();
    let avg = if n > 0 {
        ff.results.iter().map(|r| r.available_q).sum::<f64>() / n as f64
    } else {
        0.0
    };
    vec![
        ("Target Fire Flow", format!("{:.0} {qu}", d.flow)),
        ("Target Pressure", format!("{:.0} {pu}", d.pressure)),
        ("Time of Day", analysis::clock_time(d.hour * 3600)),
        ("Fire Flow Set", format!("{} nodes", d.nodes)),
        ("Pressure Zone Set", d.zone.label().to_string()),
        (
            "Nodes Meeting Target",
            if n > 0 {
                format!("{:.0} %", met as f64 * 100.0 / n as f64)
            } else {
                "N/A".into()
            },
        ),
        (
            "Average Available Flow",
            if n > 0 {
                format!("{avg:.0} {qu}")
            } else {
                "N/A".into()
            },
        ),
    ]
}

pub fn text(app: &EpanetApp) -> Option<String> {
    app.rpt.ff.design.as_ref()?;
    let mut s = String::from("Fire Flow Analysis: Summary Results\n");
    for (k, v) in summary_rows(app) {
        s.push_str(&format!("  {k:<24}{v}\n"));
    }
    s.push_str("\nFire Flow Analysis: Detailed Results\n");
    for r in detail_rows(app) {
        s.push_str(
            &r.iter()
                .map(|c| c.replace('\n', " ").trim().to_string())
                .collect::<Vec<_>>()
                .join("\t"),
        );
        s.push('\n');
    }
    Some(s)
}

/// The report's Map tab.
const MAP_TAB: u8 = 2;

/// Whether the report shows its map (copied and saved as a picture).
pub fn shows_map(app: &EpanetApp) -> bool {
    app.rpt.ff.design.is_some() && app.rpt.ff.tab == MAP_TAB
}

/// EPANET-UI's colors for the percent of the target flow available:
/// below 25, 25-50, 50-75, 75-100, and 100 or more.
const PERCENT_COLORS: [Color32; 5] = [
    Color32::from_rgb(0xff, 0x00, 0x00),
    Color32::from_rgb(0xff, 0xff, 0x00),
    Color32::from_rgb(0x00, 0xff, 0x00),
    Color32::from_rgb(0x00, 0xff, 0xff),
    Color32::from_rgb(0x00, 0x00, 0xff),
];

/// The color class of a node's available flow (as EPANET-UI rounds it).
fn percent_class(r: &FfResult) -> usize {
    let pct = r.available_q / r.target_q.max(1e-9) * 100.0 + 0.5;
    [25.0, 50.0, 75.0, 100.0]
        .iter()
        .filter(|b| pct >= **b)
        .count()
}

/// The Map tab (EPANET-UI's fire flow map): the network's links, and each
/// node analyzed colored by the percent of the target flow it can supply.
/// Dragged it pans, the wheel and the buttons zoom.
fn fire_map(app: &mut EpanetApp, ui: &mut Ui) {
    use egui::{Align2, Pos2, Rect, Sense, pos2, vec2};
    let (qu, pu, ..) = fire_units(&app.net);
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    // Copy and Save take this picture
    app.export.report_rect = Some(rect);
    let panel = Rect::from_min_max(rect.min, pos2(rect.left() + 150.0, rect.bottom()));
    let map = Rect::from_min_max(pos2(panel.right(), rect.top()), rect.max);
    let painter = ui.painter_at(map);
    painter.rect_filled(map, 0.0, Color32::WHITE);
    app.refresh_geometry();
    let Some(b) = app.geom.bounds else {
        return;
    };
    let ff = &mut app.rpt.ff;
    let inner = map.shrink(20.0);
    let fit = (inner.width() as f64 / (b[2] - b[0]).max(1e-9))
        .min(inner.height() as f64 / (b[3] - b[1]).max(1e-9));
    let scale = fit * ff.map_zoom;
    let mut center = ff
        .map_center
        .unwrap_or([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
    if resp.dragged() {
        let d = resp.drag_delta();
        center[0] -= d.x as f64 / scale;
        center[1] += d.y as f64 / scale;
        ff.map_center = Some(center);
    }
    let zoom_at = |factor: f64, ff: &mut FireFlowState| {
        ff.map_zoom = (ff.map_zoom * factor).clamp(0.5, 200.0);
    };
    if resp.hovered() {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            zoom_at((wheel as f64 / 200.0).exp(), ff);
        }
    }
    let c = map.center();
    let at = |[x, y]: [f64; 2]| -> Pos2 {
        pos2(
            c.x + ((x - center[0]) * scale) as f32,
            c.y - ((y - center[1]) * scale) as f32,
        )
    };
    // the links, plain
    let link = Stroke::new(1.0, Color32::from_gray(130));
    for (i, l) in app.net.links.iter().enumerate() {
        let Some((a, z)) = app.geom.ends.get(i).copied().flatten() else {
            continue;
        };
        let (Some(pa), Some(pz)) = (app.net.nodes[a].pos, app.net.nodes[z].pos) else {
            continue;
        };
        let mut pts = vec![at(pa)];
        pts.extend(l.vertices.iter().map(|&v| at(v)));
        pts.push(at(pz));
        painter.add(egui::Shape::line(pts, link));
    }
    // the nodes analyzed
    let hover = resp.hover_pos();
    let mut tip = None;
    for r in &ff.results {
        let Some(p) = app
            .net
            .node_index(&r.node)
            .and_then(|i| app.net.nodes[i].pos)
        else {
            continue;
        };
        let s = at(p);
        if !map.expand(6.0).contains(s) {
            continue;
        }
        painter.circle(
            s,
            4.5,
            PERCENT_COLORS[percent_class(r)],
            Stroke::new(1.0, Color32::BLACK),
        );
        if hover.is_some_and(|h| h.distance(s) <= 7.0) {
            tip = Some(format!(
                "{}: {:.0} {qu} ({:.0} %)",
                r.node,
                r.available_q,
                r.available_q / r.target_q.max(1e-9) * 100.0
            ));
        }
    }
    if let Some(t) = tip {
        resp.clone().on_hover_text(t);
    }
    // the legend: a panel left of the map, the colors from red (top) to
    // blue with the percents between them, then the targets
    let painter = ui.painter_at(rect);
    let ink = Color32::from_gray(20);
    let font = FontId::proportional(14.0);
    painter.rect_filled(panel, 0.0, Color32::from_gray(0xf3));
    painter.vline(
        panel.right(),
        panel.y_range(),
        Stroke::new(1.0, Color32::from_gray(160)),
    );
    let x = panel.left() + 14.0;
    let mut y = panel.top() + 18.0;
    for line in crate::i18n::tr("Percent of Target Flow")
        .split_whitespace()
        .collect::<Vec<_>>()
        .chunks(2)
    {
        painter.text(
            pos2(x, y),
            Align2::LEFT_TOP,
            line.join(" "),
            font.clone(),
            ink,
        );
        y += 18.0;
    }
    y += 26.0;
    let step = 38.0;
    for (k, color) in PERCENT_COLORS.iter().enumerate() {
        let cy = y + step * k as f32;
        painter.circle(
            pos2(x + 14.0, cy),
            11.0,
            *color,
            Stroke::new(1.0, Color32::BLACK),
        );
        if let Some(b) = ["25", "50", "75", "100"].get(k) {
            painter.text(
                pos2(x + 78.0, cy + step / 2.0),
                Align2::RIGHT_CENTER,
                b,
                font.clone(),
                ink,
            );
        }
    }
    if let Some(d) = &ff.design {
        y += step * 4.0 + 36.0;
        for t in [
            crate::i18n::tr("Targets:").to_string(),
            format!("{:.0} {qu}", d.flow),
            format!("{:.0} {pu}", d.pressure),
        ] {
            painter.text(pos2(x, y), Align2::LEFT_TOP, t, font.clone(), ink);
            y += 22.0;
        }
    }
    // zoom buttons, left out of a picture being taken
    if app.export.busy() {
        return;
    }
    let ff = &mut app.rpt.ff;
    let mut x = rect.right() - 10.0;
    for (tip, k) in [("Full Extent", 2), ("Zoom Out", 1), ("Zoom In", 0)] {
        let r = Rect::from_min_size(pos2(x - 28.0, rect.top() + 10.0), vec2(28.0, 28.0));
        x -= 32.0;
        let b = ui.interact(r, ui.id().with(("ff_zoom", k)), Sense::click());
        painter.rect(
            r,
            3.0,
            if b.hovered() {
                Color32::from_rgb(0xe5, 0xf1, 0xfb)
            } else {
                Color32::WHITE
            },
            Stroke::new(1.0, Color32::from_gray(150)),
            egui::StrokeKind::Inside,
        );
        if k == 2 {
            // four corners: the whole network
            let s = Stroke::new(1.5, ink);
            let q = r.shrink(8.0);
            for (cx, cy, dx, dy) in [
                (q.left(), q.top(), 1.0, 1.0),
                (q.right(), q.top(), -1.0, 1.0),
                (q.left(), q.bottom(), 1.0, -1.0),
                (q.right(), q.bottom(), -1.0, -1.0),
            ] {
                painter.line_segment([pos2(cx, cy), pos2(cx + 5.0 * dx, cy)], s);
                painter.line_segment([pos2(cx, cy), pos2(cx, cy + 5.0 * dy)], s);
            }
        } else {
            // + or −, drawn
            let s = Stroke::new(2.0, ink);
            let m = r.center();
            painter.line_segment([m - vec2(6.0, 0.0), m + vec2(6.0, 0.0)], s);
            if k == 0 {
                painter.line_segment([m - vec2(0.0, 6.0), m + vec2(0.0, 6.0)], s);
            }
        }
        if b.on_hover_text(crate::i18n::tr(tip)).clicked() {
            match k {
                0 => zoom_at(1.5, ff),
                1 => zoom_at(1.0 / 1.5, ff),
                _ => {
                    ff.map_zoom = 1.0;
                    ff.map_center = None;
                }
            }
        }
    }
}

pub fn report(app: &mut EpanetApp, ui: &mut Ui) {
    if app.rpt.ff.design.is_none() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Choose the fire flow settings in the Fire Flow Selector, then press Compute Fire Flows.")
                    .weak(),
            );
        });
        return;
    }
    let mut tab = app.rpt.ff.tab;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (k, n) in ["Summary", "Details", "Map", "Error Log"]
            .iter()
            .enumerate()
        {
            if ui
                .add(
                    egui::Button::new(RichText::new(crate::i18n::tr(n)).size(13.5))
                        .selected(tab as usize == k),
                )
                .clicked()
            {
                tab = k as u8;
            }
        }
        if app.rpt.ff.job.is_some() {
            ui.add_space(12.0);
            ui.spinner();
            ui.label("Computing…");
        }
    });
    app.rpt.ff.tab = tab;
    egui::Frame::NONE
        .stroke(Stroke::new(1.0, Color32::from_gray(190)))
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match tab {
                0 => {
                    egui::Grid::new("ff_summary")
                        .num_columns(2)
                        .striped(true)
                        .spacing([40.0, 10.0])
                        .show(ui, |ui| {
                            for (k, v) in summary_rows(app) {
                                ui.label(RichText::new(k).size(13.5));
                                ui.label(RichText::new(v).size(13.5));
                                ui.end_row();
                            }
                        });
                }
                1 => {
                    let rows = detail_rows(app);
                    let footer = format!("{} nodes analyzed", rows.len() - 1);
                    let mut sort = app.rpt.sort_ff;
                    crate::reports::table(ui, "ff_table", &rows, &mut sort, &footer);
                    app.rpt.sort_ff = sort;
                }
                MAP_TAB => fire_map(app, ui),
                _ => {
                    let log = if app.rpt.ff.log.is_empty() {
                        "No errors were reported.".to_string()
                    } else {
                        app.rpt.ff.log.join("\n")
                    };
                    ui.label(RichText::new(log).font(FontId::monospace(14.0)));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Map tab draws the nodes analyzed in their percent-of-target
    /// colors with the legend and targets; its buttons zoom; Copy then takes
    /// the map's picture.
    #[test]
    fn map_tab_shows_the_nodes_analyzed() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        let ff = &mut app.rpt.ff;
        ff.design = Some(Design {
            flow: 500.0,
            pressure: 20.0,
            hour: 0,
            zone: Zone::Individual,
            nodes: 2,
        });
        let result = |node: &str, q: f64| FfResult {
            node: node.into(),
            static_p: 50.0,
            target_q: 500.0,
            residual_p: 20.0,
            available_q: q,
            critical_p: 20.0,
            critical_node: node.into(),
        };
        ff.results = vec![result("10", 650.0), result("22", 100.0)];
        ff.tab = MAP_TAB;
        app.report_kind = ReportKind::FireFlow;
        assert!(shows_map(&app));
        let frame = |app: &mut EpanetApp, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| report(app, ui),
            );
            out.textures_delta.clear();
            out.shapes
        };
        let mut shapes = Vec::new();
        for _ in 0..3 {
            shapes = frame(&mut app, vec![]);
        }
        let fills: Vec<Color32> = shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Circle(s) => Some(s.fill),
                _ => None,
            })
            .collect();
        assert!(fills.contains(&PERCENT_COLORS[4]), "{fills:?}");
        assert!(fills.contains(&PERCENT_COLORS[0]), "{fills:?}");
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t == "Targets:"), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("500 ")), "{texts:?}");
        // Zoom In (the right button but two)
        let map = app.export.report_rect.unwrap();
        let zoom_in = egui::pos2(map.right() - 10.0 - 64.0 - 14.0, map.top() + 24.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: zoom_in,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&mut app, vec![egui::Event::PointerMoved(zoom_in)]);
        frame(&mut app, vec![button(true)]);
        frame(&mut app, vec![button(false)]);
        assert!(
            (app.rpt.ff.map_zoom - 1.5).abs() < 1e-9,
            "{}",
            app.rpt.ff.map_zoom
        );
        assert_eq!(percent_class(&result("x", 374.0)), 3, "74.8 % rounds to 75");
    }
}
