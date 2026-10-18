//! The core plugins (Plugins > Manage Plugins): Elad Salomons' EPANET plugins
//! (<https://github.com/eladsal/EPANET-Plugins>), written for the Python UI
//! of EPANET MTP4, brought over to run on the desktop and in the browser.
//! (Count is a script plugin now, in the WEPANET-Plugins project.)
//!
//! - **Elevations** – junction elevations from a digital elevation model. The
//!   original used Google's Elevation API (which needs a key); this uses the
//!   free Open-Meteo API (Copernicus DEM, 90 m).
//! - **Fire Flow Curve** – pressure at a junction, and the lowest pressure in
//!   the network, as the demand at that junction grows.

use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, Ui};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use eui_core::analysis;
use eui_core::props::{Element, PropKey};
use eui_core::{Network, NodeKind, NodeVar, SimResults};
use eui_engine::RunHandle;

use crate::app::{Dialog, EpanetApp};

pub const PLUGINS_REPO: &str = "https://github.com/eladsal/EPANET-Plugins";
const ELEVATION_API: &str = "https://api.open-meteo.com/v1/elevation";
/// Coordinates per Open-Meteo request (its limit).
const PER_REQUEST: usize = 100;

#[derive(Default)]
pub struct PluginsState {
    /// Text of the Plugins message box.
    pub message: String,
    elev: Option<ElevationJob>,
    elev_error: Option<String>,
    pub curve: CurveState,
}

/// The Plugins message box.
pub fn message(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        for line in app.plugins.message.lines() {
            ui.label(RichText::new(line).size(14.0));
        }
    });
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.add_sized([96.0, 30.0], egui::Button::new("OK"))
            .clicked()
    })
    .inner
        || ui.input(|i| i.key_pressed(egui::Key::Enter))
}

// ----- Elevations -------------------------------------------------------------

/// (index of the first junction asked about, elevations in meters)
type ElevationReply = (usize, Result<Vec<f64>, String>);

struct ElevationJob {
    /// Junctions and their longitude/latitude.
    points: Vec<(usize, [f64; 2])>,
    /// Elevations in meters, as they arrive.
    meters: Vec<Option<f64>>,
    next: usize,
    in_flight: bool,
    inbox: Arc<Mutex<Vec<ElevationReply>>>,
    error: Option<String>,
}

/// The junctions that can be located: (index, longitude/latitude), or why not.
fn locate_junctions(app: &mut EpanetApp) -> Result<Vec<(usize, [f64; 2])>, String> {
    let junctions: Vec<(usize, [f64; 2])> = app
        .net
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Junction)
        .filter_map(|(i, n)| n.pos.map(|p| (i, p)))
        .collect();
    if junctions.is_empty() {
        return Err("The network has no junctions with coordinates.".into());
    }
    let geo = crate::basemap::geo(app)
        .map_err(|e| format!("{e}. Set it in Map > Basemap > Load, then try again."))?;
    junctions
        .into_iter()
        .map(|(i, p)| {
            geo.to_lonlat(p)
                .filter(|ll| ll[0].abs() <= 180.0 && ll[1].abs() <= 90.0)
                .map(|ll| (i, ll))
                .ok_or_else(|| "Some coordinates are outside the coordinate system.".to_string())
        })
        .collect()
}

fn request_elevations(job: &mut ElevationJob, ctx: &egui::Context) {
    let k = job.next;
    let chunk = &job.points[k..(k + PER_REQUEST).min(job.points.len())];
    let join = |f: fn(&[f64; 2]) -> f64| {
        chunk
            .iter()
            .map(|(_, ll)| format!("{:.6}", f(ll)))
            .collect::<Vec<_>>()
            .join(",")
    };
    let url = format!(
        "{ELEVATION_API}?latitude={}&longitude={}",
        join(|ll| ll[1]),
        join(|ll| ll[0])
    );
    job.next += chunk.len();
    job.in_flight = true;
    let inbox = job.inbox.clone();
    let ctx = ctx.clone();
    ehttp::fetch(ehttp::Request::get(url), move |res| {
        let reply = match res {
            Ok(r) if r.ok => parse_elevations(&String::from_utf8_lossy(&r.bytes)),
            Ok(r) => Err(format!("{} {}", r.status, r.status_text)),
            Err(e) => Err(e),
        };
        if let Ok(mut q) = inbox.lock() {
            q.push((k, reply));
        }
        ctx.request_repaint();
    });
}

/// The numbers of Open-Meteo's `{"elevation":[...]}` reply.
fn parse_elevations(json: &str) -> Result<Vec<f64>, String> {
    let start = json
        .find("\"elevation\"")
        .and_then(|k| json[k..].find('[').map(|b| k + b + 1))
        .ok_or("Unexpected reply from the elevation service")?;
    let end = json[start..]
        .find(']')
        .ok_or("Unexpected reply from the elevation service")?;
    json[start..start + end]
        .split(',')
        .map(|s| s.trim().parse::<f64>().map_err(|e| e.to_string()))
        .collect()
}

/// The Elevations window.
pub fn elevations(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    ui.set_max_width(420.0);
    ui.label(
        "Sets the elevation of every junction from a digital elevation model \
         (Copernicus DEM, about 90 m resolution), using the free Open-Meteo service.",
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "The junction coordinates are sent to open-meteo.com. \
             Existing elevations are replaced; Undo restores them.",
        )
        .weak(),
    );
    ui.add_space(8.0);
    let mut close = false;
    match &app.plugins.elev {
        None => {
            let n = app.net.count_nodes(NodeKind::Junction);
            ui.label(format!("{n} junctions"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Get Elevations").clicked() {
                    app.plugins.elev_error = None;
                    match locate_junctions(app) {
                        Ok(points) => {
                            let mut job = ElevationJob {
                                meters: vec![None; points.len()],
                                points,
                                next: 0,
                                in_flight: false,
                                inbox: Default::default(),
                                error: None,
                            };
                            request_elevations(&mut job, &ctx);
                            app.plugins.elev = Some(job);
                        }
                        Err(e) => app.plugins.elev_error = Some(e),
                    }
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
            if let Some(e) = &app.plugins.elev_error {
                ui.label(RichText::new(e).color(Color32::from_rgb(0xc0, 0x39, 0x2b)));
            }
        }
        Some(job) => {
            let done = job.meters.iter().filter(|m| m.is_some()).count();
            if let Some(e) = &job.error {
                ui.label(
                    RichText::new(format!("Failed: {e}"))
                        .color(Color32::from_rgb(0xc0, 0x39, 0x2b)),
                );
                if ui.button("Close").clicked() {
                    app.plugins.elev = None;
                    close = true;
                }
            } else {
                ui.add(
                    egui::ProgressBar::new(done as f32 / job.meters.len().max(1) as f32)
                        .text(format!("{done} of {}", job.meters.len())),
                );
                if ui.button("Cancel").clicked() {
                    app.plugins.elev = None;
                    app.status = "Elevations cancelled".into();
                    close = true;
                }
            }
        }
    }
    close
}

/// Collects elevation replies; sets the elevations when all have arrived.
fn poll_elevations(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(job) = app.plugins.elev.as_mut() else {
        return;
    };
    let replies: Vec<_> = job
        .inbox
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default();
    for (k, reply) in replies {
        job.in_flight = false;
        match reply {
            Ok(v) => {
                for (j, m) in v.into_iter().enumerate() {
                    if let Some(slot) = job.meters.get_mut(k + j) {
                        *slot = Some(m);
                    }
                }
            }
            Err(e) => job.error = Some(e),
        }
    }
    if job.error.is_some() || job.in_flight {
        return;
    }
    if job.next < job.points.len() {
        request_elevations(job, ctx);
        return;
    }
    if job.meters.iter().any(Option::is_none) {
        job.error = Some("Some elevations were missing from the reply".into());
        return;
    }
    let job = app.plugins.elev.take().unwrap();
    let to_units = if app.net.is_si() { 1.0 } else { 3.280_84 };
    app.checkpoint();
    for ((i, _), m) in job.points.iter().zip(&job.meters) {
        let value = format!("{:.2}", m.unwrap_or(0.0) * to_units);
        let _ = app
            .net
            .set_prop(Element::Node(*i), PropKey::Field(0), &value);
    }
    app.plugins.message = format!(
        "Elevations imported for {} junctions\n(Copernicus DEM via Open-Meteo)",
        job.points.len()
    );
    app.dialog = Dialog::PluginMessage;
}

// ----- Fire Flow Curve -------------------------------------------------------

pub struct CurveState {
    pub node: String,
    pub min: String,
    pub step: String,
    pub max: String,
    /// Hour of the day of the fire (0-23).
    pub hour: u32,
    /// (demand, pressure at the node, lowest junction pressure)
    pub points: Vec<[f64; 3]>,
    pub error: Option<String>,
    job: Option<CurveJob>,
}

impl Default for CurveState {
    fn default() -> Self {
        CurveState {
            node: String::new(),
            min: "0".into(),
            step: String::new(),
            max: String::new(),
            hour: 0,
            points: Vec::new(),
            error: None,
            job: None,
        }
    }
}

struct CurveJob {
    base: Network,
    node: usize,
    fire_time: u32,
    flows: Vec<f64>,
    k: usize,
    run: Option<RunHandle>,
}

fn spawn_curve_run(job: &mut CurveJob, ctx: &egui::Context) {
    let net =
        analysis::fire_flow_network(&job.base, Some(job.node), job.flows[job.k], job.fire_time);
    let ctx = ctx.clone();
    job.run = Some(eui_engine::spawn_run(net.to_inp(), None, move || {
        ctx.request_repaint()
    }));
}

/// Pressure at `node` and the lowest junction pressure at the end of a run.
fn read_pressures(net: &Network, node: usize, out: &[u8]) -> Option<(f64, f64)> {
    let res = SimResults::from_out(out).ok()?;
    let ix = analysis::ResultIndex::new(net, &res);
    let p = res.node_slice(res.n_periods - 1, NodeVar::Pressure);
    let at = |i: usize| ix.nodes.get(i).copied().flatten().map(|r| p[r] as f64);
    let lowest = (0..net.nodes.len())
        .filter(|&i| net.nodes[i].kind == NodeKind::Junction)
        .filter_map(at)
        .fold(f64::INFINITY, f64::min);
    Some((at(node)?, lowest))
}

fn start_curve(app: &mut EpanetApp, ctx: &egui::Context) {
    let c = &mut app.plugins.curve;
    c.error = None;
    let num = |s: &str| s.trim().parse::<f64>().ok();
    let (Some(lo), Some(step), Some(hi)) = (num(&c.min), num(&c.step), num(&c.max)) else {
        c.error = Some("Enter numbers for the demands".into());
        return;
    };
    if step <= 0.0 || hi < lo {
        c.error = Some("The step must be positive and the maximum at least the minimum".into());
        return;
    }
    let Some(node) = app
        .net
        .node_index(c.node.trim())
        .filter(|&i| app.net.nodes[i].kind == NodeKind::Junction)
    else {
        c.error = Some(format!("\"{}\" is not a junction", c.node.trim()));
        return;
    };
    let n = ((hi - lo) / step).floor() as usize + 1;
    if n > 200 {
        c.error = Some("That is more than 200 runs; use a larger step".into());
        return;
    }
    let flows: Vec<f64> = (0..n).map(|k| lo + k as f64 * step).collect();
    let start_clock = analysis::start_clocktime(&app.net);
    let fire_time = (c.hour * 3600 + 86_400 - start_clock) % 86_400;
    let mut job = CurveJob {
        base: app.net.clone(),
        node,
        fire_time,
        flows,
        k: 0,
        run: None,
    };
    spawn_curve_run(&mut job, ctx);
    c.points.clear();
    c.job = Some(job);
}

fn poll_curve(app: &mut EpanetApp, ctx: &egui::Context) {
    let c = &mut app.plugins.curve;
    let Some(job) = c.job.as_mut() else {
        return;
    };
    let Some(out) = job.run.as_mut().and_then(|h| h.try_take()) else {
        return;
    };
    job.run = None;
    let q = job.flows[job.k];
    match out {
        Ok(o) if !o.is_error() => {
            if let Some((p, lowest)) = read_pressures(&job.base, job.node, &o.out) {
                c.points.push([q, p, lowest]);
            }
        }
        // A demand the network cannot supply ends the curve.
        Ok(o) => {
            c.error = Some(format!(
                "The solver failed at a demand of {q} (error {}); the curve stops there",
                o.code
            ));
            c.job = None;
            return;
        }
        Err(e) => {
            c.error = Some(e.to_string());
            c.job = None;
            return;
        }
    }
    job.k += 1;
    if job.k < job.flows.len() {
        spawn_curve_run(job, ctx);
    } else {
        c.job = None;
    }
}

/// The Fire Flow Curve window.
pub fn curve(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    let flow_u = app.net.flow_units();
    let p_u = app.pressure_units();
    let selected = match app.selection {
        Some(Element::Node(i)) if app.net.nodes[i].kind == NodeKind::Junction => {
            Some(app.net.nodes[i].id.clone())
        }
        _ => None,
    };
    let running = app.plugins.curve.job.is_some();
    let c = &mut app.plugins.curve;
    if c.node.is_empty()
        && let Some(id) = &selected
    {
        c.node = id.clone();
    }
    if c.step.is_empty() {
        let si = app.net.is_si();
        c.step = if si { "5" } else { "60" }.into();
        c.max = if si { "75" } else { "1200" }.into();
    }
    ui.set_min_width(560.0);
    ui.add_enabled_ui(!running, |ui| {
        egui::Grid::new("ff_curve")
            .num_columns(3)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Junction");
                ui.add(egui::TextEdit::singleline(&mut c.node).desired_width(90.0));
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Use selected"))
                    .clicked()
                    && let Some(id) = &selected
                {
                    c.node = id.clone();
                }
                ui.end_row();
                for (label, v) in [
                    ("Minimum demand", &mut c.min),
                    ("Demand step", &mut c.step),
                    ("Maximum demand", &mut c.max),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(v).desired_width(90.0));
                    ui.label(&flow_u);
                    ui.end_row();
                }
                ui.label("Time of day");
                egui::ComboBox::from_id_salt("ff_curve_hour")
                    .selected_text(analysis::clock_time(c.hour * 3600))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for h in 0..24 {
                            ui.selectable_value(&mut c.hour, h, analysis::clock_time(h * 3600));
                        }
                    });
                ui.end_row();
            });
    });
    ui.add_space(6.0);
    let (mut close, mut run) = (false, false);
    ui.horizontal(|ui| {
        if let Some(job) = &c.job {
            ui.add(
                egui::ProgressBar::new(job.k as f32 / job.flows.len() as f32)
                    .desired_width(240.0)
                    .text(format!("Run {} of {}", job.k + 1, job.flows.len())),
            );
            if ui.button("Cancel").clicked() {
                c.job = None;
            }
        } else {
            run = ui.button("Run").clicked();
            close = ui.button("Close").clicked();
        }
    });
    if run {
        start_curve(app, &ctx);
    }
    let c = &app.plugins.curve;
    if let Some(e) = &c.error {
        ui.label(RichText::new(e).color(Color32::from_rgb(0xc0, 0x39, 0x2b)));
    }
    if !c.points.is_empty() {
        let line = |k: usize| PlotPoints::from_iter(c.points.iter().map(|p| [p[0], p[k]]));
        Plot::new("ff_curve_plot")
            .height(320.0)
            .legend(Legend::default())
            .x_axis_label(format!("Demand at {} ({flow_u})", c.node.trim()))
            .y_axis_label(format!("Pressure ({p_u})"))
            .show(ui, |plot| {
                plot.line(Line::new(format!("Junction {}", c.node.trim()), line(1)).width(2.0));
                plot.line(
                    Line::new("Lowest in the network", line(2))
                        .style(egui_plot::LineStyle::dashed_dense()),
                );
            });
    }
    close
}

/// Advances the plugins' background work. Called every frame.
pub fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    poll_elevations(app, ctx);
    poll_curve(app, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_meteo_reply() {
        assert_eq!(
            parse_elevations(r#"{"elevation":[155.0, 99.0]}"#).unwrap(),
            vec![155.0, 99.0]
        );
        assert!(parse_elevations(r#"{"error":true}"#).is_err());
    }
}
