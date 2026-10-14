use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::{ENERGY_SIZE, EngineError, Progress, RunOutput};

/// The engine's progress callback: stage, hour, context; non-zero cancels.
type ProgressFn = unsafe extern "C" fn(c_int, std::ffi::c_long, *mut c_void) -> c_int;

/// What `eui_run_progress` returns for a cancelled run.
const CANCELLED: c_int = -1;

unsafe extern "C" {
    fn eui_run_progress(
        inp: *const c_char,
        rpt: *const c_char,
        out: *const c_char,
        extra: *const c_char,
        energy: *mut f64,
        cb: Option<ProgressFn>,
        ctx: *mut c_void,
    ) -> c_int;
    fn eui_convert_units(
        inp: *const c_char,
        rpt: *const c_char,
        out_inp: *const c_char,
        flow_units: c_int,
        press_units: c_int,
        hloss_model: c_int,
    ) -> c_int;
    fn eui_error_message(code: c_int, msg: *mut c_char);
    fn eui_version() -> c_int;
}

/// EPANET keeps a few process-wide statics (e.g. temp-file naming), so runs are serialized.
static ENGINE_LOCK: Mutex<()> = Mutex::new(());
static RUN_COUNTER: AtomicU32 = AtomicU32::new(0);

/// EPANET version as an integer, e.g. 20300 for 2.3.0.
pub fn engine_version() -> i32 {
    unsafe { eui_version() }
}

fn error_message(code: i32) -> String {
    let mut buf = [0 as c_char; 256];
    unsafe {
        eui_error_message(code, buf.as_mut_ptr());
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

fn cpath(p: &std::path::Path) -> Result<CString, EngineError> {
    CString::new(p.to_string_lossy().as_bytes()).map_err(|e| EngineError::Io(e.to_string()))
}

/// Changes the units or head loss formula of `inp` as EPANET does (see
/// [`crate::UnitChange`]) and returns the converted INP text.
pub fn convert_units(inp: &str, change: crate::UnitChange) -> Result<String, EngineError> {
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "epanet-ui-{}-{}",
        std::process::id(),
        RUN_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| EngineError::Io(e.to_string()))?;
    let (inp_path, rpt_path, new_path) =
        (dir.join("in.inp"), dir.join("in.rpt"), dir.join("out.inp"));
    std::fs::write(&inp_path, inp).map_err(|e| EngineError::Io(e.to_string()))?;
    let code = {
        let _guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            eui_convert_units(
                cpath(&inp_path)?.as_ptr(),
                cpath(&rpt_path)?.as_ptr(),
                cpath(&new_path)?.as_ptr(),
                change.flow_units,
                change.pressure_units,
                change.headloss,
            )
        }
    };
    let text = std::fs::read_to_string(&new_path);
    let _ = std::fs::remove_dir_all(&dir);
    if code >= 100 {
        return Err(EngineError::Io(error_message(code)));
    }
    text.map_err(|e| EngineError::Io(e.to_string()))
}

/// Runs a full simulation of `inp` (INP text) and waits for it to finish.
pub fn run_blocking(inp: &str) -> Result<RunOutput, EngineError> {
    run_with_progress(inp, &mut |_| false)
}

/// Calls the Rust progress closure behind `ctx`.
unsafe extern "C" fn on_progress(stage: c_int, hour: std::ffi::c_long, ctx: *mut c_void) -> c_int {
    // SAFETY: ctx is the `&mut &mut dyn FnMut` passed by run_with_progress,
    // alive for the whole (synchronous) run.
    let f = unsafe { &mut *(ctx as *mut &mut dyn FnMut(Progress) -> bool) };
    f(Progress::new(stage, hour as i64)) as c_int
}

/// Runs a full simulation, calling `progress` as each simulation hour starts;
/// returning true from it cancels the run.
pub fn run_with_progress(
    inp: &str,
    mut progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<RunOutput, EngineError> {
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "epanet-ui-{}-{}",
        std::process::id(),
        RUN_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| EngineError::Io(e.to_string()))?;
    let inp_path = dir.join("network.inp");
    let rpt_path = dir.join("network.rpt");
    let out_path = dir.join("network.out");
    let extra_path = dir.join("network.ext");
    std::fs::write(&inp_path, inp).map_err(|e| EngineError::Io(e.to_string()))?;

    let mut energy = [0.0; ENERGY_SIZE];
    let code = {
        let _guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            eui_run_progress(
                cpath(&inp_path)?.as_ptr(),
                cpath(&rpt_path)?.as_ptr(),
                cpath(&out_path)?.as_ptr(),
                cpath(&extra_path)?.as_ptr(),
                energy.as_mut_ptr(),
                Some(on_progress),
                &mut progress as *mut _ as *mut c_void,
            )
        }
    };

    if code == CANCELLED {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(EngineError::Cancelled);
    }
    let mut report = std::fs::read_to_string(&rpt_path).unwrap_or_default();
    if code >= 100 && !report.contains(&code.to_string()) {
        report.push_str(&format!("\n{}\n", error_message(code)));
    }
    let out = if code >= 100 {
        Vec::new()
    } else {
        std::fs::read(&out_path).unwrap_or_default()
    };
    let extra = if code >= 100 {
        Vec::new()
    } else {
        std::fs::read(&extra_path).unwrap_or_default()
    };
    let _ = std::fs::remove_dir_all(&dir);
    Ok(RunOutput {
        code,
        report,
        out,
        energy: (code < 100).then_some(energy),
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eui_core::{NodeVar, SimResults, examples};

    #[test]
    fn runs_net1() {
        let r = run_blocking(examples::NET1).unwrap();
        assert_eq!(r.code, 0, "{}", r.report);
        let res = SimResults::from_out(&r.out).unwrap();
        assert_eq!(res.n_nodes(), 11);
        assert_eq!(res.n_periods, 25);
        // Net1 junction 10 at 0:00: head 1004.35 ft, elevation 710 ft -> 127.54 psi.
        let p = res.node_slice(0, NodeVar::Pressure)[0];
        assert!((p - 127.54).abs() < 0.05, "pressure = {p}");
        let h = res.node_slice(0, NodeVar::Head)[0];
        assert!((h - 1004.35).abs() < 0.05, "head = {h}");
    }

    /// Emitter flow, which the output file only includes in the demand, comes
    /// back per reporting period with the extra results.
    #[test]
    fn extra_results_hold_emitter_flow() {
        use eui_core::{LinkExtra, NodeExtra};
        let inp = examples::NET1.replace("[EMITTERS]", "[EMITTERS]\n 11 0.5");
        let r = run_blocking(&inp).unwrap();
        assert!(!r.is_error(), "{}", r.report);
        let mut res = SimResults::from_out(&r.out).unwrap();
        res.set_extra(&r.extra);
        assert!(res.has_extra());
        let k = res.node_ids.iter().position(|id| id == "11").unwrap();
        for p in [0, res.n_periods - 1] {
            let q = res.node_extra(p, NodeExtra::EmitterFlow, k).unwrap();
            assert!(q > 1.0, "emitter flow at period {p}: {q}");
            let deficit = res.node_extra(p, NodeExtra::DemandDeficit, k).unwrap();
            assert_eq!(deficit, 0.0);
        }
        let pump = res.link_ids.iter().position(|id| id == "9").unwrap();
        assert!(res.link_extra(1, LinkExtra::Energy, pump).unwrap() > 0.0);
    }

    #[test]
    fn net1_energy_balance_matches_epanet_ui() {
        use eui_core::{Network, analysis};
        let net = Network::from_inp(examples::NET1).unwrap();
        let r = run_blocking(examples::NET1).unwrap();
        let res = SimResults::from_out(&r.out).unwrap();
        let e = analysis::energy_balance_from_sums(&net, &res, r.energy.as_ref().unwrap());
        // EPANET-UI 1.0.4's Energy Report for Net1, in MWh/day to two decimals.
        let mwh = |v: f64| format!("{:.2}", v / 1000.0);
        let got: Vec<String> = e.components().iter().map(|c| mwh(c.1)).collect();
        assert_eq!(
            got,
            ["3.80", "1.00", "1.37", "4.86", "0.00", "0.16", "1.17"]
        );
        assert_eq!(format!("{:.1}", e.supplied() / 1000.0), "6.2");
        assert_eq!(format!("{:.1}", e.consumed() / 1000.0), "6.2");
        // Performance Metrics
        let es = e.supplied();
        let metrics = [
            e.demands / es,
            e.friction / es,
            e.leakage / es,
            es / e.min_use,
            e.demands / e.min_use,
        ];
        let got: Vec<String> = metrics.iter().map(|m| format!("{m:.2}")).collect();
        assert_eq!(got, ["0.79", "0.03", "0.00", "1.78", "1.40"]);
        assert_eq!(mwh(e.min_use), "3.48");
        let flows = analysis::system_flow(&net, &res);
        assert_eq!(flows.len(), 25);
        assert!(flows.iter().all(|f| f.stored > 1.5 && f.stored < 2.5));
    }

    #[test]
    fn fire_flow_probe_lowers_pressure() {
        use eui_core::{Network, analysis};
        let net = Network::from_inp(examples::NET1).unwrap();
        let j = net.node_index("22").unwrap();
        let pressure = |q: f64| {
            let n = analysis::fire_flow_network(&net, Some(j), q, 7200);
            let r = run_blocking(&n.to_inp()).unwrap();
            assert!(!r.is_error(), "{}", r.report);
            let res = SimResults::from_out(&r.out).unwrap();
            assert_eq!(res.n_periods, 1);
            let k = res.node_ids.iter().position(|id| id == "22").unwrap();
            res.node_slice(0, NodeVar::Pressure)[k]
        };
        let (p0, p1) = (pressure(0.0), pressure(1000.0));
        assert!(p1 < p0 - 1.0, "static {p0}, with fire flow {p1}");
    }

    #[test]
    fn reports_input_errors() {
        let r = run_blocking("[JUNCTIONS]\n J1 abc\n[END]\n").unwrap();
        assert!(r.is_error());
        assert!(r.out.is_empty());
    }

    #[test]
    fn version_is_2_3() {
        assert!(engine_version() >= 20300);
    }

    #[test]
    fn renamed_pump_still_simulates_with_controls() {
        let mut net = eui_core::Network::from_inp(examples::NET1).unwrap();
        assert!(net.rename_link(net.link_index("9").unwrap(), "P9"));
        assert!(net.rename_node(net.node_index("2").unwrap(), "T2"));
        let out = run_blocking(&net.to_inp()).unwrap();
        assert_eq!(out.code, 0, "{}", out.report);
    }

    #[test]
    fn system_flow_integrates_short_and_non_hourly_reporting_intervals() {
        use eui_core::{Network, analysis};
        let run = |step: &str, start: &str| {
            let mut net = Network::from_inp(examples::NET1).unwrap();
            net.set_option("TIMES", "REPORT TIMESTEP", step);
            net.set_option("TIMES", "REPORT START", start);
            let out = run_blocking(&net.to_inp()).unwrap();
            assert_eq!(out.code, 0, "{}", out.report);
            let res = SimResults::from_out(&out.out).unwrap();
            let rows = analysis::system_flow(&net, &res);
            let expected: f64 = (0..res.n_periods - 1)
                .map(|p| {
                    res.node_slice(p, NodeVar::Demand)
                        .iter()
                        .enumerate()
                        .filter_map(|(i, &q)| {
                            let node = &net.nodes[net.node_index(&res.node_ids[i]).unwrap()];
                            (node.kind != eui_core::NodeKind::Tank && q < 0.0)
                                .then_some(-(q as f64))
                        })
                        .sum::<f64>()
                        * res.report_step as f64
                        / analysis::FLOW_UCF[res.flow_units]
                        * 7.480519e-6
                })
                .sum();
            assert!((rows.iter().map(|r| r.produced).sum::<f64>() - expected).abs() < 1e-9);
            rows
        };
        let hourly = run("1:00", "0:00");
        let quarter = run("0:15", "0:00");
        for i in 0..2 {
            // Finer hydraulic reporting resolves changing tank heads; the hourly
            // integral is approximate, but must not differ by a factor of four.
            assert!((hourly[i].produced - quarter[i].produced).abs() / hourly[i].produced < 0.05);
            assert!((hourly[i].consumed - quarter[i].consumed).abs() < 1e-6);
            assert!((hourly[i].stored - quarter[i].stored).abs() / hourly[i].stored < 0.01);
        }
        for step in ["0:45", "1:30"] {
            let rows = run(step, "0:00");
            assert_eq!(rows.len(), 25);
            assert!((rows.iter().map(|r| r.interval_hours).sum::<f64>() - 24.0).abs() < 1e-9);
            assert!(
                rows[..24]
                    .iter()
                    .all(|r| (r.interval_hours - 1.0).abs() < 1e-9)
            );
        }
        let offset = run("0:15", "0:30");
        assert_eq!(offset[0].hours, 0.5);
        assert_eq!(offset[0].interval_hours, 0.5);
        assert_eq!(offset.last().unwrap().interval_hours, 0.0);
    }
}
