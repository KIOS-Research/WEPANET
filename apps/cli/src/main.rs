//! EPANET-UI command line, sharing the core and engine with the apps.
//!
//! ```text
//! epanet-cli info  network.inp
//! epanet-cli run   network.inp [--report out.rpt] [--csv results.csv] [--var pressure]
//! epanet-cli fmt   network.inp [-o clean.inp]
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use eui_core::results::format_time;
use eui_core::{LinkKind, LinkVar, Network, NodeKind, NodeVar, SimResults};

#[derive(Parser)]
#[command(
    name = "epanet-cli",
    version,
    about = "Inspect and simulate EPANET networks"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print a summary of a network.
    Info { inp: PathBuf },
    /// Run a simulation and print a results summary.
    Run {
        inp: PathBuf,
        /// Write EPANET's status report here.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Write a time series of one variable for every element (CSV, one column per element).
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Variable for --csv.
        #[arg(long, value_enum, default_value_t = Var::Pressure)]
        var: Var,
    },
    /// Re-write a network in the canonical EPANET-UI layout.
    Fmt {
        inp: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Var {
    Demand,
    Head,
    Pressure,
    Quality,
    Flow,
    Velocity,
    Headloss,
}

fn read(path: &PathBuf) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(String::from_utf8(bytes.clone())
        .unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect()))
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Info { inp } => {
            let net = Network::from_inp(&read(&inp)?)?;
            println!("{}", inp.display());
            for line in &net.title {
                println!("  {line}");
            }
            for k in NodeKind::ALL {
                println!("  {:<11} {}", format!("{}s", k.label()), net.count_nodes(k));
            }
            for k in LinkKind::ALL {
                println!("  {:<11} {}", format!("{}s", k.label()), net.count_links(k));
            }
            println!("  Flow units  {}", net.flow_units());
            println!(
                "  Duration    {}",
                net.option("TIMES", "DURATION")
                    .unwrap_or_else(|| "0".into())
            );
        }
        Cmd::Fmt { inp, output } => {
            let net = Network::from_inp(&read(&inp)?)?;
            let text = net.to_inp();
            match output {
                Some(o) => {
                    std::fs::write(&o, text).with_context(|| format!("writing {}", o.display()))?
                }
                None => print!("{text}"),
            }
        }
        Cmd::Run {
            inp,
            report,
            csv,
            var,
        } => {
            let text = read(&inp)?;
            let started = std::time::Instant::now();
            let out = eui_engine::run_blocking(&text)?;
            if let Some(r) = &report {
                std::fs::write(r, &out.report)?;
            }
            if out.is_error() {
                eprintln!("{}", out.report);
                bail!("EPANET error {}", out.code);
            }
            let res = SimResults::from_out(&out.out)?;
            println!(
                "Simulated {} nodes, {} links, {} periods in {:.0} ms{}",
                res.n_nodes(),
                res.n_links(),
                res.n_periods,
                started.elapsed().as_secs_f64() * 1000.0,
                if out.code > 0 {
                    format!(" (warning {})", out.code)
                } else {
                    String::new()
                }
            );
            let units = res.node_units(NodeVar::Pressure);
            let (mut min, mut max) = (
                (f32::INFINITY, String::new(), 0),
                (f32::NEG_INFINITY, String::new(), 0),
            );
            let n_junc = Network::from_inp(&text)?
                .count_nodes(NodeKind::Junction)
                .min(res.n_nodes());
            for p in 0..res.n_periods {
                for (i, &v) in res.node_slice(p, NodeVar::Pressure)[..n_junc]
                    .iter()
                    .enumerate()
                {
                    if v < min.0 {
                        min = (v, res.node_ids[i].clone(), p);
                    }
                    if v > max.0 {
                        max = (v, res.node_ids[i].clone(), p);
                    }
                }
            }
            if n_junc > 0 {
                println!(
                    "Min pressure {:.2} {units} at {} ({})",
                    min.0,
                    min.1,
                    format_time(res.period_time(min.2))
                );
                println!(
                    "Max pressure {:.2} {units} at {} ({})",
                    max.0,
                    max.1,
                    format_time(res.period_time(max.2))
                );
            }
            for p in &res.pumps {
                println!(
                    "Pump {}: {:.1}% utilization, {:.1} kW avg, cost/day {:.2}",
                    res.link_ids[p.link_index], p.utilization_pct, p.avg_kw, p.cost_per_day
                );
            }
            if let Some(path) = csv {
                std::fs::write(&path, to_csv(&res, var))?;
                println!("Wrote {}", path.display());
            }
        }
    }
    Ok(())
}

fn to_csv(res: &SimResults, var: Var) -> String {
    let node_var = match var {
        Var::Demand => Some(NodeVar::Demand),
        Var::Head => Some(NodeVar::Head),
        Var::Pressure => Some(NodeVar::Pressure),
        Var::Quality => Some(NodeVar::Quality),
        _ => None,
    };
    let link_var = match var {
        Var::Flow => LinkVar::Flow,
        Var::Velocity => LinkVar::Velocity,
        _ => LinkVar::Headloss,
    };
    let ids = if node_var.is_some() {
        &res.node_ids
    } else {
        &res.link_ids
    };
    let mut s = String::from("time_h");
    for id in ids {
        let _ = write!(s, ",{id}");
    }
    s.push('\n');
    for p in 0..res.n_periods {
        let _ = write!(s, "{}", res.period_time(p) as f64 / 3600.0);
        let vals = match node_var {
            Some(v) => res.node_slice(p, v),
            None => res.link_slice(p, link_var),
        };
        for v in vals {
            let _ = write!(s, ",{v}");
        }
        s.push('\n');
    }
    s
}
