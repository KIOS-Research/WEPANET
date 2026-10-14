//! Benchmark networks from the WaterFutures WaterBenchmarkHub catalog
//! (<https://github.com/WaterFutures/WaterBenchmarkHub>). The list of INP
//! files is embedded, and refreshed from GitHub when the window opens; the
//! chosen file is downloaded from GitHub on demand.

use std::sync::{Arc, Mutex};

use egui::{RichText, Ui};

use crate::app::EpanetApp;

const REPO_PAGE: &str =
    "https://github.com/WaterFutures/WaterBenchmarkHub/tree/dev/docs/static/benchmarks";
const RAW_BASE: &str =
    "https://raw.githubusercontent.com/WaterFutures/WaterBenchmarkHub/dev/docs/static/benchmarks/";
const TREE_API: &str =
    "https://api.github.com/repos/WaterFutures/WaterBenchmarkHub/git/trees/dev?recursive=1";
const PREFIX: &str = "docs/static/benchmarks/";
/// INP paths (relative to the benchmarks folder) known when the app was built.
const EMBEDDED: &str = include_str!("../assets/benchmarks.txt");

/// What a background request delivered.
enum Reply {
    List(Vec<String>),
    File(String, Vec<u8>),
    Error(String),
}

#[derive(Default)]
pub struct CatalogState {
    paths: Vec<String>,
    filter: String,
    refreshed: bool,
    busy: Option<String>,
    error: Option<String>,
    inbox: Arc<Mutex<Vec<Reply>>>,
}

/// Benchmark name shown in the list, from its folder (e.g. `network-anytown` → `anytown`).
fn network_name(path: &str) -> &str {
    let folder = path.split('/').next().unwrap_or(path);
    folder
        .trim_start_matches("network-")
        .trim_start_matches("network")
        .trim()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Pulls the benchmark INP paths out of GitHub's tree listing (JSON).
fn parse_tree(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(k) = rest.find("\"path\"") {
        rest = &rest[k + 6..];
        let Some(q1) = rest.find('"') else { break };
        let after = &rest[q1 + 1..];
        let Some(q2) = after.find('"') else { break };
        let path = &after[..q2];
        rest = &after[q2 + 1..];
        if let Some(p) = path.strip_prefix(PREFIX)
            && p.to_ascii_lowercase().ends_with(".inp")
        {
            out.push(p.to_string());
        }
    }
    out
}

fn request(inbox: &Arc<Mutex<Vec<Reply>>>, ctx: &egui::Context, url: String, file: Option<String>) {
    let inbox = inbox.clone();
    let ctx = ctx.clone();
    let mut req = ehttp::Request::get(url);
    req.headers.insert("Accept", "*/*");
    ehttp::fetch(req, move |res| {
        let reply = match res {
            Ok(r) if r.ok => match &file {
                Some(name) => Reply::File(name.clone(), r.bytes),
                None => {
                    let list = parse_tree(&String::from_utf8_lossy(&r.bytes));
                    if list.is_empty() {
                        Reply::Error(String::new())
                    } else {
                        Reply::List(list)
                    }
                }
            },
            Ok(r) => Reply::Error(format!("{} {}", r.status, r.status_text)),
            Err(e) => Reply::Error(e),
        };
        // A failed list refresh is silent: the embedded list stays.
        if (file.is_some() || !matches!(&reply, Reply::Error(_)))
            && let Ok(mut q) = inbox.lock()
        {
            q.push(reply);
        }
        ctx.request_repaint();
    });
}

/// Handles finished downloads; returns a downloaded INP file to open.
pub fn poll(app: &mut EpanetApp) -> Option<(String, Vec<u8>)> {
    let replies: Vec<Reply> = app
        .catalog
        .inbox
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default();
    let mut file = None;
    for r in replies {
        match r {
            Reply::List(mut list) => {
                list.sort_by_key(|p| p.to_ascii_lowercase());
                list.dedup();
                app.catalog.paths = list;
            }
            Reply::File(name, bytes) => {
                app.catalog.busy = None;
                file = Some((name, bytes));
            }
            Reply::Error(e) => {
                app.catalog.busy = None;
                app.catalog.error = Some(format!("Download failed: {e}"));
            }
        }
    }
    file
}

/// The Benchmark Networks window.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    let cat = &mut app.catalog;
    if cat.paths.is_empty() {
        cat.paths = EMBEDDED
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
    }
    if !cat.refreshed {
        cat.refreshed = true;
        request(&cat.inbox, &ctx, TREE_API.into(), None);
    }
    ui.label("Open an EPANET network from the WaterFutures WaterBenchmarkHub catalog.");
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.add(
            egui::TextEdit::singleline(&mut cat.filter)
                .desired_width(210.0)
                .hint_text("Network or file name…"),
        );
        ui.hyperlink_to("View on GitHub", REPO_PAGE);
    });
    ui.add_space(10.0);
    let filter = cat.filter.trim().to_ascii_lowercase();
    let mut pick = None;
    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    ui.spacing_mut().scroll.bar_width = 8.0;
    egui::ScrollArea::vertical()
        .max_height(380.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let column_width = ui.available_width() * 0.35;
            egui::Grid::new("benchmarks")
                .num_columns(2)
                .striped(true)
                .min_col_width(column_width)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Network").font(crate::fonts::bold(13.0)));
                    ui.label(RichText::new("EPANET file").font(crate::fonts::bold(13.0)));
                    ui.end_row();
                    for p in &cat.paths {
                        if !filter.is_empty() && !p.to_ascii_lowercase().contains(&filter) {
                            continue;
                        }
                        ui.label(network_name(p));
                        if ui
                            .add_enabled(cat.busy.is_none(), egui::Link::new(file_name(p)))
                            .on_hover_text("Download and open this network")
                            .clicked()
                        {
                            pick = Some(p.clone());
                        }
                        ui.end_row();
                    }
                });
            if !cat
                .paths
                .iter()
                .any(|p| filter.is_empty() || p.to_ascii_lowercase().contains(&filter))
            {
                ui.add_space(12.0);
                ui.label(RichText::new("No matching networks.").weak());
            }
        });
    if let Some(p) = pick {
        let url = format!("{RAW_BASE}{}", p.replace(' ', "%20"));
        cat.busy = Some(file_name(&p).to_string());
        cat.error = None;
        request(&cat.inbox, &ctx, url, Some(file_name(&p).to_string()));
    }
    ui.add_space(8.0);
    if let Some(name) = &cat.busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("Downloading {name}…"));
        });
    } else if let Some(e) = &cat.error {
        ui.label(RichText::new(e).color(egui::Color32::from_rgb(0xc0, 0x30, 0x30)));
    } else {
        ui.label(
            RichText::new(format!(
                "{} of {} networks",
                cat.paths
                    .iter()
                    .filter(|p| filter.is_empty() || p.to_ascii_lowercase().contains(&filter))
                    .count(),
                cat.paths.len()
            ))
            .small()
            .weak(),
        );
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_list_and_names() {
        let n = EMBEDDED.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(n >= 70, "{n}");
        assert_eq!(network_name("network-anytown/Anytown.inp"), "anytown");
        assert_eq!(network_name("network- Pescara/PES.inp"), "Pescara");
        assert_eq!(file_name("network-ctown/CTOWN.INP"), "CTOWN.INP");
    }

    #[test]
    fn reads_github_tree() {
        let json = r#"{"tree":[{"path": "docs/static/benchmarks/network-net1/Net1.inp","type":"blob"},
            {"path":"docs/static/benchmarks/network-net1/net1_plot.png"},{"path":"README.md"}]}"#;
        assert_eq!(parse_tree(json), vec!["network-net1/Net1.inp".to_string()]);
    }
}
