//! Python plugins: a `.py` file that declares its name and commands and
//! defines `run(item)`, as a Rhai plugin does:
//!
//! ```python
//! NAME = "Pressure Check"
//! DESCRIPTION = "Junctions below a pressure limit"
//! MENU = ["Below 20", "Below 40"]
//! REQUIREMENTS = ["epyt"]          # PyPI packages it needs
//!
//! import wepanet
//!
//! def run(item):
//!     wepanet.message(f"{len(wepanet.node_ids())} nodes")
//! ```
//!
//! On the desktop they run with the local Python (`WEPANET_PYTHON`, else
//! `python`, `python3` or `py` on the PATH), in the browser with Pyodide in a
//! Web Worker (`apps/web/engine/python.js`). Either way `python_runner.py`
//! runs the plugin in a folder holding the project (`network.inp`), the
//! results of the last analysis and, in `plugin/`, the rest of the plugin's
//! own folder (its modules, translations, …: see `wepanet.plugin_dir`), and answers with what to show; a changed
//! project replaces the current one in a step Undo can take back.

use std::sync::mpsc;

use eui_core::props::Element;
use eui_core::{LinkVar, NodeVar};
use serde::Deserialize;

use crate::app::{Dialog, EpanetApp};
use crate::scripting::UserPlugin;

const RUNNER: &str = include_str!("python_runner.py");

const NODE_VARS: [NodeVar; 4] = [
    NodeVar::Demand,
    NodeVar::Head,
    NodeVar::Pressure,
    NodeVar::Quality,
];
const LINK_VARS: [LinkVar; 8] = [
    LinkVar::Flow,
    LinkVar::Velocity,
    LinkVar::Headloss,
    LinkVar::Quality,
    LinkVar::Status,
    LinkVar::Setting,
    LinkVar::Reaction,
    LinkVar::Friction,
];

/// Whether a plugin file is a Python one.
pub fn is_python(file: &str) -> bool {
    file.to_ascii_lowercase().ends_with(".py")
}

/// What the runner answers.
#[derive(Debug, Default, Deserialize)]
struct Response {
    #[serde(default)]
    messages: Vec<String>,
    status: Option<String>,
    table: Option<Table>,
    select: Option<Select>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Table {
    title: String,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Select {
    node: Option<String>,
    link: Option<String>,
}

/// The runner's answer and the project it left (when it changed it).
type Outcome = Result<(Response, Option<String>), String>;

enum Msg {
    Progress(String),
    Done(Outcome),
}

/// A plugin running in Python.
struct Job {
    plugin: String,
    rx: mpsc::Receiver<Msg>,
}

#[derive(Default)]
pub struct PythonState {
    job: Option<Job>,
}

// ----- reading a plugin's header ---------------------------------------------

/// A Python string literal ('…' or "…", with simple escapes) at the start of
/// `s`, and the rest of `s`.
fn string_literal(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let q = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let mut out = String::new();
    let mut chars = s[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                c => out.push(c),
            },
            c if c == q => return Some((out, &s[1 + i + 1..])),
            c => out.push(c),
        }
    }
    None
}

/// A list of string literals, e.g. `["A", 'B']`.
fn string_list(s: &str) -> Option<Vec<String>> {
    let mut rest = s.trim_start().strip_prefix('[')?;
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix(']') {
            let _ = r;
            return Some(out);
        }
        let (v, r) = string_literal(rest)?;
        out.push(v);
        rest = r.trim_start();
        rest = rest.strip_prefix(',').unwrap_or(rest);
    }
}

/// The value of a top-level `KEY = …` of the source (the text after `=`,
/// up to the closing bracket of a list).
fn assignment<'a>(source: &'a str, key: &str) -> Option<&'a str> {
    let mut lines = source.lines().enumerate();
    let start = lines.find_map(|(k, l)| {
        let rest = l.strip_prefix(key)?.trim_start();
        let v = rest.strip_prefix('=')?;
        (!v.starts_with('=')).then_some(k)
    })?;
    // from the `=` of that line to the end of the source
    let offset: usize = source.lines().take(start).map(|l| l.len() + 1).sum();
    let from = &source[offset.min(source.len())..];
    let eq = from.find('=')?;
    Some(&from[eq + 1..])
}

/// Reads a Python plugin's NAME, DESCRIPTION and MENU.
pub fn describe(file: &str, source: &str) -> Result<UserPlugin, String> {
    let text = |key: &str| {
        assignment(source, key)
            .and_then(string_literal)
            .map(|v| v.0)
    };
    let name = text("NAME").ok_or("the plugin has no NAME = \"…\"")?;
    let menu = match assignment(source, "MENU") {
        Some(v) => string_list(v).ok_or("MENU must be a list of strings, e.g. [\"Run\"]")?,
        None => vec!["Run".into()],
    };
    if !source.lines().any(|l| l.starts_with("def run(")) {
        return Err("the plugin has no def run(item)".into());
    }
    Ok(UserPlugin {
        name,
        description: text("DESCRIPTION").unwrap_or_default(),
        menu,
        file: file.to_string(),
        source: source.to_string(),
    })
}

/// The PyPI packages a plugin asks for (REQUIREMENTS).
fn requirements(source: &str) -> Vec<String> {
    assignment(source, "REQUIREMENTS")
        .and_then(string_list)
        .unwrap_or_default()
}

// ----- running -----------------------------------------------------------------

/// The files of a run: the runner, the plugin, the request, the project and
/// the results of its last analysis.
fn files(app: &EpanetApp, p: &UserPlugin, item: &str) -> Vec<(String, Vec<u8>)> {
    let net = &app.net;
    let mut request = serde_json::json!({
        "name": p.name,
        "file": p.file,
        "item": item,
        "period": app.period,
        "flow_units": net.flow_units(),
        "node_ids": net.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
        "node_types": net.nodes.iter().map(|n| n.kind.label()).collect::<Vec<_>>(),
        "link_ids": net.links.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
        "link_types": net.links.iter().map(|l| l.kind.label()).collect::<Vec<_>>(),
        "results": null,
    });
    let mut out = vec![
        ("python_runner.py".to_string(), RUNNER.as_bytes().to_vec()),
        ("plugin.py".to_string(), p.source.as_bytes().to_vec()),
        ("network.inp".to_string(), net.to_inp().into_bytes()),
    ];
    // the rest of the plugin's folder (its modules, translations, …)
    for (path, bytes) in app.prefs.plugin_files.get(&p.name).into_iter().flatten() {
        out.push((format!("plugin/{path}"), bytes.clone()));
    }
    if let Some(r) = app.results.as_ref().filter(|_| !app.results_stale()) {
        request["results"] = serde_json::json!({
            "periods": r.n_periods,
            "times": (0..r.n_periods).map(|p| r.period_time(p)).collect::<Vec<_>>(),
            "node_ids": r.node_ids,
            "link_ids": r.link_ids,
        });
        // per period: the 4 node results of every node, then the 8 link
        // results of every link, as little-endian f32
        let mut blob = Vec::new();
        for p in 0..r.n_periods {
            for v in NODE_VARS {
                blob.extend(r.node_slice(p, v).iter().flat_map(|x| x.to_le_bytes()));
            }
            for v in LINK_VARS {
                blob.extend(r.link_slice(p, v).iter().flat_map(|x| x.to_le_bytes()));
            }
        }
        out.push(("results.bin".to_string(), blob));
    }
    out.push(("request.json".to_string(), request.to_string().into_bytes()));
    out
}

/// Starts a Python plugin's `run(item)`; what it shows comes with [`poll`].
pub fn start(app: &mut EpanetApp, p: &UserPlugin, item: &str) {
    if let Some(job) = &app.python.job {
        app.status = format!("\"{}\" is still running", job.plugin);
        return;
    }
    let (tx, rx) = mpsc::channel();
    let files = files(app, p, item);
    backend::run(files, requirements(&p.source), tx);
    app.status = format!("Running \"{}\"…", p.name);
    app.python.job = Some(Job {
        plugin: p.name.clone(),
        rx,
    });
}

/// Shows the progress and the outcome of the running plugin.
pub fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(job) = &app.python.job else {
        return;
    };
    let name = job.plugin.clone();
    let mut done = None;
    loop {
        match job.rx.try_recv() {
            Ok(Msg::Progress(t)) => app.status = format!("{name}: {t}"),
            Ok(Msg::Done(o)) => {
                done = Some(o);
                break;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                done = Some(Err("the Python plugin stopped unexpectedly".into()));
                break;
            }
        }
    }
    let Some(outcome) = done else {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        return;
    };
    app.python.job = None;
    app.status = format!("\"{name}\" finished");
    apply(app, &name, outcome);
}

/// Shows what a plugin answered and takes the project it changed.
fn apply(app: &mut EpanetApp, name: &str, outcome: Outcome) {
    let (r, network) = match outcome {
        Ok(x) => x,
        Err(e) => {
            app.plugins.message = format!("\"{name}\" could not run:\n{e}");
            app.dialog = Dialog::PluginMessage;
            return;
        }
    };
    let mut messages = r.messages;
    if let Some(e) = &r.error {
        messages.push(format!("The plugin stopped with an error:\n{e}"));
    }
    if r.error.is_none()
        && let Some(inp) = network
    {
        match eui_core::Network::from_inp(&inp) {
            Ok(net) => {
                app.checkpoint();
                app.net = net;
                app.status = format!("{name} changed the project (Undo reverts it)");
            }
            Err(e) => messages.push(format!("The project the plugin left cannot be read: {e}")),
        }
    }
    if let Some(s) = r.status {
        app.status = s;
    }
    if let Some(s) = r.select {
        let e = match (s.node, s.link) {
            (Some(id), _) => app.net.node_index(&id).map(Element::Node),
            (_, Some(id)) => app.net.link_index(&id).map(Element::Link),
            _ => None,
        };
        if e.is_some() {
            app.select(e);
        }
    }
    if let Some(t) = r.table {
        app.scripts.table = Some((t.title, t.rows));
        app.dialog = Dialog::PluginTable;
        if !messages.is_empty() {
            app.status = messages.join(" ").chars().take(200).collect();
        }
    } else if !messages.is_empty() {
        app.plugins.message = messages.join("\n");
        app.dialog = Dialog::PluginMessage;
    }
}

/// Reads the runner's answer from its folder.
#[cfg(not(target_arch = "wasm32"))]
fn read_outcome(dir: &std::path::Path, log: &str) -> Outcome {
    let text = std::fs::read_to_string(dir.join("response.json")).map_err(|_| {
        let log = log.trim();
        if log.is_empty() {
            "Python stopped without an answer".to_string()
        } else {
            log.to_string()
        }
    })?;
    parse_outcome(
        &text,
        std::fs::read_to_string(dir.join("network_out.inp")).ok(),
    )
}

fn parse_outcome(response: &str, network: Option<String>) -> Outcome {
    let r: Response =
        serde_json::from_str(response).map_err(|e| format!("a bad answer from Python: {e}"))?;
    Ok((r, network))
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    //! The local Python, in a thread.

    use std::io::BufRead as _;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{OnceLock, atomic};

    use super::{Msg, mpsc};

    fn command(program: &[String]) -> Command {
        let mut c = Command::new(&program[0]);
        c.args(&program[1..]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            // no console window
            c.creation_flags(0x0800_0000);
        }
        c
    }

    /// The Python to run plugins with: `WEPANET_PYTHON`, else the first of
    /// python, python3 and py that runs.
    fn python() -> Result<Vec<String>, String> {
        static FOUND: OnceLock<Option<Vec<String>>> = OnceLock::new();
        let found = FOUND.get_or_init(|| {
            let mut candidates: Vec<Vec<String>> = Vec::new();
            if let Ok(p) = std::env::var("WEPANET_PYTHON")
                && !p.trim().is_empty()
            {
                candidates.push(vec![p]);
            }
            for p in [&["python"][..], &["python3"], &["py", "-3"]] {
                candidates.push(p.iter().map(|s| s.to_string()).collect());
            }
            candidates.into_iter().find(|p| {
                command(p)
                    .args(["-c", "import sys; print(sys.version_info[0])"])
                    .stdin(Stdio::null())
                    .output()
                    .is_ok_and(|o| o.status.success() && o.stdout.starts_with(b"3"))
            })
        });
        found.clone().ok_or_else(|| {
            "No Python 3 was found. Install Python (python.org) or set the \
             WEPANET_PYTHON environment variable to the python executable; \
             in the browser, WEPANET runs Python plugins itself."
                .to_string()
        })
    }

    fn folder() -> Result<PathBuf, String> {
        static NEXT: atomic::AtomicU32 = atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("wepanet-py-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    fn write(dir: &Path, files: &[(String, Vec<u8>)]) -> Result<(), String> {
        for (name, bytes) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{name}: {e}"))?;
            }
            std::fs::write(path, bytes).map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    }

    pub fn run(files: Vec<(String, Vec<u8>)>, _requirements: Vec<String>, tx: mpsc::Sender<Msg>) {
        std::thread::spawn(move || {
            let outcome = (|| {
                let py = python()?;
                let dir = folder()?;
                write(&dir, &files)?;
                let mut child = command(&py)
                    .arg("-u")
                    .arg(dir.join("python_runner.py"))
                    .arg(&dir)
                    .current_dir(&dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("{}: {e}", py.join(" ")))?;
                let stderr = child.stderr.take();
                let log = std::thread::spawn(move || {
                    let mut s = String::new();
                    if let Some(mut e) = stderr {
                        let _ = std::io::Read::read_to_string(&mut e, &mut s);
                    }
                    s
                });
                if let Some(out) = child.stdout.take() {
                    for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                        if let Some(t) = line.strip_prefix("@@progress ") {
                            let _ = tx.send(Msg::Progress(t.to_string()));
                        }
                    }
                }
                let _ = child.wait();
                let log = log.join().unwrap_or_default();
                let outcome = super::read_outcome(&dir, &log);
                let _ = std::fs::remove_dir_all(&dir);
                outcome
            })();
            let _ = tx.send(Msg::Done(outcome));
        });
    }
}

#[cfg(target_arch = "wasm32")]
mod backend {
    //! Pyodide in a Web Worker (`apps/web/engine/python.js`).

    use js_sys::{Array, Object, Reflect, Uint8Array};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    use super::{Msg, mpsc};

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(catch, js_namespace = euiPython, js_name = run)]
        fn js_run(
            files: &Object,
            requirements: &Array,
            on_progress: &Closure<dyn FnMut(String)>,
        ) -> Result<js_sys::Promise, JsValue>;
    }

    fn text(v: &JsValue) -> String {
        v.as_string().unwrap_or_else(|| format!("{v:?}"))
    }

    pub fn run(files: Vec<(String, Vec<u8>)>, requirements: Vec<String>, tx: mpsc::Sender<Msg>) {
        let obj = Object::new();
        for (name, bytes) in &files {
            let _ = Reflect::set(&obj, &name.into(), &Uint8Array::from(&bytes[..]));
        }
        let reqs: Array = requirements.iter().map(JsValue::from).collect();
        let progress_tx = tx.clone();
        let on_progress = Closure::<dyn FnMut(String)>::new(move |t: String| {
            let _ = progress_tx.send(Msg::Progress(t));
        });
        wasm_bindgen_futures::spawn_local(async move {
            let outcome = async {
                let promise = js_run(&obj, &reqs, &on_progress)
                    .map_err(|e| format!("Python is not available in this page ({})", text(&e)))?;
                let r = JsFuture::from(promise).await.map_err(|e| text(&e))?;
                let get = |k: &str| Reflect::get(&r, &k.into()).ok().and_then(|v| v.as_string());
                super::parse_outcome(&get("response").unwrap_or_default(), get("network"))
            }
            .await;
            drop(on_progress);
            let _ = tx.send(Msg::Done(outcome));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str =
        include_str!("../../../plugins/EPyT Pressure Check/EPyT Pressure Check.py");

    #[test]
    fn reads_the_header() {
        let p = describe("epyt_pressure.py", EXAMPLE).unwrap();
        assert_eq!(p.name, "EPyT Pressure Check");
        assert!(!p.menu.is_empty());
        assert_eq!(requirements(EXAMPLE), ["epyt"]);
        let src = "NAME = 'A \\'b\\''\nMENU = [\n  \"x\",\n  'y',\n]\ndef run(item):\n    pass\n";
        let p = describe("a.py", src).unwrap();
        assert_eq!(p.name, "A 'b'");
        assert_eq!(p.menu, ["x", "y"]);
        assert!(describe("a.py", "NAME = 'x'\n").is_err());
        assert!(describe("a.py", "def run(item): pass\n").is_err());
    }

    #[test]
    fn answers_are_shown() {
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        app.load_example("Net1", eui_core::examples::NET1);
        let r = r#"{"messages":["hi"],"status":"s","table":{"title":"T","rows":[["a"],["1"]]},
            "select":{"node":"10"},"error":null}"#;
        let mut net = app.net.clone();
        net.nodes.truncate(net.nodes.len() - 1);
        apply(&mut app, "p", parse_outcome(r, Some(net.to_inp())));
        assert_eq!(app.dialog, Dialog::PluginTable);
        assert_eq!(app.status, "hi");
        assert!(app.selection.is_some());
        // the project it left replaced the current one
        assert_eq!(app.net.nodes.len(), 10);
    }

    /// Runs a plugin with the Python of `WEPANET_PYTHON` (with EPyT
    /// installed); skipped when it is not set.
    fn run_local(app: &EpanetApp, p: &UserPlugin, item: &str) -> Option<Outcome> {
        std::env::var("WEPANET_PYTHON").ok()?;
        let (tx, rx) = mpsc::channel();
        backend::run(files(app, p, item), requirements(&p.source), tx);
        loop {
            match rx.recv().ok()? {
                Msg::Progress(_) => {}
                Msg::Done(o) => return Some(o),
            }
        }
    }

    #[test]
    fn plugins_run_in_the_local_python() {
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        app.load_example("Net1", eui_core::examples::NET1);
        let p = describe("epyt_pressure.py", EXAMPLE).unwrap();
        let Some(o) = run_local(&app, &p, "Lowest Pressures") else {
            return;
        };
        let (r, net) = o.unwrap();
        assert!(r.error.is_none(), "{:?}", r.error);
        let t = r.table.expect("a table");
        assert_eq!(t.rows[0], ["Junction", "Lowest pressure", "At hour"]);
        assert!(t.rows.len() > 5, "{:?}", t.rows);
        assert!(r.select.and_then(|s| s.node).is_some());
        assert!(net.is_none());
        // a changed project comes back
        let src = "NAME = 'S'\nimport wepanet\ndef run(item):\n    \
                   wepanet.set_network(open(wepanet.inp_file).read())\n";
        let s = describe("s.py", src).unwrap();
        let (r, net) = run_local(&app, &s, "Run").unwrap().unwrap();
        assert!(r.error.is_none(), "{:?}", r.error);
        let net = eui_core::Network::from_inp(&net.expect("a project")).unwrap();
        assert_eq!(net.nodes.len(), app.net.nodes.len());

        // WEPANET's results, as the runner reads them
        let out = eui_engine::run_blocking(eui_core::examples::NET1).unwrap();
        app.results = Some(eui_core::SimResults::from_out(&out.out).unwrap());
        app.results_generation = app.generation;
        let src = "NAME = 'R'\nimport wepanet\ndef run(item):\n    \
                   print(wepanet.periods(), wepanet.period_hours(1))\n    \
                   print(wepanet.node_series('10', 'Pressure')[3])\n    \
                   print(wepanet.link_result('9', 'flow', 2))\n    \
                   print(wepanet.node_type('2'), wepanet.link_type('9'))\n";
        let q = describe("r.py", src).unwrap();
        let (r, _) = run_local(&app, &q, "Run").unwrap().unwrap();
        assert!(r.error.is_none(), "{:?}", r.error);
        let res = app.results.as_ref().unwrap();
        let row = |ids: &[String], id: &str| ids.iter().position(|x| x == id).unwrap();
        let p10 = res.node_series(row(&res.node_ids, "10"), NodeVar::Pressure)[3];
        let f9 = res.link_slice(2, LinkVar::Flow)[row(&res.link_ids, "9")];
        let said: Vec<String> = r.messages[0].lines().map(str::to_string).collect();
        assert_eq!(
            said[0],
            format!("{} {:?}", res.n_periods, res.period_time(1) as f64 / 3600.0)
        );
        assert!(
            (said[1].parse::<f32>().unwrap() - p10).abs() < 1e-3,
            "{said:?} {p10}"
        );
        assert!(
            (said[2].parse::<f32>().unwrap() - f9).abs() < 1e-2,
            "{said:?} {f9}"
        );
        assert_eq!(said[3], "Tank Pump");

        // with no network the example says so, without an error
        let mut empty = EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        empty.net = eui_core::Network::default();
        for item in &p.menu {
            let (r, net) = run_local(&empty, &p, item).unwrap().unwrap();
            assert!(r.error.is_none(), "{item}: {:?}", r.error);
            assert!(r.messages[0].contains("Open a network"), "{:?}", r.messages);
            assert!(net.is_none());
        }

        // the rest of its folder is beside it, and importable
        app.prefs.plugin_files.insert(
            "H".into(),
            vec![
                ("helpers.py".into(), b"X = 41\n".to_vec()),
                ("data/v.txt".into(), b"1".to_vec()),
            ],
        );
        let src = "NAME = 'H'\nimport os, wepanet, helpers\ndef run(item):\n    \
                   d = os.path.join(wepanet.plugin_dir, 'data', 'v.txt')\n    \
                   print(helpers.X + int(open(d).read()))\n";
        let h = describe("h.py", src).unwrap();
        let (r, _) = run_local(&app, &h, "Run").unwrap().unwrap();
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.messages[0], "42");

        // errors name the plugin's line
        let bad = describe("bad.py", "NAME = 'B'\ndef run(item):\n    1 / 0\n").unwrap();
        let (r, _) = run_local(&app, &bad, "Run").unwrap().unwrap();
        let e = r.error.unwrap();
        assert!(
            e.contains("bad.py\", line 3") && e.contains("ZeroDivisionError"),
            "{e}"
        );
        assert!(!e.contains("python_runner"), "{e}");
    }
}
