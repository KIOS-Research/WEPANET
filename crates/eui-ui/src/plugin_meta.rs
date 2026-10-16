//! A plugin's `metadata.txt`: an INI file in the plugin's folder giving its
//! name, version, authors, changelog and so on.
//!
//! ```ini
//! [general]
//! name=Dead Ends
//! version=1.0
//! description=Junctions connected to a single link
//! author=Your name
//! email=you@example.org
//! about=A longer text, on as many lines as needed:
//!     lines that start with a space continue the value.
//! changelog=2026-10-16  1.0:
//!     First version.
//! tags=network, water quality
//! homepage=https://…
//! tracker=https://…/issues
//! repository=https://…
//! icon=icon.png
//! experimental=False
//! deprecated=False
//! wepanetMinimumVersion=0.1
//! ```

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// The file's name in a plugin's folder.
pub const FILE: &str = "metadata.txt";

/// What a plugin's `metadata.txt` says of it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub about: String,
    pub author: String,
    pub email: String,
    pub changelog: String,
    pub tags: String,
    pub homepage: String,
    pub tracker: String,
    /// Where its code is.
    pub repository: String,
    /// Its icon's path in the plugin's folder.
    pub icon: String,
    pub experimental: bool,
    pub deprecated: bool,
    /// The WEPANET versions it works with.
    pub wepanet_minimum_version: String,
    pub wepanet_maximum_version: String,
}

/// Items a `metadata.txt` must have.
const MANDATORY: &[&str] = &["name", "version", "description", "author"];

/// Reads the `[general]` section of a `metadata.txt`. Keys are case
/// insensitive; a line starting with a space or a tab continues the value
/// above it; `#` and `;` start comment lines.
pub fn parse(text: &str) -> Result<Metadata, String> {
    let mut items: Vec<(String, String)> = Vec::new();
    // in [general] (or before any section), and the item being read
    let mut general = true;
    let mut current: Option<usize> = None;
    let mut blank = false;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            blank = current.is_some();
            continue;
        }
        if t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        if line.starts_with([' ', '\t']) {
            if let Some(k) = current
                && general
            {
                let v = &mut items[k].1;
                if blank {
                    v.push('\n');
                }
                if !v.is_empty() {
                    v.push('\n');
                }
                v.push_str(t);
            }
            blank = false;
            continue;
        }
        blank = false;
        if let Some(s) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            general = s.trim().eq_ignore_ascii_case("general");
            current = None;
            continue;
        }
        let Some((k, v)) = t.split_once(['=', ':']) else {
            return Err(format!("{FILE}: \"{t}\" is not key=value"));
        };
        if !general {
            current = None;
            continue;
        }
        let k = k.trim().to_ascii_lowercase();
        items.retain(|(x, _)| *x != k);
        items.push((k, v.trim().to_string()));
        current = Some(items.len() - 1);
    }
    let get = |k: &str| {
        items
            .iter()
            .find(|(x, _)| x == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    if let Some(k) = MANDATORY.iter().find(|k| get(k).is_empty()) {
        return Err(format!("{FILE} has no {k}="));
    }
    let flag = |k: &str| matches!(get(k).to_ascii_lowercase().as_str(), "true" | "yes" | "1");
    Ok(Metadata {
        name: get("name"),
        version: get("version"),
        description: get("description"),
        about: get("about"),
        author: get("author"),
        email: get("email"),
        changelog: get("changelog"),
        tags: get("tags"),
        homepage: get("homepage"),
        tracker: get("tracker"),
        repository: get("repository"),
        icon: get("icon"),
        experimental: flag("experimental"),
        deprecated: flag("deprecated"),
        wepanet_minimum_version: get("wepanetminimumversion"),
        wepanet_maximum_version: get("wepanetmaximumversion"),
    })
}

impl Metadata {
    /// Whether the plugin is meant for this version of WEPANET.
    pub fn check_version(&self) -> Result<(), String> {
        let this = env!("CARGO_PKG_VERSION");
        let min = self.wepanet_minimum_version.trim();
        if !min.is_empty() && version_cmp(this, min).is_lt() {
            return Err(format!(
                "\"{}\" needs WEPANET {min} or later (this is {this})",
                self.name
            ));
        }
        let max = self.wepanet_maximum_version.trim();
        if !max.is_empty() && version_cmp(this, max).is_gt() {
            return Err(format!(
                "\"{}\" works with WEPANET up to {max} (this is {this})",
                self.name
            ));
        }
        Ok(())
    }
}

/// Compares versions part by part, numerically where they are numbers.
pub(crate) fn version_cmp(a: &str, b: &str) -> Ordering {
    let parts =
        |v: &str| -> Vec<String> { v.trim().split(['.', '-']).map(str::to_string).collect() };
    let (pa, pb) = (parts(a), parts(b));
    for k in 0..pa.len().max(pb.len()) {
        let x = pa.get(k).map_or("0", String::as_str);
        let y = pb.get(k).map_or("0", String::as_str);
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_metadata_file() {
        let m = parse(
            "# This file contains metadata for your plugin.\n\
             [general]\n\
             name=ImportPhotos\n\
             unknownKey=4.0\n\
             description=Import Photos\n\
             version=4.0.0\n\
             author=Marios S. Kyriakou, George A. Christou\n\
             email=a@b.org\n\
             \n\
             about= This tool imports photos.\n\
             \x20 It has two buttons.\n\
             changelog=2026-04-25  ImportPhotos 4.0.1:\n\
             \x20           Updated metadata.\n\
             \x20   2026-04-25  ImportPhotos 4.0.0:\n\
             \x20           Revised references.\n\
             \n\
             # Tags are comma separated with spaces allowed\n\
             tags=photos, jpeg\n\
             Experimental=True\n\
             deprecated=False\n\
             [other]\n\
             name=not this one\n",
        )
        .unwrap();
        assert_eq!(m.name, "ImportPhotos");
        assert_eq!(m.version, "4.0.0");
        assert_eq!(m.author, "Marios S. Kyriakou, George A. Christou");
        assert_eq!(m.about, "This tool imports photos.\nIt has two buttons.");
        assert_eq!(
            m.changelog,
            "2026-04-25  ImportPhotos 4.0.1:\nUpdated metadata.\n\
             2026-04-25  ImportPhotos 4.0.0:\nRevised references."
        );
        assert_eq!(m.tags, "photos, jpeg");
        assert!(m.experimental && !m.deprecated);
        assert!(m.check_version().is_ok());
    }

    #[test]
    fn mandatory_items_and_versions() {
        let e = parse("[general]\nname=A\nversion=1\nauthor=me\n").unwrap_err();
        assert!(e.contains("description"), "{e}");
        assert!(parse("[general]\nname A\n").is_err());
        let mut m = parse("name=A\nversion=1\ndescription=d\nauthor=me\n").unwrap();
        m.wepanet_minimum_version = "99.0".into();
        assert!(m.check_version().unwrap_err().contains("99.0 or later"));
        m.wepanet_minimum_version = "0.1".into();
        m.wepanet_maximum_version = "0.0.1".into();
        assert!(m.check_version().is_err());
        assert!(version_cmp("1.10", "1.9").is_gt());
        assert!(version_cmp("1.0", "1").is_eq());
        assert!(version_cmp("2.0.1", "2.0").is_gt());
        assert!(version_cmp("1.0", "1.0.1").is_lt());
    }
}
