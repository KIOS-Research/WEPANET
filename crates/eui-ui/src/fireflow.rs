//! Fire flow analysis (EPANET-UI's `fireflowcalc` / `fireflowrpt`).
//!
//! For each selected junction it finds the largest fire flow that keeps the
//! pressure at that node, or in a chosen pressure zone, above a target.
//! Every trial flow is one run of the engine on a copy of the network that
//! adds the fire demand at the time of day of the fire. The runs are chained
//! from the UI loop, so the analysis works the same on the desktop and in the
//! browser.

use egui::{Color32, FontId, RichText, Stroke, Ui};
use eui_core::analysis::{self, FLOW_UCF, PRESSURE_UCF};
use eui_core::{Network, NodeKind, NodeVar, SimResults};
use eui_engine::RunHandle;

use crate::app::{EpanetApp, ReportKind, ViewMode};
use crate::reports::Rows;

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
    pub results: Vec<FfResult>,
    pub log: Vec<String>,
    design: Option<Design>,
    job: Option<Job>,
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
            results: Vec::new(),
            log: Vec::new(),
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
    let (qu, pu, ..) = fire_units(&app.net);
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
            let b = ui.button("Select Nodes");
            ui.label(format!("{} selected", ff.nodes.len()));
            egui::Popup::menu(&b).show(|ui| {
                if ui.button("All junctions").clicked() {
                    ff.nodes = app
                        .net
                        .nodes
                        .iter()
                        .filter(|n| n.kind == NodeKind::Junction)
                        .map(|n| n.id.clone())
                        .collect();
                }
                let query: Vec<String> = if app.query.active && app.query.nodes {
                    app.query
                        .hits
                        .iter()
                        .filter(|&&i| app.net.nodes[i].kind == NodeKind::Junction)
                        .map(|&i| app.net.nodes[i].id.clone())
                        .collect()
                } else {
                    Vec::new()
                };
                if ui
                    .add_enabled(
                        !query.is_empty(),
                        egui::Button::new("Junctions found by Map Query"),
                    )
                    .clicked()
                {
                    ff.nodes = query;
                }
                let sel = match app.selection {
                    Some(eui_core::props::Element::Node(i))
                        if app.net.nodes[i].kind == NodeKind::Junction =>
                    {
                        Some(app.net.nodes[i].id.clone())
                    }
                    _ => None,
                };
                if ui
                    .add_enabled(
                        sel.is_some(),
                        egui::Button::new("Add the selected junction"),
                    )
                    .clicked()
                    && let Some(id) = sel
                    && !ff.nodes.contains(&id)
                {
                    ff.nodes.push(id);
                }
                if ui.button("Clear").clicked() {
                    ff.nodes.clear();
                }
            });
        });
        ui.add_space(8.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Pressure Zone Set");
            for z in [Zone::Individual, Zone::FireSet, Zone::All] {
                crate::widgets::radio_value(ui, &mut ff.zone, z, z.label());
            }
        });
    });
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

fn start(app: &mut EpanetApp, ctx: &egui::Context) {
    let (_, _, qi, pi) = fire_units(&app.net);
    let ff = &app.rpt.ff;
    let (Ok(q), Ok(p)) = (
        ff.target_flow.trim().parse::<f64>(),
        ff.target_pressure.trim().parse::<f64>(),
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
    app.report_kind = ReportKind::FireFlow;
    app.view_mode = ViewMode::Report;
    app.status = "Computing fire flows…".into();
}

fn spawn(job: &mut Job, node: Option<usize>, q: f64, ctx: &egui::Context) {
    let net = analysis::fire_flow_network(&job.base, node, q, job.fire_time);
    let ctx = ctx.clone();
    job.run = Some(eui_engine::spawn_run(net.to_inp(), move || {
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
        for (k, n) in ["Summary", "Details", "Error Log"].iter().enumerate() {
            if ui
                .add(egui::Button::new(RichText::new(*n).size(13.5)).selected(tab as usize == k))
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
