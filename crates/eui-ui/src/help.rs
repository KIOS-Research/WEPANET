//! Help: EPANET-UI's User Manual and Tutorial (`docs/Manual.html` and
//! `docs/Tutorial.html`), opened in the browser at the topic asked for.
//!
//! In the web app they are files next to the page; the desktop app carries
//! them and writes them to a temporary folder the first time they are needed.

/// The two help documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doc {
    Manual,
    Tutorial,
}

impl Doc {
    fn file(self) -> &'static str {
        match self {
            Doc::Manual => "Manual.html",
            Doc::Tutorial => "Tutorial.html",
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn contents(self) -> &'static [u8] {
        match self {
            Doc::Manual => include_bytes!("../../../docs/Manual.html"),
            Doc::Tutorial => include_bytes!("../../../docs/Tutorial.html"),
        }
    }
}

/// The anchor of a manual topic: its heading in lower case, with every run
/// of other characters than letters and digits made one `_` and apostrophes
/// dropped (e.g. "EPANET-UI's Workspace" is `epanet_uis_workspace`).
pub fn anchor(topic: &str) -> String {
    let mut out = String::new();
    for c in topic.chars() {
        if c == '\'' || c == '\u{2019}' {
            continue;
        }
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// Opens the User Manual at a topic (a heading of the manual).
pub fn open(ctx: &egui::Context, topic: &str) {
    open_doc(ctx, Doc::Manual, Some(&anchor(topic)));
}

/// Opens a help document, at an anchor if given.
pub fn open_doc(ctx: &egui::Context, doc: Doc, anchor: Option<&str>) {
    let fragment = anchor.map(|a| format!("#{a}")).unwrap_or_default();
    #[cfg(target_arch = "wasm32")]
    ctx.open_url(egui::OpenUrl::new_tab(format!("{}{fragment}", doc.file())));
    #[cfg(not(target_arch = "wasm32"))]
    match local_url(doc, &fragment) {
        Ok(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        Err(e) => log::warn!("help: {e}"),
    }
}

/// Writes the document to the temporary folder (if it is not there yet) and
/// returns a URL that opens it at `fragment`. Windows drops the fragment of
/// a file URL it is asked to open, so a small page that forwards to the
/// right place is written beside it and opened instead.
#[cfg(not(target_arch = "wasm32"))]
fn local_url(doc: Doc, fragment: &str) -> std::io::Result<String> {
    let dir = std::env::temp_dir().join(format!("wepanet-help-{}", env!("CARGO_PKG_VERSION")));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(doc.file());
    let bytes = doc.contents();
    if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(bytes.len() as u64) {
        std::fs::write(&path, bytes)?;
    }
    let page = if fragment.is_empty() {
        path
    } else {
        let go = dir.join(format!("go-{}.html", &fragment[1..]));
        let target = format!("{}{fragment}", doc.file());
        std::fs::write(
            &go,
            format!(
                "<!doctype html><meta charset=\"utf-8\">\
                 <meta http-equiv=\"refresh\" content=\"0;url={target}\">\
                 <a href=\"{target}\">{target}</a>"
            ),
        )?;
        go
    };
    let p = page.to_string_lossy().replace('\\', "/");
    Ok(if p.starts_with('/') {
        format!("file://{p}")
    } else {
        format!("file:///{p}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_match_the_manual() {
        let manual = String::from_utf8_lossy(Doc::Manual.contents());
        for topic in [
            "Source Quality",
            "Multi-Species Quality",
            "Water Quality Sources",
            "Using a Basemap",
            "Project Setup",
            "Map Legend",
            "Program Preferences",
            "EPANET-UI's Workspace",
            "Time Patterns",
            "Data Curves",
            "Single Species Quality",
            "Simple Controls Editor",
            "Rule-Based Controls Editor",
            "Rule-Based Controls",
            "Hydraulic Options",
            "Energy Options",
        ] {
            let a = anchor(topic);
            assert!(manual.contains(&format!("id=\"{a}\"")), "{topic} â {a}");
        }
        assert_eq!(anchor("EPANET-UI's Workspace"), "epanet_uis_workspace");
    }

    #[test]
    fn opens_a_local_copy_at_the_topic() {
        let url = local_url(Doc::Manual, "#source_quality").unwrap();
        assert!(url.starts_with("file:///") && url.ends_with("go-source_quality.html"));
        // file:///C:/... on Windows, file:///tmp/... elsewhere
        let path = url.strip_prefix("file://").unwrap();
        let path = if cfg!(windows) {
            path.trim_start_matches('/')
        } else {
            path
        };
        let go = std::fs::read_to_string(path).unwrap();
        assert!(go.contains("Manual.html#source_quality"));
    }
}
