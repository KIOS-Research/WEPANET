//! The Plugin Manager (Plugins > Manage Plugins): plugins are
//! offered by repositories — XML files listing each plugin's name, version,
//! description and where to download it — read from their URLs. WEPANET's own
//! repository is `plugins/plugins.xml` of this project; any other can be
//! added by its URL. Plugins can be installed, upgraded when a repository has
//! a newer version, and uninstalled.
//!
//! ```xml
//! <plugins>
//!   <plugin name="Dead Ends" version="1.0">
//!     <description>Junctions connected to a single link</description>
//!     <author>WEPANET</author>
//!     <!-- a ZIP of the plugin's folder, or the folder itself (its script,
//!          named like the plugin, and icon.png): a URL, or relative to the XML -->
//!     <download>Dead Ends</download>
//!     <tags>network</tags>
//!     <homepage>https://…</homepage>
//!   </plugin>
//! </plugins>
//! ```

use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::app::EpanetApp;
use crate::icons::{self, Icon};
use crate::plugin_meta::version_cmp;

const HELP: &str = "https://github.com/KIOS-Research/WEPANET/blob/main/docs/plugins.md";

/// WEPANET's own plugin repository.
pub const DEFAULT_REPOSITORY: &str =
    "https://raw.githubusercontent.com/KIOS-Research/WEPANET/main/plugins/plugins.xml";

/// The repository as it was when this version of WEPANET was built: always
/// offered, so plugins can be installed offline too. An online repository
/// with a higher version of a plugin offers it as an upgrade.
const BUILTIN_XML: &str = include_str!("../../../plugins/plugins.xml");
const BUILTIN_BASE: &str = "builtin:/plugins.xml";
const BUILTIN_NAME: &str = "Built into WEPANET";
/// The scripts `plugins/plugins.xml` lists, by their path there.
pub(crate) const BUILTIN: &[(&str, &str)] = &[
    (
        "Count/Count.rhai",
        include_str!("../../../plugins/Count/Count.rhai"),
    ),
    (
        "Network Statistics/Network Statistics.rhai",
        include_str!("../../../plugins/Network Statistics/Network Statistics.rhai"),
    ),
    (
        "Pipe Summary/Pipe Summary.rhai",
        include_str!("../../../plugins/Pipe Summary/Pipe Summary.rhai"),
    ),
    (
        "Dead Ends/Dead Ends.rhai",
        include_str!("../../../plugins/Dead Ends/Dead Ends.rhai"),
    ),
    (
        "Low Pressure/Low Pressure.rhai",
        include_str!("../../../plugins/Low Pressure/Low Pressure.rhai"),
    ),
    (
        "High Velocity/High Velocity.rhai",
        include_str!("../../../plugins/High Velocity/High Velocity.rhai"),
    ),
    (
        "EPyT Pressure Check/EPyT Pressure Check.py",
        include_str!("../../../plugins/EPyT Pressure Check/EPyT Pressure Check.py"),
    ),
];

/// The built-in repository, with what each plugin's `metadata.txt` adds.
fn builtin_offers() -> Vec<Offer> {
    let mut offers = parse_repository(BUILTIN_XML, BUILTIN_BASE, BUILTIN_NAME).unwrap_or_default();
    for o in &mut offers {
        if let Some(m) = builtin_meta(&o.url).and_then(|t| crate::plugin_meta::parse(t).ok()) {
            o.add_meta(&m);
        }
    }
    offers
}

/// Their icons.
const BUILTIN_ICONS: &[(&str, &[u8])] = &[
    (
        "Count/icon.png",
        include_bytes!("../../../plugins/Count/icon.png"),
    ),
    (
        "Network Statistics/icon.png",
        include_bytes!("../../../plugins/Network Statistics/icon.png"),
    ),
    (
        "Pipe Summary/icon.png",
        include_bytes!("../../../plugins/Pipe Summary/icon.png"),
    ),
    (
        "Dead Ends/icon.png",
        include_bytes!("../../../plugins/Dead Ends/icon.png"),
    ),
    (
        "Low Pressure/icon.png",
        include_bytes!("../../../plugins/Low Pressure/icon.png"),
    ),
    (
        "High Velocity/icon.png",
        include_bytes!("../../../plugins/High Velocity/icon.png"),
    ),
    (
        "EPyT Pressure Check/icon.png",
        include_bytes!("../../../plugins/EPyT Pressure Check/icon.png"),
    ),
];

/// Their `metadata.txt`.
const BUILTIN_META: &[(&str, &str)] = &[
    (
        "Count/metadata.txt",
        include_str!("../../../plugins/Count/metadata.txt"),
    ),
    (
        "Network Statistics/metadata.txt",
        include_str!("../../../plugins/Network Statistics/metadata.txt"),
    ),
    (
        "Pipe Summary/metadata.txt",
        include_str!("../../../plugins/Pipe Summary/metadata.txt"),
    ),
    (
        "Dead Ends/metadata.txt",
        include_str!("../../../plugins/Dead Ends/metadata.txt"),
    ),
    (
        "Low Pressure/metadata.txt",
        include_str!("../../../plugins/Low Pressure/metadata.txt"),
    ),
    (
        "High Velocity/metadata.txt",
        include_str!("../../../plugins/High Velocity/metadata.txt"),
    ),
    (
        "EPyT Pressure Check/metadata.txt",
        include_str!("../../../plugins/EPyT Pressure Check/metadata.txt"),
    ),
];

/// The `metadata.txt` of a built-in plugin's folder (`builtin:/Count`).
fn builtin_meta(folder: &str) -> Option<&'static str> {
    let path = format!(
        "{}/{}",
        folder.strip_prefix("builtin:/")?.replace("%20", " "),
        crate::plugin_meta::FILE
    );
    BUILTIN_META
        .iter()
        .find(|(f, _)| *f == path)
        .map(|(_, s)| *s)
}

/// The script of a built-in offer (`builtin:/Count/Count.rhai`, …).
fn builtin_source(url: &str) -> Option<&'static str> {
    let path = url.strip_prefix("builtin:/")?.replace("%20", " ");
    BUILTIN.iter().find(|(f, _)| *f == path).map(|(_, s)| *s)
}

/// A core plugin: part of WEPANET itself (written in Rust, so it can reach
/// web services or run the engine), offered in the Plugin Manager like the
/// others. Installing one puts it on the Plugins toolbar.
pub(crate) struct CorePlugin {
    pub name: &'static str,
    pub description: &'static str,
    pub tags: &'static str,
    pub icon: Icon,
    pub dialog: crate::app::Dialog,
}

pub(crate) const CORE: &[CorePlugin] = &[
    CorePlugin {
        name: "Elevations",
        description: "Junction elevations from a digital elevation model (Open-Meteo, Copernicus DEM 90 m)",
        tags: "elevation, terrain, web service",
        icon: Icon::Import,
        dialog: crate::app::Dialog::Elevations,
    },
    CorePlugin {
        name: "Fire Flow Curve",
        description: "Pressure at a junction, and the lowest in the network, as its demand grows",
        tags: "fire flow, hydraulics, analysis",
        icon: Icon::FireFlow,
        dialog: crate::app::Dialog::FireFlowCurve,
    },
];

pub(crate) fn core(name: &str) -> Option<&'static CorePlugin> {
    CORE.iter().find(|c| c.name == name)
}

const CORE_REPOSITORY: &str = "Core (part of WEPANET)";

fn core_offers() -> Vec<Offer> {
    CORE.iter()
        .map(|c| Offer {
            name: c.name.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: c.description.into(),
            author: "WEPANET (after Elad Salomons' EPANET-Plugins)".into(),
            tags: c.tags.into(),
            homepage: crate::plugins::PLUGINS_REPO.into(),
            url: format!("core:{}", c.name),
            repository: CORE_REPOSITORY.into(),
            ..Default::default()
        })
        .collect()
}

fn builtin_icon(url: &str) -> Option<&'static [u8]> {
    let path = url.strip_prefix("builtin:/")?.replace("%20", " ");
    BUILTIN_ICONS
        .iter()
        .find(|(f, _)| *f == path)
        .map(|(_, b)| *b)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

pub fn default_repositories() -> Vec<Repository> {
    vec![Repository {
        name: "WEPANET Plugins".into(),
        url: DEFAULT_REPOSITORY.into(),
        enabled: true,
    }]
}

/// A plugin a repository offers.
#[derive(Clone, Debug, Default, PartialEq)]
struct Offer {
    name: String,
    version: String,
    description: String,
    author: String,
    tags: String,
    homepage: String,
    /// From its `metadata.txt` (or the repository's XML).
    about: String,
    email: String,
    changelog: String,
    tracker: String,
    /// Where its code is.
    code: String,
    experimental: bool,
    deprecated: bool,
    /// Where to download it: a ZIP of the plugin's folder, the folder itself
    /// (holding `<name>.rhai` or `<name>.py`, and `icon.png`), or (in older
    /// lists, `<file>`) its script.
    url: String,
    /// The icon's URL (PNG, or the ZIP holding it), if the plugin has one.
    icon: String,
    repository: String,
}

impl Offer {
    /// Takes what a plugin's `metadata.txt` says of it (all but its name).
    fn add_meta(&mut self, m: &crate::plugin_meta::Metadata) {
        let set = |to: &mut String, v: &str| {
            if !v.trim().is_empty() {
                *to = v.to_string();
            }
        };
        set(&mut self.version, &m.version);
        set(&mut self.description, &m.description);
        set(&mut self.author, &m.author);
        set(&mut self.tags, &m.tags);
        set(&mut self.homepage, &m.homepage);
        set(&mut self.about, &m.about);
        set(&mut self.email, &m.email);
        set(&mut self.changelog, &m.changelog);
        set(&mut self.tracker, &m.tracker);
        set(&mut self.code, &m.repository);
        self.experimental |= m.experimental;
        self.deprecated |= m.deprecated;
    }
}

enum Reply {
    Repository {
        url: String,
        result: Result<Vec<Offer>, String>,
    },
    Plugin {
        offer: Box<Offer>,
        /// The file downloaded, its bytes, and (for a script) the
        /// `metadata.txt` of its folder.
        result: Result<(String, Vec<u8>, Option<String>), String>,
    },
    Icon {
        url: String,
        bytes: Option<Vec<u8>>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Tab {
    #[default]
    All,
    Installed,
    NotInstalled,
    Upgradeable,
    Settings,
}

#[derive(Default)]
pub struct ManagerState {
    offers: Vec<Offer>,
    /// Per repository URL: number of plugins, or the error reading it.
    read: Vec<(String, Result<usize, String>)>,
    pending: usize,
    synced: bool,
    tab: Tab,
    selected: Option<String>,
    filter: String,
    /// Plugins being downloaded.
    installing: Vec<String>,
    message: Option<(bool, String)>,
    new_name: String,
    new_url: String,
    inbox: Arc<Mutex<Vec<Reply>>>,
    /// Offered plugins' icons by URL (None: asked for, or could not be read).
    icons: std::collections::HashMap<String, Option<Vec<u8>>>,
    /// Decoded icons, by URL or by `installed:` + plugin name.
    textures: std::collections::HashMap<String, Option<egui::TextureHandle>>,
}

// ----- repositories -------------------------------------------------------------

/// `file` as a URL: as it is, or relative to the repository's XML.
fn resolve(base: &str, file: &str) -> String {
    let file = file.trim();
    if file.contains("://") {
        return file.to_string();
    }
    let dir = base.rsplit_once('/').map_or(base, |(d, _)| d);
    format!(
        "{dir}/{}",
        file.trim_start_matches("./").replace(' ', "%20")
    )
}

fn is_zip(url: &str) -> bool {
    url.to_ascii_lowercase().ends_with(".zip")
}

fn is_script(url: &str) -> bool {
    let l = url.to_ascii_lowercase();
    l.ends_with(".rhai") || l.ends_with(".py")
}

/// The scripts an offer may download: itself, or in its folder
/// `<name>.rhai`, then `<name>.py`.
fn script_urls(o: &Offer) -> Vec<String> {
    if is_script(&o.url) {
        return vec![o.url.clone()];
    }
    let n = o.name.replace(' ', "%20");
    ["rhai", "py"]
        .iter()
        .map(|e| format!("{}/{n}.{e}", o.url))
        .collect()
}

fn file_name(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or("plugin")
        .replace("%20", " ")
}

fn parse_repository(xml: &str, url: &str, repository: &str) -> Result<Vec<Offer>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("not a plugin list: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "plugins" {
        return Err("not a plugin list (no <plugins> element)".into());
    }
    let mut out = Vec::new();
    for p in root.children().filter(|n| n.has_tag_name("plugin")) {
        let child = |tag: &str| {
            p.children()
                .find(|n| n.has_tag_name(tag))
                .and_then(|n| n.text())
                .map(|t| t.trim().to_string())
                .unwrap_or_default()
        };
        let name = p.attribute("name").map(str::trim).unwrap_or_default();
        let download = match child("download") {
            d if d.is_empty() => child("file"),
            d => d,
        };
        if name.is_empty() || download.is_empty() {
            continue;
        }
        let download = resolve(url, download.trim_end_matches('/'));
        let icon = match child("icon") {
            i if !i.is_empty() => resolve(url, &i),
            _ if is_zip(&download) => download.clone(),
            _ if is_script(&download) => String::new(),
            _ => format!("{download}/icon.png"),
        };
        let flag = |tag: &str| child(tag).eq_ignore_ascii_case("true");
        out.push(Offer {
            name: name.to_string(),
            version: p.attribute("version").unwrap_or("").trim().to_string(),
            description: child("description"),
            author: match child("author") {
                a if a.is_empty() => child("author_name"),
                a => a,
            },
            tags: child("tags"),
            homepage: child("homepage"),
            about: child("about"),
            email: child("email"),
            changelog: child("changelog"),
            tracker: child("tracker"),
            code: child("repository"),
            experimental: flag("experimental"),
            deprecated: flag("deprecated"),
            url: download,
            icon,
            repository: repository.to_string(),
        });
    }
    Ok(out)
}

fn fetch(
    app: &EpanetApp,
    ctx: &egui::Context,
    url: String,
    done: impl FnOnce(Result<String, String>) -> Reply + Send + 'static,
) {
    let inbox = app.plugin_manager.inbox.clone();
    let ctx = ctx.clone();
    ehttp::fetch(ehttp::Request::get(&url), move |res| {
        let text = match res {
            Ok(r) if r.ok => Ok(crate::io::decode_text(&r.bytes)),
            Ok(r) => Err(format!("{} {}", r.status, r.status_text)),
            Err(e) => Err(e),
        };
        if let Ok(mut q) = inbox.lock() {
            q.push(done(text));
        }
        ctx.request_repaint();
    });
}

/// Reads every enabled repository again.
fn sync(app: &mut EpanetApp, ctx: &egui::Context) {
    let m = &mut app.plugin_manager;
    m.synced = true;
    m.offers = core_offers();
    m.offers.extend(builtin_offers());
    m.read.clear();
    m.message = None;
    let repos: Vec<Repository> = app
        .prefs
        .plugin_repos
        .iter()
        .filter(|r| r.enabled)
        .cloned()
        .collect();
    app.plugin_manager.pending = repos.len();
    for r in repos {
        let url = r.url.trim().to_string();
        let name = r.name.clone();
        let u = url.clone();
        fetch(app, ctx, url, move |text| Reply::Repository {
            url: u.clone(),
            result: text.and_then(|xml| parse_repository(&xml, &u, &name)),
        });
    }
}

fn install(app: &mut EpanetApp, ctx: &egui::Context, offer: &Offer) {
    if app.plugin_manager.installing.contains(&offer.name) {
        return;
    }
    if core(&offer.name).is_some() {
        // built in: installing puts it on the toolbar
        app.prefs.core_plugins.insert(offer.name.clone());
        app.plugin_manager.message = Some((
            true,
            format!(
                "\"{}\" was installed: it is on the Plugins toolbar.",
                offer.name
            ),
        ));
        return;
    }
    app.plugin_manager.installing.push(offer.name.clone());
    let scripts = script_urls(offer);
    if let Some((u, src)) = scripts
        .iter()
        .find_map(|u| builtin_source(u).map(|s| (u, s)))
    {
        reply(
            &app.plugin_manager.inbox,
            Reply::Plugin {
                offer: Box::new(offer.clone()),
                result: Ok((
                    file_name(u),
                    src.as_bytes().to_vec(),
                    builtin_meta(&offer.url).map(str::to_string),
                )),
            },
        );
        return;
    }
    let urls = if is_zip(&offer.url) {
        vec![offer.url.clone()]
    } else {
        scripts
    };
    download(
        app.plugin_manager.inbox.clone(),
        ctx.clone(),
        offer.clone(),
        urls,
    );
}

type Inbox = Arc<Mutex<Vec<Reply>>>;

fn reply(inbox: &Inbox, r: Reply) {
    if let Ok(mut q) = inbox.lock() {
        q.push(r);
    }
}

/// Downloads the first of `urls` that can be read: the offer's ZIP or
/// script, or the script its folder holds (and then its `metadata.txt`).
fn download(inbox: Inbox, ctx: egui::Context, offer: Offer, mut urls: Vec<String>) {
    let url = urls.remove(0);
    ehttp::fetch(ehttp::Request::get(&url), move |res| {
        match res {
            Ok(r) if r.ok && !is_zip(&offer.url) && !is_script(&offer.url) => {
                let meta = format!("{}/{}", offer.url, crate::plugin_meta::FILE);
                ehttp::fetch(ehttp::Request::get(&meta), move |res| {
                    let meta = res
                        .ok()
                        .filter(|m| m.ok)
                        .map(|m| crate::io::decode_text(&m.bytes));
                    reply(
                        &inbox,
                        Reply::Plugin {
                            offer: Box::new(offer),
                            result: Ok((file_name(&url), r.bytes, meta)),
                        },
                    );
                    ctx.request_repaint();
                });
                return;
            }
            Ok(r) if r.ok => reply(
                &inbox,
                Reply::Plugin {
                    offer: Box::new(offer),
                    result: Ok((file_name(&url), r.bytes, None)),
                },
            ),
            _ if !urls.is_empty() => return download(inbox, ctx, offer, urls),
            r => reply(
                &inbox,
                Reply::Plugin {
                    offer: Box::new(offer),
                    result: Err(match r {
                        Ok(r) => format!("{} {}", r.status, r.status_text),
                        Err(e) => e,
                    }),
                },
            ),
        }
        ctx.request_repaint();
    });
}

/// Asks for the icons of the offered plugins not read yet.
fn fetch_icons(app: &mut EpanetApp, ctx: &egui::Context) {
    let wanted: Vec<String> = app
        .plugin_manager
        .offers
        .iter()
        .map(|o| o.icon.clone())
        .filter(|u| !u.is_empty() && !app.plugin_manager.icons.contains_key(u))
        .collect();
    for url in wanted {
        if let Some(b) = builtin_icon(&url) {
            app.plugin_manager.icons.insert(url, Some(b.to_vec()));
            continue;
        }
        app.plugin_manager.icons.insert(url.clone(), None);
        let inbox = app.plugin_manager.inbox.clone();
        let ctx = ctx.clone();
        ehttp::fetch(ehttp::Request::get(&url), move |res| {
            let bytes = res.ok().filter(|r| r.ok).map(|r| r.bytes);
            // a ZIP's icon is inside it
            let bytes = if is_zip(&url) {
                bytes.and_then(|b| crate::scripting::read_zip(&b).ok()?.icon)
            } else {
                bytes
            };
            if let Ok(mut q) = inbox.lock() {
                q.push(Reply::Icon { url, bytes });
            }
            ctx.request_repaint();
        });
    }
}

/// Keeps the icon of an installed plugin with it (so it shows offline too).
fn keep_icon(app: &mut EpanetApp, name: &str, icon_url: &str) {
    let bytes = app.plugin_manager.icons.get(icon_url).cloned().flatten();
    set_icon(app, name, bytes);
}

/// Sets (or, with None, forgets) the icon of an installed plugin.
pub(crate) fn set_icon(app: &mut EpanetApp, name: &str, png: Option<Vec<u8>>) {
    match png {
        Some(b) => {
            app.prefs.plugin_icons.insert(name.to_string(), b);
        }
        None => {
            app.prefs.plugin_icons.remove(name);
        }
    }
    app.plugin_manager
        .textures
        .remove(&format!("installed:{name}"));
}

fn texture(
    app: &mut EpanetApp,
    ctx: &egui::Context,
    key: String,
    bytes: Option<&[u8]>,
) -> Option<egui::TextureId> {
    if let Some(t) = app.plugin_manager.textures.get(&key) {
        return t.as_ref().map(|t| t.id());
    }
    let bytes = bytes?;
    let t = image::load_from_memory(bytes).ok().map(|img| {
        let img = img.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        ctx.load_texture(
            format!("plugin-icon {key}"),
            color,
            egui::TextureOptions::LINEAR,
        )
    });
    let id = t.as_ref().map(|t| t.id());
    app.plugin_manager.textures.insert(key, t);
    id
}

/// The icon of an installed plugin, if its author gave it one.
pub(crate) fn installed_icon(
    app: &mut EpanetApp,
    ctx: &egui::Context,
    name: &str,
) -> Option<egui::TextureId> {
    let bytes = app.prefs.plugin_icons.get(name).cloned();
    texture(app, ctx, format!("installed:{name}"), bytes.as_deref())
}

/// A plugin's icon: its author's picture, or one of WEPANET's icons.
#[derive(Clone, Copy)]
pub(crate) enum Pic {
    Image(egui::TextureId),
    Glyph(Icon),
}

impl Pic {
    /// The author's picture, else the default plugin icon.
    pub(crate) fn or_default(tex: Option<egui::TextureId>) -> Pic {
        tex.map_or(Pic::Glyph(Icon::Setup), Pic::Image)
    }
}

/// The icon of a row of the list: a core plugin's, the repository's, else the
/// installed one's.
fn row_icon(app: &mut EpanetApp, ctx: &egui::Context, r: &Row) -> Pic {
    if let Some(c) = core(&r.name) {
        return Pic::Glyph(c.icon);
    }
    if let Some(o) = &r.offer
        && !o.icon.is_empty()
    {
        let bytes = app.plugin_manager.icons.get(&o.icon).cloned().flatten();
        if let Some(t) = texture(app, ctx, o.icon.clone(), bytes.as_deref()) {
            return Pic::Image(t);
        }
    }
    Pic::or_default(installed_icon(app, ctx, &r.name))
}

/// Draws a plugin icon centred at `c`.
pub(crate) fn paint_icon(p: &egui::Painter, pic: Pic, c: egui::Pos2, size: f32, fg: Color32) {
    match pic {
        Pic::Image(t) => {
            p.image(
                t,
                egui::Rect::from_center_size(c, egui::vec2(size, size)),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        Pic::Glyph(icon) => icons::paint(p, icon, c, size, fg),
    }
}

fn uninstall(app: &mut EpanetApp, name: &str) {
    app.prefs.core_plugins.remove(name);
    crate::scripting::remove(app, name);
    app.prefs.plugin_versions.remove(name);
    app.prefs.disabled_plugins.remove(name);
    app.prefs.plugin_icons.remove(name);
    app.plugin_manager
        .textures
        .remove(&format!("installed:{name}"));
    app.plugin_manager.message = Some((true, format!("\"{name}\" was uninstalled.")));
}

fn poll(app: &mut EpanetApp) {
    let replies: Vec<Reply> = app
        .plugin_manager
        .inbox
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default();
    for r in replies {
        match r {
            Reply::Repository { url, result } => {
                let m = &mut app.plugin_manager;
                m.pending = m.pending.saturating_sub(1);
                match result {
                    Ok(offers) => {
                        m.read.push((url, Ok(offers.len())));
                        for o in offers.into_iter().filter(|o| core(&o.name).is_none()) {
                            // the first repository to offer a name wins, unless a later one is newer
                            match m.offers.iter_mut().find(|x| x.name == o.name) {
                                Some(x) if version_cmp(&o.version, &x.version).is_gt() => *x = o,
                                Some(_) => {}
                                None => m.offers.push(o),
                            }
                        }
                        m.offers.sort_by_key(|o| o.name.to_ascii_lowercase());
                    }
                    Err(e) => m.read.push((url, Err(e))),
                }
            }
            Reply::Plugin { offer, result } => {
                app.plugin_manager.installing.retain(|n| n != &offer.name);
                let outcome = result.and_then(|(file, bytes, meta)| {
                    crate::scripting::install_file(app, &file, &bytes, meta.as_deref())
                });
                app.plugin_manager.message = Some(match outcome {
                    Ok(name) => {
                        app.prefs
                            .plugin_versions
                            .insert(name.clone(), offer.version.clone());
                        // a ZIP brought its own icon
                        if !is_zip(&offer.url) {
                            keep_icon(app, &name, &offer.icon);
                        }
                        app.status = format!("Plugin \"{name}\" {} installed", offer.version);
                        (
                            true,
                            format!(
                                "\"{name}\" {} was installed: it is on the Plugins toolbar.",
                                offer.version
                            ),
                        )
                    }
                    Err(e) => (false, format!("Could not install \"{}\": {e}", offer.name)),
                });
            }
            Reply::Icon { url, bytes } => {
                app.plugin_manager.icons.insert(url.clone(), bytes);
                app.plugin_manager.textures.remove(&url);
                // a plugin installed before its icon arrived
                let late: Vec<String> = app
                    .plugin_manager
                    .offers
                    .iter()
                    .filter(|o| o.icon == url && !app.prefs.plugin_icons.contains_key(&o.name))
                    .map(|o| o.name.clone())
                    .filter(|n| app.prefs.plugin_versions.contains_key(n))
                    .collect();
                for n in late {
                    keep_icon(app, &n, &url);
                }
            }
        }
    }
}

// ----- the window ----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    NotInstalled,
    Installed,
    Upgradeable,
}

fn state(app: &EpanetApp, installed: &[String], o: &Offer) -> State {
    if core(&o.name).is_some() {
        return if app.prefs.core_plugins.contains(&o.name) {
            State::Installed
        } else {
            State::NotInstalled
        };
    }
    if !installed.contains(&o.name) {
        return State::NotInstalled;
    }
    match app.prefs.plugin_versions.get(&o.name) {
        Some(v) if version_cmp(&o.version, v).is_le() => State::Installed,
        // newer, or installed from a file (version unknown)
        _ => State::Upgradeable,
    }
}

/// A row of the list: a repository's offer, or an installed plugin no
/// repository offers (loaded from a file).
struct Row {
    name: String,
    offer: Option<Offer>,
    state: State,
}

fn rows(app: &EpanetApp) -> Vec<Row> {
    let installed = crate::scripting::installed(app);
    let mut rows: Vec<Row> = app
        .plugin_manager
        .offers
        .iter()
        .map(|o| Row {
            name: o.name.clone(),
            offer: Some(o.clone()),
            state: state(app, &installed, o),
        })
        .collect();
    for n in installed {
        if !rows.iter().any(|r| r.name == n) {
            rows.push(Row {
                name: n,
                offer: None,
                state: State::Installed,
            });
        }
    }
    rows.sort_by_key(|r| r.name.to_ascii_lowercase());
    rows
}

const SIDEBAR_W: f32 = 168.0;
const BODY_H: f32 = 330.0;
/// Width of the page right of the sidebar.
const PAGE_W: f32 = 600.0;
const LIST_W: f32 = 220.0;

/// One entry of the sidebar; true when clicked.
fn side_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(SIDEBAR_W, 36.0), egui::Sense::click());
    let fg = Color32::from_gray(235);
    if selected {
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(40));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(80));
    }
    icons::paint(
        ui.painter(),
        icon,
        rect.left_center() + egui::vec2(20.0, 0.0),
        22.0,
        fg,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(42.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.5),
        fg,
    );
    resp.clicked()
}

pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    poll(app);
    fetch_icons(app, &ctx);
    if !app.plugin_manager.synced {
        sync(app, &ctx);
    }
    let all = rows(app);
    let upgradeable = all.iter().filter(|r| r.state == State::Upgradeable).count();
    if app.plugin_manager.tab == Tab::Upgradeable && upgradeable == 0 {
        app.plugin_manager.tab = Tab::All;
    }
    let mut close = false;
    ui.horizontal_top(|ui| {
        // ---- sidebar ----
        let (side, _) =
            ui.allocate_exact_size(egui::vec2(SIDEBAR_W, BODY_H + 44.0), egui::Sense::hover());
        ui.painter().rect_filled(side, 0.0, Color32::from_gray(64));
        let mut side_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(side)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        side_ui.spacing_mut().item_spacing.y = 0.0;
        let tab = app.plugin_manager.tab;
        let mut items = vec![
            (Tab::All, Icon::Summary, format!("All ({})", all.len())),
            (
                Tab::Installed,
                Icon::Setup,
                format!(
                    "Installed ({})",
                    all.iter()
                        .filter(|r| r.state != State::NotInstalled)
                        .count()
                ),
            ),
            (
                Tab::NotInstalled,
                Icon::Import,
                format!(
                    "Not installed ({})",
                    all.iter()
                        .filter(|r| r.state == State::NotInstalled)
                        .count()
                ),
            ),
        ];
        if upgradeable > 0 {
            items.push((
                Tab::Upgradeable,
                Icon::Redo,
                format!("Upgradeable ({upgradeable})"),
            ));
        }
        for (t, icon, label) in items {
            if side_item(&mut side_ui, icon, &label, tab == t) {
                app.plugin_manager.tab = t;
            }
        }
        if side_item(&mut side_ui, Icon::Open, "Install from ZIP", false) {
            crate::scripting::load(app, &ctx);
        }
        if side_item(
            &mut side_ui,
            Icon::Settings,
            "Settings",
            tab == Tab::Settings,
        ) {
            app.plugin_manager.tab = Tab::Settings;
        }
        ui.add_space(8.0);
        // ---- page ----
        ui.vertical(|ui| {
            ui.set_width(PAGE_W);
            if app.plugin_manager.tab == Tab::Settings {
                ui.set_min_height(BODY_H);
                settings(app, ui, &ctx);
            } else {
                list(app, ui, &ctx, all);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                status_line(app, ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("Close").min_size(egui::vec2(80.0, 26.0)))
                        .clicked()
                    {
                        close = true;
                    }
                    if ui
                        .add(egui::Button::new("Help").min_size(egui::vec2(80.0, 26.0)))
                        .clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(HELP));
                    }
                });
            });
        });
    });
    close
}

fn status_line(app: &EpanetApp, ui: &mut Ui) {
    let m = &app.plugin_manager;
    let red = Color32::from_rgb(0xc0, 0x30, 0x30);
    if m.pending > 0 {
        ui.spinner();
        ui.label("Reading the repositories…");
    } else if let Some((ok, text)) = &m.message {
        ui.label(RichText::new(text).color(if *ok { ui.visuals().text_color() } else { red }));
    } else {
        let failed = m.read.iter().filter(|(_, r)| r.is_err()).count();
        if failed > 0 {
            ui.label(
                RichText::new(format!(
                    "Could not read {failed} repositories (see Settings)"
                ))
                .color(red),
            )
            .on_hover_text("The plugins built into WEPANET are offered all the same");
        }
    }
}

fn list(app: &mut EpanetApp, ui: &mut Ui, ctx: &egui::Context, all: Vec<Row>) {
    let tab = app.plugin_manager.tab;
    ui.add(
        egui::TextEdit::singleline(&mut app.plugin_manager.filter)
            .desired_width(f32::INFINITY)
            .hint_text("🔍 Search name, description or tag…"),
    );
    ui.add_space(6.0);
    let filter = app.plugin_manager.filter.trim().to_ascii_lowercase();
    let shown: Vec<&Row> = all
        .iter()
        .filter(|r| match tab {
            Tab::Installed => r.state != State::NotInstalled,
            Tab::NotInstalled => r.state == State::NotInstalled,
            Tab::Upgradeable => r.state == State::Upgradeable,
            _ => true,
        })
        .filter(|r| {
            filter.is_empty()
                || r.name.to_ascii_lowercase().contains(&filter)
                || r.offer.as_ref().is_some_and(|o| {
                    o.description.to_ascii_lowercase().contains(&filter)
                        || o.tags.to_ascii_lowercase().contains(&filter)
                })
        })
        .collect();
    if app
        .plugin_manager
        .selected
        .as_ref()
        .is_none_or(|s| !shown.iter().any(|r| &r.name == s))
    {
        app.plugin_manager.selected = shown.first().map(|r| r.name.clone());
    }
    let selected = shown
        .iter()
        .find(|r| app.plugin_manager.selected.as_deref() == Some(&r.name));
    let mut act: Option<(bool, String)> = None; // (install, name)
    let list_h = BODY_H - 40.0;
    ui.horizontal_top(|ui| {
        // ---- the list ----
        egui::Frame::NONE
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(170)))
            .fill(ui.visuals().extreme_bg_color)
            .show(ui, |ui| {
                ui.set_width(LIST_W);
                egui::ScrollArea::vertical()
                    .id_salt("plugin_list")
                    .max_height(list_h)
                    .min_scrolled_height(list_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            if shown.is_empty() {
                                ui.label(RichText::new("  No plugins here.").weak());
                            }
                            for (k, r) in shown.iter().enumerate() {
                                plugin_row(app, ui, r, k);
                            }
                        });
                    });
            });
        // ---- the selected plugin ----
        egui::Frame::NONE
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(170)))
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(PAGE_W - LIST_W - 30.0);
                ui.set_height(list_h - 20.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    if let Some(r) = selected {
                        details(app, ui, r);
                    }
                });
            });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let upgrades: Vec<&Offer> = all
            .iter()
            .filter(|r| r.state == State::Upgradeable)
            .filter_map(|r| r.offer.as_ref())
            .collect();
        if ui
            .add_enabled(!upgrades.is_empty(), egui::Button::new("Upgrade All"))
            .clicked()
        {
            for o in upgrades.into_iter().cloned().collect::<Vec<_>>() {
                install(app, ctx, &o);
            }
        }
        let Some(r) = selected else { return };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.plugin_manager.installing.contains(&r.name) {
                ui.label("Downloading…");
                ui.spinner();
                return;
            }
            if r.offer.is_some() {
                let label = match r.state {
                    State::NotInstalled => "Install Plugin",
                    State::Upgradeable => "Upgrade Plugin",
                    State::Installed => "Reinstall Plugin",
                };
                if ui.button(label).clicked() {
                    act = Some((true, r.name.clone()));
                }
            }
            if ui
                .add_enabled(
                    r.state != State::NotInstalled,
                    egui::Button::new("Uninstall Plugin"),
                )
                .clicked()
            {
                act = Some((false, r.name.clone()));
            }
        });
    });
    match act {
        Some((true, name)) => {
            if let Some(o) = all
                .iter()
                .find(|r| r.name == name)
                .and_then(|r| r.offer.clone())
            {
                install(app, ctx, &o);
            }
        }
        Some((false, name)) => uninstall(app, &name),
        None => {}
    }
}

/// A line of the list: a check box (enabled) for installed plugins, the name.
fn plugin_row(app: &mut EpanetApp, ui: &mut Ui, r: &Row, k: usize) {
    let ctx = ui.ctx().clone();
    let tex = row_icon(app, &ctx, r);
    let sel = app.plugin_manager.selected.as_deref() == Some(&r.name);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    let v = ui.visuals();
    let fill = if sel {
        v.selection.bg_fill
    } else if k % 2 == 1 {
        v.faint_bg_color
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    let fg = if sel {
        v.selection.stroke.color
    } else {
        v.text_color()
    };
    // check box: installed and enabled
    let boxr = egui::Rect::from_center_size(
        rect.left_center() + egui::vec2(12.0, 0.0),
        egui::vec2(13.0, 13.0),
    );
    let installed = r.state != State::NotInstalled;
    let enabled = installed && !app.prefs.disabled_plugins.contains(&r.name);
    let box_resp = installed.then(|| {
        ui.interact(
            boxr,
            ui.id().with(("plugin_on", &r.name)),
            egui::Sense::click(),
        )
    });
    if installed {
        ui.painter().rect(
            boxr,
            2.0,
            Color32::WHITE,
            egui::Stroke::new(1.0, Color32::from_gray(120)),
            egui::StrokeKind::Inside,
        );
        if enabled {
            let s = egui::Stroke::new(1.8, Color32::from_gray(30));
            let c = boxr.center();
            ui.painter()
                .line_segment([c + egui::vec2(-4.0, 0.0), c + egui::vec2(-1.0, 3.0)], s);
            ui.painter()
                .line_segment([c + egui::vec2(-1.0, 3.0), c + egui::vec2(4.5, -3.5)], s);
        }
    }
    paint_icon(
        ui.painter(),
        tex,
        rect.left_center() + egui::vec2(35.0, 0.0),
        20.0,
        fg,
    );
    let mark = match r.state {
        State::Upgradeable => "⬆ ",
        _ => "",
    };
    ui.painter().text(
        rect.left_center() + egui::vec2(48.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("{mark}{}", r.name),
        egui::FontId::proportional(13.0),
        fg,
    );
    if let Some(b) = box_resp
        && b.clicked()
    {
        if enabled {
            app.prefs.disabled_plugins.insert(r.name.clone());
        } else {
            app.prefs.disabled_plugins.remove(&r.name);
        }
    } else if resp.clicked() {
        app.plugin_manager.selected = Some(r.name.clone());
    }
}

fn details(app: &mut EpanetApp, ui: &mut Ui, r: &Row) {
    let installed_version = app.prefs.plugin_versions.get(&r.name).cloned();
    let banner = |ui: &mut Ui, text: &str, fill: Color32| {
        egui::Frame::NONE
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).strong().color(Color32::from_gray(20)));
            });
        ui.add_space(6.0);
    };
    match r.state {
        State::Upgradeable if r.offer.is_some() => banner(
            ui,
            "There is a new version of this plugin",
            Color32::from_rgb(0xff, 0xe0, 0x8a),
        ),
        State::Installed if app.prefs.disabled_plugins.contains(&r.name) => banner(
            ui,
            "This plugin is installed but disabled (tick it in the list to enable it)",
            Color32::from_gray(215),
        ),
        _ if core(&r.name).is_some() => banner(
            ui,
            "This is a core plugin: part of WEPANET, nothing to download",
            Color32::from_rgb(0x9e, 0xe6, 0xe6),
        ),
        _ => {}
    }
    // a plugin installed from a file: what its metadata.txt says
    let from_file = r.offer.is_none();
    let offer = r.offer.clone().or_else(|| {
        let m = app.prefs.plugin_meta.get(&r.name)?;
        let mut o = Offer {
            name: r.name.clone(),
            ..Default::default()
        };
        o.add_meta(m);
        Some(o)
    });
    if let Some(o) = &offer {
        if o.deprecated {
            banner(
                ui,
                "This plugin is deprecated",
                Color32::from_rgb(0xff, 0xb0, 0xa8),
            );
        } else if o.experimental {
            banner(
                ui,
                "This plugin is experimental",
                Color32::from_rgb(0xff, 0xd0, 0xa0),
            );
        }
    }
    let tex = row_icon(app, &ui.ctx().clone(), r);
    ui.horizontal(|ui| {
        let (icon, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
        let fg = ui.visuals().text_color();
        paint_icon(ui.painter(), tex, icon.center(), 40.0, fg);
        ui.label(RichText::new(&r.name).strong().size(24.0));
    });
    ui.add_space(6.0);
    let Some(o) = &offer else {
        ui.label(RichText::new("Installed from a file; no repository offers this plugin.").weak());
        return;
    };
    egui::ScrollArea::vertical()
        .id_salt("plugin_details")
        .auto_shrink([false, true])
        .show(ui, |ui| info(ui, r, o, from_file, installed_version));
}

/// The description, metadata and versions of a plugin.
fn info(ui: &mut Ui, r: &Row, o: &Offer, from_file: bool, installed_version: Option<String>) {
    if !o.description.is_empty() {
        ui.label(RichText::new(&o.description).strong().size(15.0));
        ui.add_space(6.0);
    }
    if !o.about.is_empty() {
        ui.label(&o.about);
        ui.add_space(6.0);
    }
    if from_file {
        ui.label(RichText::new("Installed from a file; no repository offers this plugin.").weak());
        ui.add_space(6.0);
    }
    let download =
        !(o.url.is_empty() || o.url.starts_with("builtin:") || o.url.starts_with("core:"));
    egui::Grid::new("plugin_info")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            let key = |ui: &mut Ui, k: &str| {
                ui.label(RichText::new(k).strong());
            };
            if !o.tags.is_empty() {
                key(ui, "Tags");
                ui.label(&o.tags);
                ui.end_row();
            }
            let links = [
                ("homepage", &o.homepage),
                ("bug tracker", &o.tracker),
                ("code repository", &o.code),
                ("download", &o.url),
            ];
            let links: Vec<_> = links
                .into_iter()
                .filter(|(k, u)| !u.is_empty() && (*k != "download" || download))
                .collect();
            if !links.is_empty() {
                key(ui, "More info");
                ui.horizontal_wrapped(|ui| {
                    for (k, u) in links {
                        ui.hyperlink_to(k, u);
                    }
                });
                ui.end_row();
            }
            if !o.author.is_empty() {
                key(ui, "Author");
                ui.add(egui::Label::new(&o.author).wrap());
                ui.end_row();
            }
            if !o.email.is_empty() {
                key(ui, "Email");
                ui.hyperlink_to(&o.email, format!("mailto:{}", o.email));
                ui.end_row();
            }
            match (r.state, &installed_version) {
                (State::NotInstalled, _) => {}
                (_, Some(v)) => {
                    key(ui, "Installed version");
                    ui.label(v);
                    ui.end_row();
                }
                (_, None) => {
                    key(ui, "Installed version");
                    ui.label("from a file");
                    ui.end_row();
                }
            }
            if !from_file {
                key(ui, "Available version");
                ui.label(&o.version);
                ui.end_row();
                key(ui, "Repository");
                ui.label(&o.repository);
                ui.end_row();
            }
        });
    if !o.changelog.is_empty() {
        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new("Changelog").strong())
            .id_salt(("plugin_changelog", &o.name))
            .show(ui, |ui| ui.label(&o.changelog));
    }
}

fn settings(app: &mut EpanetApp, ui: &mut Ui, ctx: &egui::Context) {
    ui.label(RichText::new("Plugin Repositories").strong().size(15.0));
    ui.label(
        RichText::new(
            "XML files listing plugins (see plugins/plugins.xml in the WEPANET project for the \
format). The plugins of all enabled repositories are offered, along with those built into WEPANET.",
        )
        .weak(),
    );
    ui.add_space(8.0);
    let mut remove = None;
    let mut changed = false;
    let read = app.plugin_manager.read.clone();
    let red = Color32::from_rgb(0xc0, 0x30, 0x30);
    for (k, r) in app.prefs.plugin_repos.iter_mut().enumerate() {
        egui::Frame::NONE
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(190)))
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    changed |= ui
                        .checkbox(&mut r.enabled, RichText::new(&r.name).strong())
                        .changed();
                    match read.iter().find(|(u, _)| u == r.url.trim()) {
                        Some((_, Ok(n))) => {
                            ui.label(RichText::new(format!("✔ {n} plugins")).weak());
                        }
                        Some((_, Err(e))) => {
                            ui.label(RichText::new(format!("✖ could not read it: {e}")).color(red));
                        }
                        None if !r.enabled => {
                            ui.label(RichText::new("disabled").weak());
                        }
                        None => {}
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Remove").clicked() {
                            remove = Some(k);
                        }
                    });
                });
                ui.add(egui::Label::new(RichText::new(&r.url).monospace().size(11.5)).truncate())
                    .on_hover_text(&r.url);
            });
        ui.add_space(4.0);
    }
    if let Some(k) = remove {
        app.prefs.plugin_repos.remove(k);
        changed = true;
    }
    ui.add_space(10.0);
    ui.label(RichText::new("Add a Repository").strong());
    let m = &mut app.plugin_manager;
    egui::Grid::new("add_repo")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut m.new_name)
                    .desired_width(200.0)
                    .hint_text("optional"),
            );
            ui.end_row();
            ui.label("URL:");
            ui.add(
                egui::TextEdit::singleline(&mut m.new_url)
                    .desired_width(f32::INFINITY)
                    .hint_text("https://…/plugins.xml"),
            );
            ui.end_row();
        });
    let url = m.new_url.trim().to_string();
    let ok = url.starts_with("http://") || url.starts_with("https://");
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.add_enabled(ok, egui::Button::new("Add")).clicked() {
            let name = match app.plugin_manager.new_name.trim() {
                "" => url.rsplit('/').nth(1).unwrap_or("Repository").to_string(),
                n => n.to_string(),
            };
            app.prefs.plugin_repos.retain(|r| r.url.trim() != url);
            app.prefs.plugin_repos.push(Repository {
                name,
                url,
                enabled: true,
            });
            app.plugin_manager.new_name.clear();
            app.plugin_manager.new_url.clear();
            changed = true;
        }
        if ui.button("Reload All Repositories").clicked() {
            changed = true;
        }
        if app.prefs.plugin_repos != default_repositories()
            && ui.button("Restore Default").clicked()
        {
            app.prefs.plugin_repos = default_repositories();
            changed = true;
        }
    });
    if changed {
        sync(app, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = include_str!("../../../plugins/plugins.xml");

    #[test]
    fn reads_the_project_repository() {
        let offers = parse_repository(XML, DEFAULT_REPOSITORY, "WEPANET Plugins").unwrap();
        assert!(offers.len() >= 6);
        let d = offers.iter().find(|o| o.name == "Dead Ends").unwrap();
        assert_eq!(d.version, "1.0");
        assert_eq!(
            d.url,
            "https://raw.githubusercontent.com/KIOS-Research/WEPANET/main/plugins/Dead%20Ends"
        );
        assert_eq!(d.icon, format!("{}/icon.png", d.url));
        // every plugin listed is in the project, and calls itself by the listed name
        for o in &offers {
            let (_, src) = BUILTIN
                .iter()
                .find(|(f, _)| {
                    let f = f.replace(' ', "%20");
                    script_urls(o)
                        .iter()
                        .any(|u| u.ends_with(&format!("/plugins/{f}")))
                })
                .unwrap_or_else(|| panic!("{} is not in plugins/", o.url));
            assert!(
                src.contains(&format!("fn name() {{ \"{}\" }}", o.name))
                    || src.contains(&format!("NAME = \"{}\"", o.name)),
                "{}",
                o.name
            );
        }
    }

    #[test]
    fn repository_errors_and_urls() {
        assert!(parse_repository("<html></html>", "u", "r").is_err());
        assert!(parse_repository("not xml", "u", "r").is_err());
        assert_eq!(
            resolve("https://a.org/x/plugins.xml", "p/a b.rhai"),
            "https://a.org/x/p/a%20b.rhai"
        );
        assert_eq!(
            resolve("https://a.org/x/plugins.xml", "https://b.org/c.rhai"),
            "https://b.org/c.rhai"
        );
    }

    #[test]
    fn downloads_a_zip_a_folder_or_a_script() {
        let xml = r#"<plugins>
            <plugin name="A b" version="1"><download>A b/</download></plugin>
            <plugin name="Z" version="1"><download>https://x.org/z.zip</download></plugin>
            <plugin name="Old" version="1"><file>old.rhai</file><icon>old.png</icon></plugin>
            <plugin name="None" version="1"></plugin>
        </plugins>"#;
        let o = parse_repository(xml, "https://r.org/p/plugins.xml", "r").unwrap();
        assert_eq!(o.len(), 3);
        assert_eq!(o[0].url, "https://r.org/p/A%20b");
        assert_eq!(o[0].icon, "https://r.org/p/A%20b/icon.png");
        assert_eq!(
            script_urls(&o[0]),
            [
                "https://r.org/p/A%20b/A%20b.rhai",
                "https://r.org/p/A%20b/A%20b.py"
            ]
        );
        assert_eq!(file_name(&script_urls(&o[0])[1]), "A b.py");
        assert_eq!(o[1].icon, "https://x.org/z.zip");
        assert_eq!(script_urls(&o[2]), ["https://r.org/p/old.rhai"]);
        assert_eq!(o[2].icon, "https://r.org/p/old.png");
    }

    #[test]
    fn every_plugin_has_an_icon_built_in() {
        for o in builtin_offers() {
            assert!(!o.icon.is_empty(), "{} has no <icon>", o.name);
            let png = builtin_icon(&o.icon).unwrap_or_else(|| panic!("{} not built in", o.icon));
            let img = image::load_from_memory(png).unwrap();
            assert_eq!((img.width(), img.height()), (64, 64), "{}", o.icon);
            assert!(
                script_urls(&o).iter().any(|u| builtin_source(u).is_some()),
                "{}",
                o.url
            );
        }
    }

    /// Each plugin's metadata.txt is read, and agrees with plugins.xml.
    #[test]
    fn every_plugin_has_its_metadata() {
        let listed = parse_repository(BUILTIN_XML, BUILTIN_BASE, BUILTIN_NAME).unwrap();
        assert_eq!(listed.len(), BUILTIN_META.len());
        for o in &listed {
            let text = builtin_meta(&o.url).unwrap_or_else(|| panic!("{} has none", o.name));
            let m = crate::plugin_meta::parse(text).unwrap();
            assert_eq!(
                (&m.name, &m.version, &m.description, &m.author),
                (&o.name, &o.version, &o.description, &o.author),
                "{}/metadata.txt and plugins.xml differ",
                o.name
            );
            assert!(m.check_version().is_ok(), "{}", o.name);
            assert!(!m.about.is_empty() && !m.changelog.is_empty(), "{}", o.name);
        }
        // the offers carry it
        let d = builtin_offers()
            .into_iter()
            .find(|o| o.name == "Dead Ends")
            .unwrap();
        assert!(d.about.contains("stagnate") && d.tracker.ends_with("/issues"));
        // and what an XML says
        let xml = r#"<plugins><plugin name="Q" version="2"><download>q.zip</download>
            <author_name>Me</author_name><about>Long</about><tracker>t</tracker>
            <repository>c</repository><experimental>True</experimental></plugin></plugins>"#;
        let q = &parse_repository(xml, "https://r.org/plugins.xml", "r").unwrap()[0];
        assert_eq!((q.author.as_str(), q.about.as_str()), ("Me", "Long"));
        assert_eq!((q.tracker.as_str(), q.code.as_str()), ("t", "c"));
        assert!(q.experimental && !q.deprecated);
    }

    #[test]
    fn installs_upgrades_and_uninstalls() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        // from the built-in repository: no network needed
        app.plugin_manager.offers = builtin_offers();
        fetch_icons(&mut app, &ctx);
        let mut o = builtin_offers()
            .into_iter()
            .find(|o| o.name == "Dead Ends")
            .unwrap();
        install(&mut app, &ctx, &o);
        poll(&mut app);
        let installed = crate::scripting::installed(&app);
        assert!(installed.contains(&"Dead Ends".to_string()));
        assert_eq!(state(&app, &installed, &o), State::Installed);
        // its icon is kept with it, for the toolbar, and its metadata
        assert!(app.prefs.plugin_icons.contains_key("Dead Ends"));
        assert_eq!(app.prefs.plugin_meta["Dead Ends"].version, "1.0");
        let mut out = ctx.run_ui(Default::default(), |_| {
            assert!(installed_icon(&mut app, &ctx, "Dead Ends").is_some());
            assert!(installed_icon(&mut app, &ctx, "Count").is_none());
        });
        out.textures_delta.clear();
        o.version = "1.1".into();
        assert_eq!(state(&app, &installed, &o), State::Upgradeable);
        uninstall(&mut app, "Dead Ends");
        let installed = crate::scripting::installed(&app);
        assert_eq!(state(&app, &installed, &o), State::NotInstalled);
        assert!(!app.prefs.plugin_versions.contains_key("Dead Ends"));
        assert!(!app.prefs.plugin_icons.contains_key("Dead Ends"));
        assert!(!app.prefs.plugin_meta.contains_key("Dead Ends"));
    }

    /// The window as drawn: each piece of text and where it is.
    fn layout(app: &mut EpanetApp, ctx: &egui::Context) -> Vec<(String, egui::Pos2)> {
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut texts = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(input(), |ui| crate::dialogs::show(app, ui.ctx()));
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos)),
                    _ => None,
                })
                .collect();
        }
        texts
    }

    #[test]
    fn window_is_laid_out() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.prefs.plugin_repos.clear(); // built-in plugins only: no network
        app.dialog = crate::app::Dialog::PluginManager;
        let t = layout(&mut app, &ctx);
        let at = |s: &str| {
            t.iter()
                .find(|(x, _)| x == s)
                .map(|(_, p)| *p)
                .unwrap_or_else(|| panic!("no {s:?} in {t:?}"))
        };
        // the sidebar: one entry below the other
        let side = [
            "All (9)",
            "Installed (0)",
            "Not installed (9)",
            "Install from ZIP",
            "Settings",
        ]
        .map(at);
        for w in side.windows(2) {
            assert!(
                (w[0].x - w[1].x).abs() < 1.0 && w[1].y > w[0].y + 20.0,
                "{side:?}"
            );
        }
        // the list: one plugin below the other, right of the sidebar
        let list = ["Count", "Dead Ends", "High Velocity", "Low Pressure"].map(at);
        for w in list.windows(2) {
            assert!(
                (w[0].x - w[1].x).abs() < 1.0 && w[1].y > w[0].y + 15.0,
                "{list:?}"
            );
        }
        assert!(list[0].x > side[0].x + 120.0);
        // the details right of the list, the buttons below; a window of sensible width
        let tags = at("Tags");
        assert!(tags.x > list[0].x + 150.0, "{tags:?}");
        // labels and their values side by side, not on top of each other
        let author = at("Author");
        assert!(
            t.iter()
                .any(|(_, p)| (p.y - author.y).abs() < 4.0 && p.x > author.x + 50.0),
            "no value beside {author:?}"
        );
        let close = at("Close");
        assert!(
            close.y > list[3].y && close.x - side[0].x < 800.0,
            "{close:?} {side:?}"
        );
        // Settings: the URL on one line
        app.plugin_manager.tab = Tab::Settings;
        app.prefs.plugin_repos = default_repositories();
        app.plugin_manager.synced = true; // do not go online
        let t = layout(&mut app, &ctx);
        assert!(
            t.iter()
                .any(|(s, _)| s.contains("raw.githubusercontent.com/KIOS-Research")),
            "{t:?}"
        );
    }

    #[test]
    fn core_plugins_install_onto_the_toolbar() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let offers = core_offers();
        assert_eq!(offers.len(), CORE.len());
        let installed = crate::scripting::installed(&app);
        let e = offers.iter().find(|o| o.name == "Elevations").unwrap();
        assert_eq!(state(&app, &installed, e), State::NotInstalled);
        install(&mut app, &ctx, e);
        assert!(app.prefs.core_plugins.contains("Elevations"));
        assert_eq!(state(&app, &installed, e), State::Installed);
        // a repository cannot replace a core plugin
        app.plugin_manager.offers = core_offers();
        app.plugin_manager
            .inbox
            .lock()
            .unwrap()
            .push(Reply::Repository {
                url: "u".into(),
                result: Ok(vec![Offer {
                    version: "99".into(),
                    url: "https://x/evil.rhai".into(),
                    ..e.clone()
                }]),
            });
        poll(&mut app);
        let now = app
            .plugin_manager
            .offers
            .iter()
            .find(|o| o.name == "Elevations")
            .unwrap();
        assert!(now.url.starts_with("core:"));
        uninstall(&mut app, "Elevations");
        assert!(!app.prefs.core_plugins.contains("Elevations"));
    }
}
