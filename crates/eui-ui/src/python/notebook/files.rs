//! Notebooks as files (`.ipynb`): opened from the computer, saved, and
//! EPyT's example notebooks, listed and downloaded from its repository.

use std::sync::{Arc, Mutex};

/// EPyT's example notebooks: the folder's listing (GitHub's API) gives
/// their names and where to download them.
const EPYT_EXAMPLES: &str =
    "https://api.github.com/repos/OpenWaterAnalytics/EPyT/contents/examples/notebooks?ref=dev";

/// A cell of a notebook file: Markdown (or code) and its text.
pub(super) type FileCell = (bool, String);

/// What a download brought.
pub(super) enum Fetched {
    /// EPyT's examples: (name, URL of the file).
    List(Result<Vec<(String, String)>, String>),
    /// A notebook: its name and its cells.
    Notebook(String, Result<Vec<FileCell>, String>),
}

/// The downloads that have arrived; the notebook takes them each frame.
pub(super) type Inbox = Arc<Mutex<Vec<Fetched>>>;

/// The cells of an `.ipynb` file (nbformat 4): its code and Markdown cells,
/// without their outputs.
pub(super) fn parse(json: &str) -> Result<Vec<FileCell>, String> {
    let nb: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("not a notebook (.ipynb): {e}"))?;
    let cells = nb
        .get("cells")
        .and_then(|c| c.as_array())
        .ok_or("not a notebook (.ipynb): it has no cells")?;
    let text = |v: Option<&serde_json::Value>| match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(lines)) => lines.iter().filter_map(|l| l.as_str()).collect(),
        _ => String::new(),
    };
    Ok(cells
        .iter()
        .filter_map(|c| {
            let markdown = match c.get("cell_type")?.as_str()? {
                "markdown" => true,
                "code" => false,
                _ => return None,
            };
            let source = text(c.get("source"));
            Some((markdown, source.trim_end().to_string()))
        })
        .collect())
}

/// An `.ipynb` file (nbformat 4) of cells, without their outputs.
pub(super) fn write(cells: &[(bool, &str)]) -> String {
    let lines = |s: &str| -> Vec<String> { s.split_inclusive('\n').map(str::to_string).collect() };
    let cells: Vec<serde_json::Value> = cells
        .iter()
        .map(|(markdown, source)| {
            if *markdown {
                serde_json::json!({"cell_type": "markdown", "metadata": {}, "source": lines(source)})
            } else {
                serde_json::json!({
                    "cell_type": "code",
                    "execution_count": null,
                    "metadata": {},
                    "outputs": [],
                    "source": lines(source),
                })
            }
        })
        .collect();
    let nb = serde_json::json!({
        "cells": cells,
        "metadata": {
            "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
            "language_info": {"name": "python"},
        },
        "nbformat": 4,
        "nbformat_minor": 5,
    });
    serde_json::to_string_pretty(&nb).unwrap_or_default()
}

/// The example notebooks of a GitHub folder listing: (name, download URL),
/// in the order of their numbers (EX2 before EX10).
fn listing(json: &str) -> Result<Vec<(String, String)>, String> {
    let items: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| format!("an unexpected reply from GitHub: {e}"))?;
    let mut list: Vec<(String, String)> = items
        .iter()
        .filter_map(|i| {
            let name = i.get("name")?.as_str()?;
            let url = i.get("download_url")?.as_str()?;
            let name = name.strip_suffix(".ipynb")?;
            Some((name.to_string(), url.to_string()))
        })
        .collect();
    list.sort_by_key(|(n, _)| natural(n));
    Ok(list)
}

/// A key that sorts names with numbers by their numbers.
fn natural(name: &str) -> Vec<(String, u64)> {
    let mut key = Vec::new();
    let mut text = String::new();
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut n = u64::from(c as u8 - b'0');
            while let Some(d) = chars.peek().and_then(|d| d.to_digit(10)) {
                n = n.saturating_mul(10).saturating_add(u64::from(d));
                chars.next();
            }
            key.push((std::mem::take(&mut text).to_lowercase(), n));
        } else {
            text.push(c);
        }
    }
    key.push((text.to_lowercase(), 0));
    key
}

fn get(
    url: &str,
    inbox: &Inbox,
    ctx: &egui::Context,
    done: impl 'static + Send + FnOnce(Result<String, String>) -> Fetched,
) {
    let inbox = inbox.clone();
    let ctx = ctx.clone();
    ehttp::fetch(ehttp::Request::get(url), move |res| {
        let text = match res {
            Ok(r) if r.ok => Ok(String::from_utf8_lossy(&r.bytes).into_owned()),
            Ok(r) => Err(format!("{} {}", r.status, r.status_text)),
            Err(e) => Err(e),
        };
        if let Ok(mut q) = inbox.lock() {
            q.push(done(text));
        }
        ctx.request_repaint();
    });
}

/// Asks GitHub for the list of EPyT's example notebooks.
pub(super) fn list_epyt_examples(inbox: &Inbox, ctx: &egui::Context) {
    get(EPYT_EXAMPLES, inbox, ctx, |r| {
        Fetched::List(r.and_then(|j| listing(&j)))
    });
}

/// The name and the address to download of a notebook's address: a page of
/// GitHub (`github.com/<owner>/<repo>/blob/<branch>/<path>`) becomes its
/// raw file; other addresses are downloaded as they are.
pub(super) fn address(url: &str) -> (String, String) {
    let url = url.trim();
    let raw = match url
        .strip_prefix("https://github.com/")
        .and_then(|rest| rest.split_once("/blob/"))
    {
        Some((repo, path)) => format!("https://raw.githubusercontent.com/{repo}/{path}"),
        None => url.to_string(),
    };
    let raw = raw.strip_suffix("?raw=true").unwrap_or(&raw).to_string();
    let file = raw.rsplit('/').next().unwrap_or("notebook");
    let file = file.split(['?', '#']).next().unwrap_or(file);
    let name = file.strip_suffix(".ipynb").unwrap_or(file);
    let name = if name.is_empty() { "notebook" } else { name };
    (name.to_string(), raw)
}

/// Downloads a notebook (one of EPyT's examples, or from an address).
pub(super) fn download(name: &str, url: &str, inbox: &Inbox, ctx: &egui::Context) {
    let name = name.to_string();
    get(url, inbox, ctx, move |r| {
        Fetched::Notebook(name, r.and_then(|j| parse(&j)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notebook_files_are_read_and_written() {
        let json = r##"{"cells": [
            {"cell_type": "markdown", "metadata": {}, "source": ["# Title\n", "Some text"]},
            {"cell_type": "code", "metadata": {}, "outputs": [{"output_type": "stream"}],
             "source": "from epyt import epanet\nd = epanet('Net1.inp')\n"},
            {"cell_type": "raw", "metadata": {}, "source": "left out"}
        ], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"##;
        let cells = parse(json).unwrap();
        assert_eq!(
            cells,
            vec![
                (true, "# Title\nSome text".to_string()),
                (
                    false,
                    "from epyt import epanet\nd = epanet('Net1.inp')".to_string()
                ),
            ]
        );
        let back: Vec<(bool, &str)> = cells.iter().map(|(m, s)| (*m, s.as_str())).collect();
        assert_eq!(parse(&write(&back)).unwrap(), cells);
        assert!(parse("{\"nbformat\": 4}").is_err());
        assert!(parse("print(1)").is_err());
    }

    #[test]
    fn github_links_open_their_raw_files() {
        let raw = "https://raw.githubusercontent.com/OpenWaterAnalytics/EPyT/dev/examples/notebooks/EX1_Plot.ipynb";
        for link in [
            "https://github.com/OpenWaterAnalytics/EPyT/blob/dev/examples/notebooks/EX1_Plot.ipynb",
            " https://github.com/OpenWaterAnalytics/EPyT/blob/dev/examples/notebooks/EX1_Plot.ipynb?raw=true ",
            raw,
        ] {
            assert_eq!(
                address(link),
                ("EX1_Plot".to_string(), raw.to_string()),
                "{link}"
            );
        }
        let (name, url) = address("https://example.com/nb/demo.ipynb?token=1");
        assert_eq!(name, "demo");
        assert_eq!(url, "https://example.com/nb/demo.ipynb?token=1");
    }

    #[test]
    fn epyt_examples_are_listed_by_their_numbers() {
        let json = r#"[
            {"name": "EX10_close_pipes.ipynb", "download_url": "https://x/EX10.ipynb"},
            {"name": "README.md", "download_url": "https://x/README.md"},
            {"name": "EX2_Hydraulic_analysis.ipynb", "download_url": "https://x/EX2.ipynb"},
            {"name": "EX1_Plot.ipynb", "download_url": "https://x/EX1.ipynb"}
        ]"#;
        let names: Vec<String> = listing(json).unwrap().into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            ["EX1_Plot", "EX2_Hydraulic_analysis", "EX10_close_pipes"]
        );
    }
}
