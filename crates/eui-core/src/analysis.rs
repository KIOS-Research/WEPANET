//! Quantities derived from a network and its simulation results, for
//! EPANET-UI's reports: energy balance (`energycalc.pas`), system flow
//! volumes (`sysflowrpt.pas`), percentiles (`pcntilerpt.pas`), profile paths
//! (`profileselector.pas`), calibration statistics (`calibrationrpt.pas`) and
//! the network edits behind a fire flow analysis (`fireflowcalc.pas`).
//!
//! The energy balance is added up inside the solver at every hydraulic time
//! step, as EPANET-UI does (the engine returns the sums, see
//! [`energy_balance_from_sums`]); [`energy_balance`] integrates it over the
//! reporting periods of the output file instead, which misses the tank and
//! pump changes between reporting times.

use std::collections::HashMap;
use std::collections::VecDeque;

use crate::network::{LinkKind, Network, NodeKind};
use crate::results::{NodeVar, SimResults};

const SEC_PER_DAY: f64 = 86_400.0;
const M_PER_FT: f64 = 0.3048;
/// Flow units per cubic foot per second, in EPANET's flow-unit order
/// (CFS, GPM, MGD, IMGD, AFD, LPS, LPM, MLD, CMH, CMD, CMS).
pub const FLOW_UCF: [f64; 11] = [
    1.0, 448.831, 0.64632, 0.5382, 1.9837, 28.317, 1699.0, 2.4466, 101.94, 2446.6, 0.028317,
];
/// Pressure units per psi, in EPANET's order (psi, kPa, m, bar, ft).
pub const PRESSURE_UCF: [f64; 5] = [1.0, 6.894745, 0.70325, 0.06894745, 2.306645];

/// Index of a flow-units keyword in [`FLOW_UCF`] (GPM if unknown).
pub fn flow_units_index(units: &str) -> usize {
    crate::units::FLOW_UNITS
        .iter()
        .position(|u| u.eq_ignore_ascii_case(units))
        .unwrap_or(1)
}

/// Index of the project's pressure units in [`PRESSURE_UCF`].
pub fn pressure_units_index(net: &Network) -> usize {
    match net
        .option("OPTIONS", "PRESSURE")
        .map(|s| s.to_ascii_uppercase())
        .as_deref()
    {
        Some("PSI") => 0,
        Some("KPA") => 1,
        Some("METERS") => 2,
        Some("BAR") => 3,
        Some("FEET") => 4,
        _ => {
            if net.is_si() {
                2
            } else {
                0
            }
        }
    }
}

// ----- times ---------------------------------------------------------------------

/// Parses an EPANET time value: `H:MM[:SS]`, or a number followed by an
/// optional unit (SEC, MIN, HOURS, DAYS; hours by default), or a clock time
/// with AM / PM. Returns seconds.
pub fn parse_time(s: &str) -> Option<u32> {
    let mut parts = s.split_whitespace();
    let value = parts.next()?;
    let unit = parts.next().unwrap_or("").to_ascii_uppercase();
    let secs = if value.contains(':') {
        let mut f = value.split(':').map(|x| x.trim().parse::<f64>());
        let h = f.next()?.ok()?;
        let m = f.next().unwrap_or(Ok(0.0)).ok()?;
        let s = f.next().unwrap_or(Ok(0.0)).ok()?;
        h * 3600.0 + m * 60.0 + s
    } else {
        let v: f64 = value.parse().ok()?;
        match unit.as_str() {
            u if u.starts_with("SEC") => v,
            u if u.starts_with("MIN") => v * 60.0,
            u if u.starts_with("DAY") => v * SEC_PER_DAY,
            _ => v * 3600.0,
        }
    };
    if secs < 0.0 {
        return None;
    }
    let mut secs = secs.round() as u32;
    match unit.as_str() {
        "AM" if secs >= 12 * 3600 => secs -= 12 * 3600,
        "PM" if secs < 12 * 3600 => secs += 12 * 3600,
        _ => {}
    }
    Some(secs)
}

/// Simulation start time of day (START CLOCKTIME), in seconds.
pub fn start_clocktime(net: &Network) -> u32 {
    net.option("TIMES", "START CLOCKTIME")
        .and_then(|s| parse_time(&s))
        .unwrap_or(0)
        % 86_400
}

/// Formats seconds since midnight as a clock time, e.g. `12:00 am`.
pub fn clock_time(secs: u32) -> String {
    let s = secs % 86_400;
    let (h, m) = (s / 3600, (s % 3600) / 60);
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{m:02} {}", if h < 12 { "am" } else { "pm" })
}

// ----- result lookup ---------------------------------------------------------------

/// Maps network elements to their rows in a results file (by ID, so results
/// stay usable while the network is being edited).
pub struct ResultIndex {
    pub nodes: Vec<Option<usize>>,
    pub links: Vec<Option<usize>>,
}

impl ResultIndex {
    pub fn new(net: &Network, res: &SimResults) -> Self {
        let n: HashMap<&str, usize> = res
            .node_ids
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let l: HashMap<&str, usize> = res
            .link_ids
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        ResultIndex {
            nodes: net
                .nodes
                .iter()
                .map(|x| n.get(x.id.as_str()).copied())
                .collect(),
            links: net
                .links
                .iter()
                .map(|x| l.get(x.id.as_str()).copied())
                .collect(),
        }
    }
}

/// Length of a reporting period in seconds (an hour for a single-period run,
/// as EPANET-UI assumes).
fn period_seconds(res: &SimResults) -> f64 {
    if res.n_periods <= 1 || res.report_step == 0 {
        3600.0
    } else {
        res.report_step as f64
    }
}

// ----- tanks -----------------------------------------------------------------------

/// Initial volume of a tank in project units (ft³ or m³), as EPANET computes it.
pub fn tank_initial_volume(net: &Network, i: usize) -> Option<f64> {
    let n = &net.nodes[i];
    if n.kind != NodeKind::Tank {
        return None;
    }
    let init = n.num(1)?;
    tank_volume(net, i, init)
}

fn tank_volume(net: &Network, i: usize, level: f64) -> Option<f64> {
    let n = &net.nodes[i];
    let min = n.num(2).unwrap_or(0.0);
    let diam = n.num(4).unwrap_or(0.0);
    let curve = n.field(6);
    if !curve.is_empty()
        && curve != "*"
        && let Some((_, _, pts)) = net
            .curves()
            .into_iter()
            .find(|c| c.0.eq_ignore_ascii_case(curve))
    {
        return Some(interpolate_curve(&pts, level));
    }
    let area = std::f64::consts::PI * diam * diam / 4.0;
    let vmin = match n.num(5) {
        Some(v) if v > 0.0 => v,
        _ => area * min,
    };
    Some(vmin + area * (level - min))
}

fn interpolate_curve(pts: &[[f64; 2]], x: f64) -> f64 {
    match pts {
        [] => 0.0,
        [p] => p[1],
        _ => {
            let k = pts
                .windows(2)
                .position(|w| x <= w[1][0])
                .unwrap_or(pts.len() - 2);
            let (a, b) = (pts[k], pts[k + 1]);
            if b[0] == a[0] {
                a[1]
            } else {
                a[1] + (x - a[0]) * (b[1] - a[1]) / (b[0] - a[0])
            }
        }
    }
}

// ----- energy balance --------------------------------------------------------------

/// System energy balance in kWh/day (EPANET-UI's Energy Report).
#[derive(Clone, Debug, Default)]
pub struct EnergyBalance {
    pub inflows: f64,
    pub pumping: f64,
    pub tank_out: f64,
    pub demands: f64,
    pub leakage: f64,
    pub friction: f64,
    pub tank_in: f64,
    /// Energy needed to deliver the demands at the required service pressure.
    pub min_use: f64,
    /// The required pressure used for `min_use`, e.g. `0.1 psi`.
    pub required_pressure: String,
}

impl EnergyBalance {
    pub fn supplied(&self) -> f64 {
        self.inflows + self.pumping + self.tank_out
    }
    pub fn consumed(&self) -> f64 {
        self.demands + self.leakage + self.friction + self.tank_in
    }
    /// The seven balance components with their labels.
    pub fn components(&self) -> [(&'static str, f64); 7] {
        [
            ("External Sources", self.inflows),
            ("Internal Pumping", self.pumping),
            ("From Storage", self.tank_out),
            ("Consumer Demands (Ed)", self.demands),
            ("Leakage Losses (El)", self.leakage),
            ("Friction Losses (Ef)", self.friction),
            ("To Storage", self.tank_in),
        ]
    }
}

pub fn energy_balance(net: &Network, res: &SimResults) -> EnergyBalance {
    let ix = ResultIndex::new(net, res);
    let si = res.is_si();
    let mut e = EnergyBalance::default();

    // Required pressure, converted to feet (US) or meters (SI) of water.
    let preq_raw: f64 = net
        .option("OPTIONS", "REQUIRED PRESSURE")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.1);
    let punits = res.pressure_units.min(4);
    let label = ["psi", "kPa", "m", "bar", "ft"][punits];
    e.required_pressure = format!("{preq_raw:.1} {label}");
    let mut preq = match punits {
        0 => preq_raw / 0.4333,
        1 => preq_raw * 0.334553,
        2 => preq_raw / M_PER_FT,
        3 => preq_raw * 33.4553,
        _ => preq_raw,
    };
    if si {
        preq *= M_PER_FT;
    }
    let sp_grav: f64 = net
        .option("OPTIONS", "SPECIFIC GRAVITY")
        .and_then(|s| s.parse().ok())
        .filter(|g: &f64| *g > 0.0)
        .unwrap_or(1.0);
    preq /= sp_grav;

    let dt = if res.n_periods <= 1 {
        SEC_PER_DAY
    } else {
        period_seconds(res)
    };
    let mut tsum = 0.0;
    for p in 0..res.n_periods {
        let heads = res.node_slice(p, NodeVar::Head);
        let demands = res.node_slice(p, NodeVar::Demand);
        let hmin = heads.iter().fold(0f32, |m, &h| m.min(h)) as f64;
        for (i, n) in net.nodes.iter().enumerate() {
            let Some(r) = ix.nodes[i] else { continue };
            let qd = demands[r] as f64;
            let h = heads[r] as f64 - hmin;
            let en = (qd * h * dt).abs();
            match n.kind {
                NodeKind::Junction => {
                    let el = n.num(0).unwrap_or(0.0);
                    e.min_use += (qd * (el + preq - hmin) * dt).abs();
                    if qd > 0.0 {
                        e.demands += en;
                    } else {
                        e.inflows += en;
                    }
                }
                NodeKind::Reservoir => {
                    if qd < 0.0 {
                        e.inflows += en;
                    } else {
                        e.demands += en;
                    }
                }
                NodeKind::Tank => {
                    if qd > 0.0 {
                        e.tank_in += en;
                    } else {
                        e.tank_out += en;
                    }
                }
            }
        }
        let flows = res.link_slice(p, crate::results::LinkVar::Flow);
        for (i, l) in net.links.iter().enumerate() {
            let (Some(r), Some((a, b))) = (ix.links[i], net.link_ends(l)) else {
                continue;
            };
            let (Some(ra), Some(rb)) = (ix.nodes[a], ix.nodes[b]) else {
                continue;
            };
            let pw = ((heads[ra] - heads[rb]) as f64 * flows[r] as f64).abs() * dt;
            if l.kind == LinkKind::Pump {
                e.pumping += pw;
            } else {
                e.friction += pw;
            }
        }
        tsum += dt;
    }
    if tsum <= 0.0 {
        return e;
    }
    // Q*H in user units -> kW, then kW·s -> kWh/day.
    let mut ecf = FLOW_UCF[1] / FLOW_UCF[res.flow_units.min(10)] / 5310.0;
    if si {
        ecf /= M_PER_FT;
    }
    let f = ecf * (SEC_PER_DAY / 3600.0) / tsum;
    for v in [
        &mut e.inflows,
        &mut e.pumping,
        &mut e.tank_out,
        &mut e.demands,
        &mut e.leakage,
        &mut e.friction,
        &mut e.tank_in,
        &mut e.min_use,
    ] {
        *v *= f;
    }
    e
}

/// Energy balance sums from the engine, accumulated at every hydraulic time
/// step: |Q·H·dt| in project units for inflows, pumping, storage outflow,
/// demands, leakage, friction, storage inflow and minimum use, then the
/// total time in seconds.
pub type EnergySums = [f64; 9];

/// The energy balance from the engine's per-time-step sums, converted to
/// kWh/day as EPANET-UI's `energycalc.Finish` does.
pub fn energy_balance_from_sums(net: &Network, res: &SimResults, s: &EnergySums) -> EnergyBalance {
    let preq_raw: f64 = net
        .option("OPTIONS", "REQUIRED PRESSURE")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.1);
    let label = ["psi", "kPa", "m", "bar", "ft"][res.pressure_units.min(4)];
    let tsum = s[8];
    let f = if tsum > 0.0 {
        let mut ecf = FLOW_UCF[1] / FLOW_UCF[res.flow_units.min(10)] / 5310.0;
        if res.is_si() {
            ecf /= M_PER_FT;
        }
        ecf * (SEC_PER_DAY / 3600.0) / tsum
    } else {
        0.0
    };
    EnergyBalance {
        inflows: s[0] * f,
        pumping: s[1] * f,
        tank_out: s[2] * f,
        demands: s[3] * f,
        leakage: s[4] * f,
        friction: s[5] * f,
        tank_in: s[6] * f,
        min_use: s[7] * f,
        required_pressure: format!("{preq_raw:.1} {label}"),
    }
}

// ----- system flow -----------------------------------------------------------------

/// Volumes in one reporting period (millions of gallons or megaliters).
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemFlow {
    pub hours: f64,
    /// Duration represented by the produced/consumed volumes.
    pub interval_hours: f64,
    pub stored: f64,
    pub produced: f64,
    pub consumed: f64,
}

/// Volume stored in tanks, produced by sources and consumed by demands in
/// each hour of the simulation (EPANET-UI's System Flow Report).
pub fn system_flow(net: &Network, res: &SimResults) -> Vec<SystemFlow> {
    let ix = ResultIndex::new(net, res);
    let si = res.is_si();
    let qcf = FLOW_UCF[res.flow_units.min(10)];
    // ft³ -> million gallons, or ft³ -> megaliters
    let vcf = if si { 28.317e-6 } else { 7.480519e-6 };
    let mut out = Vec::new();
    for p in 0..res.n_periods {
        let d = res.node_slice(p, NodeVar::Demand);
        let heads = res.node_slice(p, NodeVar::Head);
        let (mut produced, mut consumed, mut stored) = (0.0, 0.0, 0.0);
        for (i, n) in net.nodes.iter().enumerate() {
            let Some(r) = ix.nodes[i] else { continue };
            let v = d[r] as f64 / qcf;
            if n.kind == NodeKind::Tank {
                let level = heads[r] as f64 - n.num(0).unwrap_or(0.0);
                stored +=
                    tank_volume(net, i, level).unwrap_or(0.0) * if si { 35.31467 } else { 1.0 };
            } else if v > 0.0 {
                consumed += v;
            } else {
                produced -= v;
            }
        }
        let start = res.period_time(p) as f64;
        let mut t = start;
        let end = if res.n_periods == 1 {
            t + 3600.0
        } else if p + 1 < res.n_periods {
            res.period_time(p + 1) as f64
        } else {
            res.duration as f64
        };
        let next_stored = if p + 1 < res.n_periods {
            let next_heads = res.node_slice(p + 1, NodeVar::Head);
            net.nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| {
                    if n.kind != NodeKind::Tank {
                        return None;
                    }
                    let r = ix.nodes[i]?;
                    tank_volume(net, i, next_heads[r] as f64 - n.num(0).unwrap_or(0.0))
                })
                .sum::<f64>()
                * if si { 35.31467 } else { 1.0 }
        } else {
            stored
        };
        while t < end {
            let boundary = ((t / 3600.0).floor() + 1.0) * 3600.0;
            let dt = end.min(boundary) - t;
            let hour = (t / 3600.0).floor();
            if out
                .last()
                .is_none_or(|row: &SystemFlow| row.hours.floor() != hour)
            {
                out.push(SystemFlow {
                    hours: t / 3600.0,
                    // Interpolate storage when an hourly boundary falls between samples.
                    stored: (stored + (next_stored - stored) * (t - start) / (end - start)) * vcf,
                    ..Default::default()
                });
            }
            let row = out.last_mut().unwrap();
            row.interval_hours += dt / 3600.0;
            row.produced += produced * dt * vcf;
            row.consumed += consumed * dt * vcf;
            t += dt;
        }
        if p + 1 == res.n_periods && res.n_periods > 1 {
            out.push(SystemFlow {
                hours: end / 3600.0,
                stored: stored * vcf,
                ..Default::default()
            });
        }
    }
    if res.n_periods == 1
        && let Some(&last) = out.last()
    {
        out.push(SystemFlow {
            hours: last.hours + 1.0,
            interval_hours: 0.0,
            produced: 0.0,
            consumed: 0.0,
            ..last
        });
    }
    out
}

// ----- percentiles -----------------------------------------------------------------

/// The value below which `p` percent of the sorted `values` fall.
pub fn percentile(sorted: &[f32], p: f64) -> f32 {
    let n = sorted.len();
    if n == 0 {
        return f32::NAN;
    }
    let i = ((p * n as f64 / 100.0).round() as usize).min(n - 1);
    sorted[i]
}

// ----- profile path ----------------------------------------------------------------

/// The links along the path from `start` to `end` with the fewest links
/// (EPANET-UI's Profile Plot), listed from `start`.
pub fn profile_path(net: &Network, start: usize, end: usize) -> Option<Vec<usize>> {
    if start == end {
        return Some(Vec::new());
    }
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); net.nodes.len()];
    for (k, l) in net.links.iter().enumerate() {
        if let Some((a, b)) = net.link_ends(l) {
            adj[a].push((k, b));
            adj[b].push((k, a));
        }
    }
    let mut prev: Vec<Option<(usize, usize)>> = vec![None; net.nodes.len()];
    let mut seen = vec![false; net.nodes.len()];
    seen[start] = true;
    let mut queue = VecDeque::from([start]);
    while let Some(n) = queue.pop_front() {
        if n == end {
            break;
        }
        for &(link, m) in &adj[n] {
            if !seen[m] {
                seen[m] = true;
                prev[m] = Some((link, n));
                queue.push_back(m);
            }
        }
    }
    if !seen[end] {
        return None;
    }
    let mut path = Vec::new();
    let mut n = end;
    while let Some((link, p)) = prev[n] {
        path.push(link);
        n = p;
    }
    path.reverse();
    Some(path)
}

/// Nodes along a profile path with their distance from its start.
pub fn profile_nodes(net: &Network, start: usize, path: &[usize]) -> Vec<(f64, usize)> {
    let mut out = vec![(0.0, start)];
    let mut n = start;
    let mut x = 0.0;
    for &k in path {
        let l = &net.links[k];
        let Some((a, b)) = net.link_ends(l) else {
            break;
        };
        let len = match l.kind {
            LinkKind::Pipe => l.num(0).filter(|v| *v > 0.0).unwrap_or(10.0),
            _ => 10.0,
        };
        x += len;
        n = if a == n { b } else { a };
        out.push((x, n));
    }
    out
}

// ----- calibration -------------------------------------------------------------------

/// One measurement: location ID, time in hours, measured value.
#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub location: String,
    pub hours: f64,
    pub value: f64,
}

/// Reads calibration data in EPANET's format: `location time value` lines,
/// where a line with only `time value` continues the previous location.
/// Times are hours or `H:MM`. Lines starting with `;` are comments.
pub fn parse_calibration(text: &str) -> Vec<Measurement> {
    let mut out = Vec::new();
    let mut loc = String::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        let t: Vec<&str> = line.split_whitespace().collect();
        let (l, time, v) = match t.as_slice() {
            [time, v] => (loc.clone(), *time, *v),
            [l, time, v, ..] => (l.to_string(), *time, *v),
            _ => continue,
        };
        let (Some(secs), Ok(value)) = (parse_time(time), v.parse::<f64>()) else {
            continue;
        };
        loc = l.clone();
        out.push(Measurement {
            location: l,
            hours: secs as f64 / 3600.0,
            value,
        });
    }
    out
}

/// Writes calibration data back in the format [`parse_calibration`] reads.
pub fn format_calibration(data: &[Measurement]) -> String {
    let mut s = String::from(";Location\tTime\tValue\n");
    for m in data {
        s.push_str(&format!("{}\t{}\t{}\n", m.location, m.hours, m.value));
    }
    s
}

/// Running sums for error and correlation statistics.
#[derive(Clone, Copy, Debug, Default)]
pub struct ErrorSums {
    pub n: usize,
    sx: f64,
    sy: f64,
    sx2: f64,
    sy2: f64,
    sxy: f64,
    se: f64,
    se2: f64,
}

impl ErrorSums {
    /// Adds an observed `x` and simulated `y` pair.
    pub fn add(&mut self, x: f64, y: f64) {
        self.n += 1;
        self.sx += x;
        self.sy += y;
        self.sx2 += x * x;
        self.sy2 += y * y;
        self.sxy += x * y;
        let e = (x - y).abs();
        self.se += e;
        self.se2 += e * e;
    }
    pub fn mean_observed(&self) -> f64 {
        self.sx / self.n.max(1) as f64
    }
    pub fn mean_computed(&self) -> f64 {
        self.sy / self.n.max(1) as f64
    }
    pub fn mean_error(&self) -> f64 {
        self.se / self.n.max(1) as f64
    }
    pub fn rms_error(&self) -> f64 {
        (self.se2 / self.n.max(1) as f64).sqrt()
    }
    pub fn correlation(&self) -> f64 {
        let n = self.n as f64;
        let t1 = n * self.sx2 - self.sx * self.sx;
        let t2 = n * self.sy2 - self.sy * self.sy;
        let t3 = n * self.sxy - self.sx * self.sy;
        if t1 * t2 <= 0.0 {
            0.0
        } else {
            t3 / (t1 * t2).sqrt()
        }
    }
}

/// Linear interpolation of a result series at `hours`.
pub fn interpolate_series(res: &SimResults, series: &[f32], hours: f64) -> Option<f64> {
    let t = hours * 3600.0 - res.report_start as f64;
    if t < 0.0 || series.is_empty() {
        return None;
    }
    let step = res.report_step.max(1) as f64;
    let p1 = (t / step).floor() as usize;
    if p1 >= series.len() {
        return None;
    }
    let p2 = (p1 + 1).min(series.len() - 1);
    let (y1, y2) = (series[p1] as f64, series[p2] as f64);
    Some(y1 + (y2 - y1) * (t - p1 as f64 * step) / step)
}

// ----- fire flow ---------------------------------------------------------------------

/// Pattern used to switch on the fire demand at the time of the fire.
pub const FIRE_PATTERN: &str = "EUI_FIREFLOW";

/// A copy of `net` set up to find pressures at `fire_time` (seconds into the
/// simulation) with an extra demand `q` (project flow units) at junction
/// `node`, starting at that time. Water quality and reporting are switched off
/// and only the fire time is saved to the output file.
pub fn fire_flow_network(net: &Network, node: Option<usize>, q: f64, fire_time: u32) -> Network {
    let mut n = net.clone();
    let t = format!(
        "{}:{:02}:{:02}",
        fire_time / 3600,
        (fire_time % 3600) / 60,
        fire_time % 60
    );
    n.set_option("TIMES", "DURATION", &t);
    n.set_option("TIMES", "REPORT START", &t);
    n.set_option("OPTIONS", "QUALITY", "NONE");
    n.set_option("REPORT", "STATUS", "NO");
    n.set_option("REPORT", "SUMMARY", "NO");
    n.set_option("REPORT", "NODES", "NONE");
    n.set_option("REPORT", "LINKS", "NONE");
    let Some(i) = node.filter(|_| q != 0.0) else {
        return n;
    };
    let id = n.nodes[i].id.clone();

    // A pattern that is zero until the fire starts.
    let step = n
        .option("TIMES", "PATTERN TIMESTEP")
        .and_then(|s| parse_time(&s))
        .filter(|s| *s > 0)
        .unwrap_or(3600);
    let start = n
        .option("TIMES", "PATTERN START")
        .and_then(|s| parse_time(&s))
        .unwrap_or(0);
    let k = ((fire_time + start) / step) as usize;
    let mut mult = vec![0.0; k];
    mult.push(1.0);
    let mut pats = n.patterns();
    pats.retain(|p| p.0 != FIRE_PATTERN);
    pats.push((FIRE_PATTERN.to_string(), mult));
    n.set_patterns(&pats);

    // The first [DEMANDS] entry of a junction replaces its [JUNCTIONS] demand,
    // so restate that demand before adding the fire flow.
    let has_demands = n
        .raw_lines("DEMANDS")
        .any(|t| t[0].eq_ignore_ascii_case(&id));
    let mut lines: Vec<String> = n
        .section_text("DEMANDS")
        .lines()
        .map(str::to_string)
        .collect();
    if !has_demands {
        let base = n.nodes[i].field(1).to_string();
        let pat = n.nodes[i].field(2).to_string();
        if base.parse::<f64>().is_ok_and(|b| b != 0.0) {
            lines.push(format!(" {id}\t{base}\t{pat}"));
        }
    }
    lines.push(format!(" {id}\t{q}\t{FIRE_PATTERN}"));
    n.replace_section("DEMANDS", lines);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("24:00"), Some(86_400));
        assert_eq!(parse_time("1:30"), Some(5_400));
        assert_eq!(parse_time("6"), Some(21_600));
        assert_eq!(parse_time("15 MIN"), Some(900));
        assert_eq!(parse_time("12 am"), Some(0));
        assert_eq!(parse_time("12 pm"), Some(43_200));
        assert_eq!(parse_time("6:30 PM"), Some(66_600));
        assert_eq!(clock_time(0), "12:00 am");
        assert_eq!(clock_time(13 * 3600 + 5 * 60), "1:05 pm");
    }

    #[test]
    fn tank_volume_from_geometry() {
        let net = Network::from_inp(crate::examples::NET1).unwrap();
        let t = net.node_index("2").unwrap();
        // Net1 tank 2: init level 120, min 100, diameter 50.5, no min volume.
        let area = std::f64::consts::PI * 50.5 * 50.5 / 4.0;
        let v = tank_initial_volume(&net, t).unwrap();
        assert!((v - area * 120.0).abs() < 1e-6, "{v}");
    }

    #[test]
    fn shortest_profile_path() {
        let net = Network::from_inp(crate::examples::NET1).unwrap();
        let a = net.node_index("11").unwrap();
        let b = net.node_index("32").unwrap();
        let path = profile_path(&net, a, b).unwrap();
        assert_eq!(path.len(), 3, "11-21-31-32 or 11-12-22-32");
        let nodes = profile_nodes(&net, a, &path);
        assert_eq!(nodes.first().unwrap().1, a);
        assert_eq!(nodes.last().unwrap().1, b);
    }

    #[test]
    fn calibration_data() {
        let d = parse_calibration(";comment\n 10  0  1.2\n     2:00 1.4\n 22 1 100 ; x\n");
        assert_eq!(d.len(), 3);
        assert_eq!(d[1].location, "10");
        assert_eq!(d[1].hours, 2.0);
        let mut s = ErrorSums::default();
        for (x, y) in [(1.0, 1.1), (2.0, 2.1), (3.0, 2.9)] {
            s.add(x, y);
        }
        assert!(s.correlation() > 0.98);
        assert!((s.mean_error() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn fire_flow_adds_a_timed_demand() {
        let net = Network::from_inp(crate::examples::NET1).unwrap();
        let j = net.node_index("22").unwrap();
        let n = fire_flow_network(&net, Some(j), 500.0, 7200);
        let dem: Vec<Vec<String>> = n.raw_lines("DEMANDS").collect();
        assert_eq!(dem.len(), 2, "{dem:?}");
        assert_eq!(dem[0][1], "200", "base demand restated");
        assert_eq!(dem[1][2], FIRE_PATTERN);
        let p = n
            .patterns()
            .into_iter()
            .find(|p| p.0 == FIRE_PATTERN)
            .unwrap();
        // Net1's pattern time step is 2 hours: the fire starts in period 2.
        assert_eq!(p.1, vec![0.0, 1.0]);
        assert_eq!(n.option("TIMES", "DURATION").as_deref(), Some("2:00:00"));
    }
}

/// A time option as EPANET-UI shows it: `HH:MM` (with `:SS` when there are
/// seconds), e.g. `01:00` or `24:00`. A clock time (`12 am`, `6:30 PM`) is
/// shown in 24-hour form. Text that is not a time is returned unchanged.
pub fn format_time_option(text: &str) -> String {
    let t = text.trim();
    let upper = t.to_ascii_uppercase();
    let (body, pm) = match upper
        .strip_suffix("PM")
        .map(|b| (b, Some(true)))
        .or_else(|| upper.strip_suffix("AM").map(|b| (b, Some(false))))
    {
        Some((b, p)) => (b.trim().to_string(), p),
        None => (t.to_string(), None),
    };
    let Some(mut secs) = parse_time(&body) else {
        return text.to_string();
    };
    if let Some(pm) = pm {
        secs %= 12 * 3600;
        if pm {
            secs += 12 * 3600;
        }
    }
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if s > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{h:02}:{m:02}")
    }
}

#[cfg(test)]
mod time_option_tests {
    use super::format_time_option as f;

    #[test]
    fn formats_time_options() {
        assert_eq!(f("24:00"), "24:00");
        assert_eq!(f("1:00"), "01:00");
        assert_eq!(f("0:05"), "00:05");
        assert_eq!(f("72"), "72:00");
        assert_eq!(f("300 SEC"), "00:05");
        assert_eq!(f("12 am"), "00:00");
        assert_eq!(f("6:30 PM"), "18:30");
        assert_eq!(f("0:00:30"), "00:00:30");
        assert_eq!(f("abc"), "abc");
    }
}
