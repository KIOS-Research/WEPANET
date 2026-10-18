//! Plugins for the tests: small ones written here, and the plugins of the
//! WEPANET-Plugins project when it is checked out next to this one (or where
//! `WEPANET_PLUGINS` says); the tests of those are skipped without it.

use std::path::PathBuf;

/// A script plugin with two commands that need no results.
pub const COUNT: &str = r#"
fn name() { "Count" }

fn description() { "Counts the junctions or the pipes of the network" }

fn menu() { ["Junctions", "Pipes"] }

fn run(item) {
    let kind = if item == "Junctions" { "Junction" } else { "Pipe" };
    let n = 0;
    if item == "Junctions" {
        for id in node_ids() { if node_type(id) == kind { n += 1; } }
    } else {
        for id in link_ids() { if link_type(id) == kind { n += 1; } }
    }
    message(`Number of ${item}: ${n}`);
}
"#;

/// A script plugin that reads the results and shows a table.
pub const LOW_PRESSURE: &str = r#"
fn name() { "Low Pressure" }

fn menu() { ["Below 120"] }

fn run(item) {
    if !has_results() { message("Run an analysis first."); return; }
    let limit = parse_float(item.sub_string(6));
    let rows = [];
    for id in node_ids() {
        if node_type(id) != "Junction" { continue; }
        let lowest = 1e9;
        for p in node_series(id, "Pressure") { if p < lowest { lowest = p; } }
        if lowest < limit { rows.push([id, lowest]); }
    }
    if rows.is_empty() { message("None."); return; }
    select_node(rows[0][0]);
    table("Low pressure", ["Junction", "Lowest pressure"], rows);
}
"#;

/// A Python plugin that needs EPyT.
pub const PYTHON: &str = "NAME = \"EPyT Pressure Check\"\n\
DESCRIPTION = \"Lowest junction pressures\"\n\
MENU = [\"Lowest Pressures\", \"Pressure Statistics\"]\n\
REQUIREMENTS = [\"epyt\"]\n\
\n\
import wepanet\n\
\n\
def run(item):\n    wepanet.message(item)\n";

/// A 64x64 PNG.
pub fn icon() -> &'static [u8] {
    static PNG: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    PNG.get_or_init(|| {
        let img = image::RgbaImage::from_pixel(64, 64, image::Rgba([40, 90, 200, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    })
}

/// The WEPANET-Plugins checkout, if there is one.
pub fn repository() -> Option<PathBuf> {
    let dir = std::env::var_os("WEPANET_PLUGINS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../WEPANET-Plugins")
        });
    if dir.join("plugins.xml").is_file() {
        Some(dir)
    } else {
        eprintln!("no WEPANET-Plugins at {}: skipped", dir.display());
        None
    }
}

/// A file of the checkout, by its path there.
pub fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(repository()?.join(path)).ok()
}

/// The script of each of its plugins, by its path there (`Count/Count.rhai`).
pub fn scripts() -> Vec<(String, String)> {
    let Some(dir) = repository() else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            ["rhai", "py"].iter().find_map(|ext| {
                let file = format!("{name}/{name}.{ext}");
                Some((file.clone(), std::fs::read_to_string(dir.join(&file)).ok()?))
            })
        })
        .collect();
    out.sort();
    out
}
