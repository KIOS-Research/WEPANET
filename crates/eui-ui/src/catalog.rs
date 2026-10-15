//! Benchmark networks from the WaterFutures WaterBenchmarkHub catalog
//! (<https://github.com/WaterFutures/WaterBenchmarkHub>). The hub's
//! `database.json` (name, year, size, components, web page) and its folder of
//! INP files are embedded, and refreshed from GitHub when the window opens;
//! the chosen file is downloaded on demand.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{RichText, Ui};
use serde::Deserialize;

use crate::app::EpanetApp;

const SITE: &str = "https://waterfutures.github.io/WaterBenchmarkHub/";
const RAW_BASE: &str =
    "https://raw.githubusercontent.com/WaterFutures/WaterBenchmarkHub/dev/docs/static/benchmarks/";
const TREE_API: &str =
    "https://api.github.com/repos/WaterFutures/WaterBenchmarkHub/git/trees/dev?recursive=1";
const DB_URL: &str = "https://raw.githubusercontent.com/WaterFutures/WaterBenchmarkHub/dev/docs/static/database.json";
const PREFIX: &str = "docs/static/benchmarks/";
/// INP paths (relative to the benchmarks folder) known when the app was built.
const EMBEDDED: &str = include_str!("../assets/benchmarks.txt");
/// The hub's catalog as it was when the app was built.
const EMBEDDED_DB: &str = include_str!("../assets/benchmarks-database.json");

/// Networks too large for the hub's GitHub folder: (hub ID, download URL).
#[cfg(not(target_arch = "wasm32"))]
const EXTERNAL: &[(&str, &str)] = &[(
    "network-ky-all",
    "https://filedn.com/lumBFq2P9S74PNoLPWtzxG4/Benchmarks/ky-all-customers.inp",
)];
/// That host does not allow downloads from web pages (no CORS), so the web app
/// gets a gzipped copy published next to it (see `.github/workflows/pages.yml`).
#[cfg(target_arch = "wasm32")]
const EXTERNAL: &[(&str, &str)] = &[("network-ky-all", "benchmarks/ky-all-customers.inp.gz")];

#[derive(Deserialize)]
struct Database {
    resources: BTreeMap<String, Resource>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Resource {
    name: String,
    desc: String,
    year: serde_json::Value,
    tags: Vec<String>,
    download_url: serde_json::Value,
    permalink: String,
}

/// What a background request delivered.
enum Reply {
    Tree(Vec<String>),
    Database(Database),
    File(String, Vec<u8>),
    Error(String),
}

enum Request {
    Tree,
    Database,
    File(String),
}

/// One row of the table: a network and one of its INP files.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    name: String,
    desc: String,
    year: String,
    size: String,
    parts: String,
    /// INP file name; empty when the hub offers none to download directly.
    file: String,
    url: Option<String>,
    /// A dataset's or competition's download, opened in the browser.
    link: Option<String>,
    /// The benchmark's page on the hub's website.
    page: Option<String>,
    network: bool,
}

#[derive(Default)]
pub struct CatalogState {
    paths: Vec<String>,
    db: Option<Database>,
    rows: Vec<Row>,
    stale: bool,
    filter: String,
    size: Option<&'static str>,
    /// Some(true): networks only, Some(false): datasets and competitions only.
    networks: Option<bool>,
    refreshed: bool,
    busy: Option<String>,
    error: Option<String>,
    inbox: Arc<Mutex<Vec<Reply>>>,
}

/// Benchmark name from its folder (e.g. `network-anytown` → `anytown`).
fn network_name(path: &str) -> &str {
    let folder = path.split('/').next().unwrap_or(path);
    folder
        .trim_start_matches("network-")
        .trim_start_matches("network")
        .trim()
}

fn file_name(path: &str) -> &str {
    let path = path.split(['?', '#']).next().unwrap_or(path);
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

fn parse_db(json: &str) -> Option<Database> {
    serde_json::from_str(json).ok()
}

/// Compares names with their numbers in numeric order (KY2 before KY10).
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    loop {
        match (a.first(), b.first()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na = a.iter().take_while(|c| c.is_ascii_digit()).count();
                let nb = b.iter().take_while(|c| c.is_ascii_digit()).count();
                let va: u64 = std::str::from_utf8(&a[..na])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let vb: u64 = std::str::from_utf8(&b[..nb])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                match va.cmp(&vb) {
                    Ordering::Equal => (a, b) = (&a[na..], &b[nb..]),
                    o => return o,
                }
            }
            (Some(x), Some(y)) => match x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase()) {
                Ordering::Equal => (a, b) = (&a[1..], &b[1..]),
                o => return o,
            },
        }
    }
}

fn value_text(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Array(a) => a.iter().find_map(value_text),
        _ => None,
    }
}

/// The table rows: every benchmark in the hub's catalog, networks with their
/// INP file(s) from the hub's folder, then any INP files in the folder the
/// catalog lacks.
fn build_rows(db: Option<&Database>, paths: &[String]) -> Vec<Row> {
    let mut used = vec![false; paths.len()];
    let mut rows = Vec::new();
    let hub_url = |p: &str| format!("{RAW_BASE}{}", p.replace(' ', "%20"));
    for (id, r) in db.iter().flat_map(|d| &d.resources) {
        let network = id.starts_with("network-");
        let tag = |t: &str| r.tags.iter().any(|x| x.eq_ignore_ascii_case(t));
        const SIZES: [&str; 3] = ["small", "medium", "large"];
        let size = SIZES
            .into_iter()
            .find(|&t| tag(t))
            .unwrap_or("")
            .to_string();
        // A network's components; what a dataset or competition is about.
        let mut parts: Vec<String> = if network {
            ["tanks", "pumps", "valves", "quality", "demand patterns"]
                .into_iter()
                .filter(|&t| tag(t))
                .map(str::to_string)
                .collect()
        } else {
            r.tags
                .iter()
                .filter(|t| !SIZES.contains(&t.as_str()))
                .cloned()
                .collect()
        };
        parts.dedup();
        let download = value_text(&r.download_url).unwrap_or_default();
        let row = Row {
            name: r.name.clone(),
            desc: r.desc.clone(),
            year: value_text(&r.year).unwrap_or_default(),
            size,
            parts: parts.join(", "),
            file: String::new(),
            url: None,
            link: (!network && download.starts_with("http")).then(|| download.clone()),
            page: (!r.permalink.is_empty()).then(|| format!("{SITE}{}", r.permalink)),
            network,
        };
        if !network {
            rows.push(row);
            continue;
        }
        // The hub's own copy: its folder, or (when several networks share a
        // folder) the file the catalog links to.
        let mut hub: Vec<usize> = (0..paths.len())
            .filter(|&i| {
                paths[i]
                    .split('/')
                    .next()
                    .is_some_and(|f| f.eq_ignore_ascii_case(id))
            })
            .collect();
        if hub.is_empty() && !download.is_empty() {
            let want = file_name(&download);
            hub = (0..paths.len())
                .filter(|&i| file_name(&paths[i]).eq_ignore_ascii_case(want))
                .collect();
        }
        let mut files: Vec<(String, String)> = hub
            .iter()
            .map(|&i| {
                used[i] = true;
                (file_name(&paths[i]).to_string(), hub_url(&paths[i]))
            })
            .collect();
        if files.is_empty() {
            if let Some((_, url)) = EXTERNAL.iter().find(|(e, _)| e == id) {
                files.push((file_name(&download).to_string(), url.to_string()));
            } else if file_name(&download).to_ascii_lowercase().ends_with(".inp") {
                files.push((file_name(&download).to_string(), download.clone()));
            }
        }
        if files.is_empty() {
            rows.push(row.clone());
        }
        for (file, url) in files {
            rows.push(Row {
                file,
                url: Some(url),
                ..row.clone()
            });
        }
    }
    // Older copies of a catalogued network (same file in another folder) are left out.
    let catalogued: Vec<String> = rows.iter().map(|r| r.file.to_ascii_lowercase()).collect();
    for (i, p) in paths.iter().enumerate() {
        if !used[i] && !catalogued.contains(&file_name(p).to_ascii_lowercase()) {
            rows.push(Row {
                name: network_name(p).to_string(),
                desc: String::new(),
                year: String::new(),
                size: String::new(),
                parts: String::new(),
                file: file_name(p).to_string(),
                url: Some(hub_url(p)),
                link: None,
                page: None,
                network: true,
            });
        }
    }
    rows.sort_by(|a, b| natural_cmp(&a.name, &b.name).then_with(|| natural_cmp(&a.file, &b.file)));
    rows
}

fn request(inbox: &Arc<Mutex<Vec<Reply>>>, ctx: &egui::Context, url: String, what: Request) {
    let inbox = inbox.clone();
    let ctx = ctx.clone();
    let mut req = ehttp::Request::get(url);
    req.headers.insert("Accept", "*/*");
    ehttp::fetch(req, move |res| {
        let reply = match res {
            Ok(r) if r.ok => match &what {
                Request::File(name) => Reply::File(name.clone(), r.bytes),
                Request::Tree => {
                    let list = parse_tree(&String::from_utf8_lossy(&r.bytes));
                    if list.is_empty() {
                        Reply::Error(String::new())
                    } else {
                        Reply::Tree(list)
                    }
                }
                Request::Database => match parse_db(&String::from_utf8_lossy(&r.bytes)) {
                    Some(db) => Reply::Database(db),
                    None => Reply::Error(String::new()),
                },
            },
            Ok(r) => Reply::Error(format!("{} {}", r.status, r.status_text)),
            Err(e) => Reply::Error(e),
        };
        // A failed list refresh is silent: the embedded catalog stays.
        let is_file = matches!(what, Request::File(_));
        if (is_file || !matches!(&reply, Reply::Error(_)))
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
    let cat = &mut app.catalog;
    let mut file = None;
    for r in replies {
        match r {
            Reply::Tree(mut list) => {
                list.sort_by_key(|p| p.to_ascii_lowercase());
                list.dedup();
                cat.paths = list;
                cat.stale = true;
            }
            Reply::Database(db) => {
                cat.db = Some(db);
                cat.stale = true;
            }
            Reply::File(name, bytes) => {
                cat.busy = None;
                file = Some((name, bytes));
            }
            Reply::Error(e) => {
                cat.busy = None;
                cat.error = Some(format!("Download failed: {e}"));
            }
        }
    }
    file
}

fn matches(r: &Row, filter: &str, size: Option<&str>, networks: Option<bool>) -> bool {
    networks.is_none_or(|n| r.network == n)
        && size.is_none_or(|s| r.size == s)
        && (filter.is_empty()
            || [&r.name, &r.file, &r.desc, &r.parts]
                .iter()
                .any(|t| t.to_ascii_lowercase().contains(filter)))
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
        cat.stale = true;
    }
    if cat.db.is_none() {
        cat.db = parse_db(EMBEDDED_DB);
        cat.stale = true;
    }
    if !cat.refreshed {
        cat.refreshed = true;
        request(&cat.inbox, &ctx, TREE_API.into(), Request::Tree);
        request(&cat.inbox, &ctx, DB_URL.into(), Request::Database);
    }
    if cat.stale {
        cat.stale = false;
        cat.rows = build_rows(cat.db.as_ref(), &cat.paths);
    }
    ui.horizontal(|ui| {
        ui.label("Open an EPANET network from the WaterFutures WaterBenchmarkHub catalog.");
        ui.hyperlink_to("WaterBenchmarkHub ↗", SITE);
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.add(
            egui::TextEdit::singleline(&mut cat.filter)
                .desired_width(210.0)
                .hint_text("Name, file, component or task…"),
        );
        ui.add_space(12.0);
        ui.label("Show:");
        let kind = |n: Option<bool>| match n {
            None => "All",
            Some(true) => "Networks",
            Some(false) => "Datasets & competitions",
        };
        egui::ComboBox::from_id_salt("benchmark_kind")
            .selected_text(kind(cat.networks))
            .show_ui(ui, |ui| {
                for n in [None, Some(true), Some(false)] {
                    ui.selectable_value(&mut cat.networks, n, kind(n));
                }
            });
        ui.add_space(12.0);
        ui.label("Size:");
        egui::ComboBox::from_id_salt("benchmark_size")
            .selected_text(cat.size.unwrap_or("All"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut cat.size, None, "All");
                for s in ["small", "medium", "large"] {
                    ui.selectable_value(&mut cat.size, Some(s), s);
                }
            });
    });
    ui.add_space(10.0);
    let filter = cat.filter.trim().to_ascii_lowercase();
    let (size, networks) = (cat.size, cat.networks);
    let shown = cat
        .rows
        .iter()
        .filter(|r| matches(r, &filter, size, networks))
        .count();
    let mut pick = None;
    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    ui.spacing_mut().scroll.bar_width = 8.0;
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("benchmarks")
                .num_columns(6)
                .striped(true)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    for h in [
                        "Benchmark",
                        "Year",
                        "Size",
                        "Includes / task",
                        "EPANET file",
                        "Details",
                    ] {
                        ui.label(RichText::new(h).font(crate::fonts::bold(13.0)));
                    }
                    ui.end_row();
                    for r in cat
                        .rows
                        .iter()
                        .filter(|r| matches(r, &filter, size, networks))
                    {
                        let name = ui.label(&r.name);
                        if !r.desc.is_empty() && r.desc != r.name {
                            name.on_hover_text(&r.desc);
                        }
                        ui.label(&r.year);
                        ui.label(&r.size);
                        ui.scope(|ui| {
                            ui.set_max_width(250.0);
                            // the full text shows on hover when it is cut short
                            ui.add(egui::Label::new(RichText::new(&r.parts).weak()).truncate());
                        });
                        match (&r.url, &r.link) {
                            (Some(url), _) => {
                                if ui
                                    .add_enabled(cat.busy.is_none(), egui::Link::new(&r.file))
                                    .on_hover_text("Download and open this network")
                                    .clicked()
                                {
                                    pick = Some((r.file.clone(), url.clone()));
                                }
                            }
                            (None, Some(link)) => {
                                ui.hyperlink_to("Download ↗", link).on_hover_text(
                                    "Data and files of this benchmark (opens in the browser)",
                                );
                            }
                            (None, None) => {
                                ui.label(RichText::new("on the hub page").weak());
                            }
                        }
                        match &r.page {
                            Some(page) => {
                                ui.hyperlink_to("More info ↗", page).on_hover_text(
                                    "Description, reference and license on WaterBenchmarkHub",
                                );
                            }
                            None => {
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
            if shown == 0 {
                ui.add_space(12.0);
                ui.label(RichText::new("No matching networks.").weak());
            }
        });
    if let Some((name, url)) = pick {
        cat.busy = Some(name.clone());
        cat.error = None;
        request(&cat.inbox, &ctx, url, Request::File(name));
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
            RichText::new(format!("{shown} of {} benchmarks", cat.rows.len()))
                .small()
                .weak(),
        );
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded_paths() -> Vec<String> {
        EMBEDDED
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn embedded_list_and_names() {
        assert!(embedded_paths().len() >= 70);
        assert_eq!(network_name("network-anytown/Anytown.inp"), "anytown");
        assert_eq!(network_name("network- Pescara/PES.inp"), "Pescara");
        assert_eq!(file_name("network-ctown/CTOWN.INP"), "CTOWN.INP");
        assert_eq!(file_name("https://x.org/a/b.inp?download=1"), "b.inp");
    }

    #[test]
    fn reads_github_tree() {
        let json = r#"{"tree":[{"path": "docs/static/benchmarks/network-net1/Net1.inp","type":"blob"},
            {"path":"docs/static/benchmarks/network-net1/net1_plot.png"},{"path":"README.md"}]}"#;
        assert_eq!(parse_tree(json), vec!["network-net1/Net1.inp".to_string()]);
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["KY10", "KY2", "ky1", "Anytown"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["Anytown", "ky1", "KY2", "KY10"]);
    }

    #[test]
    fn catalog_rows_from_database_and_folder() {
        let db = parse_db(EMBEDDED_DB).expect("embedded database parses");
        let rows = build_rows(Some(&db), &embedded_paths());
        let find = |name: &str| rows.iter().filter(|r| r.name == name).collect::<Vec<_>>();
        // in the hub's folder, with its web page
        let ky1 = find("KY1");
        assert_eq!(ky1.len(), 1);
        assert_eq!(ky1[0].file, "ky1.inp");
        assert!(ky1[0].url.as_deref().unwrap().starts_with(RAW_BASE));
        assert!(
            ky1[0]
                .page
                .as_deref()
                .unwrap()
                .ends_with("benchmarks/network-KY1.html")
        );
        assert_eq!(ky1[0].size, "medium");
        // two networks sharing one folder each get their own file
        assert_eq!(find("BWSN-1")[0].file, "BWSN_Network_1.inp");
        assert_eq!(find("BWSN-2")[0].file, "BWSN_Network_2.inp");
        // hosted outside GitHub
        let all = find("KY-ALL");
        assert_eq!(all[0].file, "ky-all-customers.inp");
        assert_eq!(all[0].url.as_deref(), Some(EXTERNAL[0].1));
        // in the folder but not in the catalog: still listed
        assert!(rows.iter().any(|r| r.file == "BIN.inp" && r.page.is_none()));
        // ...unless it is a copy of a catalogued one
        assert_eq!(rows.iter().filter(|r| r.file == "PES.inp").count(), 1);
        // datasets and competitions: a download link and the hub page, nothing to open
        let leakdb = find("LeakDB");
        assert!(!leakdb[0].network && leakdb[0].url.is_none());
        assert!(leakdb[0].link.is_some() && leakdb[0].page.is_some());
        assert!(leakdb[0].parts.contains("leakage detection"));
        // every benchmark in the catalog is listed
        for r in db.resources.values() {
            assert!(!find(&r.name).is_empty(), "{} missing", r.name);
        }
    }
}
