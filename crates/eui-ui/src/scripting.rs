//! User plugins: scripts in the Rhai language (<https://rhai.rs>) that anyone
//! can write and load from the Plugins menu, on the desktop and in the
//! browser alike. The API they see is documented in `docs/plugins.md`.
//!
//! A plugin is one `.rhai` file with three functions:
//!
//! ```text
//! fn name() { "Count" }                       // shown on the Plugins toolbar
//! fn menu() { ["Junctions", "Pipes"] }        // its drop-down items
//! fn run(item) { message(`${node_count()} nodes`); }
//! ```
//!
//! A plugin works on a copy of the project; when it has changed the network
//! the copy replaces the project in one step that Undo can take back. Scripts
//! run sandboxed: they cannot touch files or the network, and a step limit
//! stops runaway loops.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use eui_core::props::Element;
use eui_core::{LinkVar, Network, NodeVar, SimResults};
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Scope};

use crate::app::{Dialog, EpanetApp};
use crate::io::Inbox;

/// Steps a plugin may take before it is stopped (a runaway loop guard).
const MAX_OPERATIONS: u64 = 50_000_000;

/// A loaded plugin.
#[derive(Clone, Debug)]
pub struct UserPlugin {
    pub name: String,
    pub description: String,
    pub menu: Vec<String>,
    pub file: String,
    pub source: String,
}

#[derive(Default)]
pub struct ScriptState {
    pub plugins: Vec<UserPlugin>,
    pub inbox: Inbox,
    loaded: bool,
    /// The table a plugin showed: title and rows (header first).
    pub table: Option<(String, crate::reports::Rows)>,
}

/// What a running plugin can see and what it asks for.
struct Ctx {
    net: Network,
    results: Option<SimResults>,
    period: usize,
    node_rows: HashMap<String, usize>,
    link_rows: HashMap<String, usize>,
    changed: bool,
    messages: Vec<String>,
    table: Option<(String, crate::reports::Rows)>,
    select: Option<Element>,
    status: Option<String>,
}

type Res<T> = Result<T, Box<EvalAltResult>>;

fn err<T>(msg: String) -> Res<T> {
    Err(msg.into())
}

fn node_index(c: &Ctx, id: &str) -> Res<usize> {
    c.net
        .node_index(id)
        .map_or_else(|| err(format!("there is no node \"{id}\"")), Ok)
}

fn link_index(c: &Ctx, id: &str) -> Res<usize> {
    c.net
        .link_index(id)
        .map_or_else(|| err(format!("there is no link \"{id}\"")), Ok)
}

/// A property value: a number when it reads as one, else text.
fn value(s: String) -> Dynamic {
    match s.trim().parse::<f64>() {
        Ok(x) => Dynamic::from_float(x),
        Err(_) => Dynamic::from(s),
    }
}

fn get_prop(c: &Ctx, e: Element, prop: &str) -> Res<Dynamic> {
    let props = c.net.element_props(e);
    match props
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(prop.trim()))
    {
        Some(p) => Ok(value(p.value.clone())),
        None => err(format!(
            "unknown property \"{prop}\"; this object has: {}",
            props.iter().map(|p| p.name).collect::<Vec<_>>().join(", ")
        )),
    }
}

fn set_prop(c: &mut Ctx, e: Element, prop: &str, v: Dynamic) -> Res<()> {
    let key = c
        .net
        .element_props(e)
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(prop.trim()))
        .map(|p| p.key);
    let Some(key) = key else {
        return err(format!("unknown property \"{prop}\""));
    };
    let text = if v.is_float() {
        let x = v.as_float().unwrap_or(0.0);
        format!("{}", (x * 1e6).round() / 1e6)
    } else {
        v.to_string()
    };
    c.net
        .set_prop(e, key, &text)
        .map_err(|m| -> Box<EvalAltResult> { m.into() })?;
    c.changed = true;
    Ok(())
}

fn node_var(name: &str) -> Res<NodeVar> {
    match name.trim().to_ascii_lowercase().as_str() {
        "demand" => Ok(NodeVar::Demand),
        "head" => Ok(NodeVar::Head),
        "pressure" => Ok(NodeVar::Pressure),
        "quality" => Ok(NodeVar::Quality),
        _ => err(format!(
            "unknown node result \"{name}\" (Demand, Head, Pressure, Quality)"
        )),
    }
}

fn link_var(name: &str) -> Res<LinkVar> {
    match name.trim().to_ascii_lowercase().as_str() {
        "flow" => Ok(LinkVar::Flow),
        "velocity" => Ok(LinkVar::Velocity),
        "headloss" | "head loss" => Ok(LinkVar::Headloss),
        "quality" => Ok(LinkVar::Quality),
        "status" => Ok(LinkVar::Status),
        "setting" => Ok(LinkVar::Setting),
        "reaction" => Ok(LinkVar::Reaction),
        "friction" => Ok(LinkVar::Friction),
        _ => err(format!(
            "unknown link result \"{name}\" (Flow, Velocity, Headloss, Quality, Status, Setting, Reaction, Friction)"
        )),
    }
}

fn results(c: &Ctx) -> Res<&SimResults> {
    c.results.as_ref().map_or_else(
        || err("there are no results; run an analysis first".into()),
        Ok,
    )
}

fn period_of(c: &Ctx, p: i64) -> Res<usize> {
    let n = results(c)?.n_periods as i64;
    if (0..n).contains(&p) {
        Ok(p as usize)
    } else {
        err(format!("period {p} is out of range (0 to {})", n - 1))
    }
}

fn array_of_strings(a: Array) -> Vec<String> {
    a.into_iter().map(|d| d.to_string()).collect()
}

/// The script engine with WEPANET's API bound to `ctx`.
fn engine(ctx: Rc<RefCell<Ctx>>) -> Engine {
    let mut e = Engine::new();
    e.set_max_operations(MAX_OPERATIONS);
    e.set_max_call_levels(64);
    e.set_max_expr_depths(256, 128);
    let c = ctx.clone();
    e.on_print(move |s| c.borrow_mut().messages.push(s.to_string()));

    macro_rules! with {
        ($name:literal, |$c:ident $(, $a:ident : $t:ty)*| -> $r:ty $body:block) => {{
            let ctx = ctx.clone();
            e.register_fn($name, move |$($a: $t),*| -> $r {
                #[allow(unused_mut)]
                let mut $c = ctx.borrow_mut();
                $body
            });
        }};
        ($name:literal, |$c:ident $(, $a:ident : $t:ty)*| $body:expr) => {{
            let ctx = ctx.clone();
            e.register_fn($name, move |$($a: $t),*| {
                #[allow(unused_mut)]
                let mut $c = ctx.borrow_mut();
                $body
            });
        }};
    }

    // ---- project ----
    with!("node_count", |c| c.net.nodes.len() as i64);
    with!("link_count", |c| c.net.links.len() as i64);
    with!("node_ids", |c| c
        .net
        .nodes
        .iter()
        .map(|n| Dynamic::from(n.id.clone()))
        .collect::<Array>());
    with!("link_ids", |c| c
        .net
        .links
        .iter()
        .map(|l| Dynamic::from(l.id.clone()))
        .collect::<Array>());
    with!("node_type", |c, id: &str| -> Res<ImmutableString> {
        let i = node_index(&c, id)?;
        Ok(c.net.nodes[i].kind.label().into())
    });
    with!("link_type", |c, id: &str| -> Res<ImmutableString> {
        let i = link_index(&c, id)?;
        Ok(c.net.links[i].kind.label().into())
    });
    with!("link_nodes", |c, id: &str| -> Res<Array> {
        let i = link_index(&c, id)?;
        let l = &c.net.links[i];
        Ok(vec![
            Dynamic::from(l.from.clone()),
            Dynamic::from(l.to.clone()),
        ])
    });
    with!("node_xy", |c, id: &str| -> Res<Dynamic> {
        let i = node_index(&c, id)?;
        Ok(match c.net.nodes[i].pos {
            Some([x, y]) => {
                Dynamic::from_array(vec![Dynamic::from_float(x), Dynamic::from_float(y)])
            }
            None => Dynamic::UNIT,
        })
    });
    with!("node_value", |c, id: &str, prop: &str| -> Res<Dynamic> {
        let i = node_index(&c, id)?;
        get_prop(&c, Element::Node(i), prop)
    });
    with!("link_value", |c, id: &str, prop: &str| -> Res<Dynamic> {
        let i = link_index(&c, id)?;
        get_prop(&c, Element::Link(i), prop)
    });
    with!("set_node_value", |c,
                             id: &str,
                             prop: &str,
                             v: Dynamic|
     -> Res<()> {
        let i = node_index(&c, id)?;
        set_prop(&mut c, Element::Node(i), prop, v)
    });
    with!("set_link_value", |c,
                             id: &str,
                             prop: &str,
                             v: Dynamic|
     -> Res<()> {
        let i = link_index(&c, id)?;
        set_prop(&mut c, Element::Link(i), prop, v)
    });
    with!("option", |c, section: &str, key: &str| -> Dynamic {
        c.net
            .option(&section.to_ascii_uppercase(), &key.to_ascii_uppercase())
            .map_or(Dynamic::UNIT, value)
    });
    with!("set_option", |c, section: &str, key: &str, v: Dynamic| {
        c.net.set_option(
            &section.to_ascii_uppercase(),
            &key.to_ascii_uppercase(),
            &v.to_string(),
        );
        c.changed = true;
    });
    with!("flow_units", |c| -> ImmutableString {
        c.net.flow_units().into()
    });
    with!("is_si", |c| c.net.is_si());
    with!("title", |c| -> ImmutableString {
        c.net.title.join("\n").into()
    });

    // ---- results ----
    with!("has_results", |c| c.results.is_some());
    with!("periods", |c| c
        .results
        .as_ref()
        .map_or(0, |r| r.n_periods as i64));
    with!("current_period", |c| c.period as i64);
    with!("period_hours", |c, p: i64| -> Res<f64> {
        let p = period_of(&c, p)?;
        Ok(results(&c)?.period_time(p) as f64 / 3600.0)
    });
    with!("node_result", |c,
                          id: &str,
                          var: &str,
                          p: i64|
     -> Res<f64> {
        let p = period_of(&c, p)?;
        let v = node_var(var)?;
        let r = *c
            .node_rows
            .get(id)
            .ok_or_else(|| format!("no results for node \"{id}\""))?;
        Ok(results(&c)?.node_slice(p, v)[r] as f64)
    });
    with!("link_result", |c,
                          id: &str,
                          var: &str,
                          p: i64|
     -> Res<f64> {
        let p = period_of(&c, p)?;
        let v = link_var(var)?;
        let r = *c
            .link_rows
            .get(id)
            .ok_or_else(|| format!("no results for link \"{id}\""))?;
        Ok(results(&c)?.link_slice(p, v)[r] as f64)
    });
    with!("node_series", |c, id: &str, var: &str| -> Res<Array> {
        let v = node_var(var)?;
        let r = *c
            .node_rows
            .get(id)
            .ok_or_else(|| format!("no results for node \"{id}\""))?;
        Ok(results(&c)?
            .node_series(r, v)
            .into_iter()
            .map(|x| Dynamic::from_float(x as f64))
            .collect())
    });
    with!("link_series", |c, id: &str, var: &str| -> Res<Array> {
        let v = link_var(var)?;
        let r = *c
            .link_rows
            .get(id)
            .ok_or_else(|| format!("no results for link \"{id}\""))?;
        Ok(results(&c)?
            .link_series(r, v)
            .into_iter()
            .map(|x| Dynamic::from_float(x as f64))
            .collect())
    });

    // ---- showing things ----
    with!("message", |c, text: Dynamic| c
        .messages
        .push(text.to_string()));
    with!("status", |c, text: Dynamic| c.status =
        Some(text.to_string()));
    with!("table", |c, title: &str, header: Array, rows: Array| {
        let mut t = vec![array_of_strings(header)];
        for r in rows {
            t.push(array_of_strings(r.into_array().unwrap_or_default()));
        }
        c.table = Some((title.to_string(), t));
    });
    with!("select_node", |c, id: &str| -> Res<()> {
        c.select = Some(Element::Node(node_index(&c, id)?));
        Ok(())
    });
    with!("select_link", |c, id: &str| -> Res<()> {
        c.select = Some(Element::Link(link_index(&c, id)?));
        Ok(())
    });
    e
}

/// Reads a plugin's name, description and menu.
fn describe(file: &str, source: &str) -> Result<UserPlugin, String> {
    let mut e = Engine::new();
    e.set_max_expr_depths(256, 128);
    let ast = e.compile(source).map_err(|x| x.to_string())?;
    let call = |f: &str| e.call_fn::<Dynamic>(&mut Scope::new(), &ast, f, ());
    let name = call("name")
        .map_err(|_| "the plugin has no fn name()".to_string())?
        .to_string();
    let menu = match call("menu") {
        Ok(m) => array_of_strings(
            m.into_array()
                .map_err(|_| "fn menu() must return an array".to_string())?,
        ),
        Err(_) => vec!["Run".into()],
    };
    if ast.iter_functions().all(|f| f.name != "run") {
        return Err("the plugin has no fn run(item)".into());
    }
    let description = call("description")
        .map(|d| d.to_string())
        .unwrap_or_default();
    Ok(UserPlugin {
        name,
        description,
        menu,
        file: file.to_string(),
        source: source.to_string(),
    })
}

/// Loads the plugins kept in the preferences (once) and any file just opened.
pub fn poll(app: &mut EpanetApp) {
    if !app.scripts.loaded {
        app.scripts.loaded = true;
        for (file, source) in app.prefs.user_plugins.clone() {
            if let Ok(p) = describe(&file, &source) {
                app.scripts.plugins.push(p);
            }
        }
    }
    for item in app.scripts.inbox.drain() {
        match item.and_then(|(file, bytes)| install(app, &file, &crate::io::decode_text(&bytes))) {
            Ok(name) => {
                app.status = format!("Plugin \"{name}\" loaded");
                app.prefs.menu_tab = crate::app::MenuTab::Plugins;
            }
            Err(e) => {
                app.plugins.message = format!("Could not load the plugin:\n{e}");
                app.dialog = Dialog::PluginMessage;
            }
        }
    }
}

/// Adds a plugin (replacing one of the same name) and keeps it for the next
/// session. Returns its name.
pub fn install(app: &mut EpanetApp, file: &str, source: &str) -> Result<String, String> {
    let p = describe(file, source)?;
    let name = p.name.clone();
    app.prefs
        .user_plugins
        .retain(|(f, s)| describe(f, s).map_or(true, |q| q.name != name));
    app.prefs
        .user_plugins
        .push((p.file.clone(), p.source.clone()));
    app.scripts.plugins.retain(|q| q.name != name);
    app.scripts.plugins.push(p);
    Ok(name)
}

/// Names of the loaded plugins.
pub fn installed(app: &EpanetApp) -> Vec<String> {
    app.scripts.plugins.iter().map(|p| p.name.clone()).collect()
}

/// Asks for a plugin file to load.
pub fn load(app: &mut EpanetApp, ctx: &egui::Context) {
    crate::io::open_file(&app.scripts.inbox, ctx, "WEPANET plugin (Rhai)", &["rhai"]);
}

/// Removes a loaded plugin.
pub fn remove(app: &mut EpanetApp, name: &str) {
    app.scripts.plugins.retain(|p| p.name != name);
    app.prefs
        .user_plugins
        .retain(|(f, s)| describe(f, s).map_or(true, |p| p.name != name));
    app.status = format!("Plugin \"{name}\" removed");
}

/// Runs a plugin's `run(item)`.
pub fn run(app: &mut EpanetApp, plugin: &str, item: &str) {
    let Some(p) = app
        .scripts
        .plugins
        .iter()
        .find(|p| p.name == plugin)
        .cloned()
    else {
        return;
    };
    let _ = run_source(app, &p.name, &p.source, Some(item));
}

/// Runs a script, as a plugin runs: its `run(item)` when `item` is given,
/// else its top-level code (then `run("Run")`, if it defines one). Returns a
/// one-line account of what it did, or the error it stopped with.
pub fn run_source(
    app: &mut EpanetApp,
    name: &str,
    source: &str,
    item: Option<&str>,
) -> Result<String, String> {
    let rows = |ids: &[String]| {
        ids.iter()
            .enumerate()
            .map(|(k, id)| (id.clone(), k))
            .collect()
    };
    let ctx = Rc::new(RefCell::new(Ctx {
        net: app.net.clone(),
        node_rows: app
            .results
            .as_ref()
            .map(|r| rows(&r.node_ids))
            .unwrap_or_default(),
        link_rows: app
            .results
            .as_ref()
            .map(|r| rows(&r.link_ids))
            .unwrap_or_default(),
        results: if app.results_stale() {
            None
        } else {
            app.results.clone()
        },
        period: app.period,
        changed: false,
        messages: Vec::new(),
        table: None,
        select: None,
        status: None,
    }));
    let e = engine(ctx.clone());
    let call_run = |ast: &rhai::AST, item: &str| {
        e.call_fn::<Dynamic>(
            &mut Scope::new(),
            ast,
            "run",
            (ImmutableString::from(item),),
        )
    };
    let outcome = e
        .compile(source)
        .map_err(|x| x.to_string())
        .and_then(|ast| {
            match item {
                Some(item) => call_run(&ast, item),
                None => e
                    .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast)
                    .and_then(|v| {
                        if ast
                            .iter_functions()
                            .any(|f| f.name == "run" && f.params.len() == 1)
                        {
                            call_run(&ast, "Run")
                        } else {
                            Ok(v)
                        }
                    }),
            }
            .map_err(|x| x.to_string())
        });
    drop(e);
    let c = Rc::try_unwrap(ctx)
        .ok()
        .map(RefCell::into_inner)
        .ok_or_else(|| "the script is still in use".to_string())?;
    let mut messages = c.messages;
    if let Err(x) = &outcome {
        messages.push(format!("The plugin stopped with an error:\n{x}"));
    }
    let mut done = Vec::new();
    if outcome.is_ok() && c.changed {
        app.checkpoint();
        app.net = c.net;
        app.status = format!("{name} changed the project (Undo reverts it)");
        done.push("changed the project (Undo reverts it)".to_string());
    }
    if let Some(s) = c.status {
        app.status = s;
    }
    if let Some(e) = c.select {
        app.select(Some(e));
        done.push("selected an object".into());
    }
    if let Some(t) = c.table {
        done.push(format!(
            "showed the table \"{}\" ({} rows)",
            t.0,
            t.1.len().saturating_sub(1)
        ));
        app.scripts.table = Some(t);
        app.dialog = Dialog::PluginTable;
    } else if !messages.is_empty() {
        app.plugins.message = messages.join("\n");
        app.dialog = Dialog::PluginMessage;
    }
    let _ = outcome?;
    if !messages.is_empty() {
        done.push(format!("said: {}", messages.join(" / ")));
    }
    Ok(if done.is_empty() {
        "ran without changing or showing anything".into()
    } else {
        done.join("; ")
    })
}

/// The table a plugin showed.
pub fn table_dialog(app: &mut EpanetApp, ui: &mut egui::Ui) -> bool {
    let Some((title, rows)) = &app.scripts.table else {
        return true;
    };
    ui.set_min_size(egui::vec2(520.0, 360.0));
    ui.label(egui::RichText::new(title).strong().size(14.0));
    ui.add_space(6.0);
    let mut sort = None;
    let footer = format!("{} rows", rows.len().saturating_sub(1));
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .show(ui, |ui| {
            crate::reports::table(ui, "plugin_table", rows, &mut sort, &footer);
        });
    ui.add_space(6.0);
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button("Copy").clicked() {
            let text = rows
                .iter()
                .map(|r| r.join("\t"))
                .collect::<Vec<_>>()
                .join("\n");
            ui.ctx().copy_text(text);
        }
        close = ui.button("Close").clicked();
    });
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    const COUNT: &str = include_str!("../../../plugins/examples/count.rhai");

    fn ctx() -> Rc<RefCell<Ctx>> {
        Rc::new(RefCell::new(Ctx {
            net: Network::from_inp(eui_core::examples::NET1).unwrap(),
            results: None,
            period: 0,
            node_rows: HashMap::new(),
            link_rows: HashMap::new(),
            changed: false,
            messages: Vec::new(),
            table: None,
            select: None,
            status: None,
        }))
    }

    fn eval(c: &Rc<RefCell<Ctx>>, script: &str) -> Result<Dynamic, String> {
        engine(c.clone())
            .eval::<Dynamic>(script)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn example_plugin_describes_itself() {
        let p = describe("count.rhai", COUNT).unwrap();
        assert_eq!(p.name, "Count");
        assert_eq!(p.menu, ["Junctions", "Pipes"]);
    }

    #[test]
    fn example_plugins_run() {
        let run = |c: &Rc<RefCell<Ctx>>, src: &str, item: &str| {
            let e = engine(c.clone());
            let ast = e.compile(src).unwrap();
            e.call_fn::<Dynamic>(
                &mut Scope::new(),
                &ast,
                "run",
                (ImmutableString::from(item),),
            )
            .map_err(|x| x.to_string())
        };
        let c = ctx();
        let _ = run(
            &c,
            include_str!("../../../plugins/examples/pipe_summary.rhai"),
            "Length by Diameter",
        )
        .unwrap();
        let (_, rows) = c.borrow().table.clone().unwrap();
        assert!(rows.len() > 3, "{rows:?}");
        // with results
        let out = eui_engine::run_blocking(eui_core::examples::NET1).unwrap();
        let res = SimResults::from_out(&out.out).unwrap();
        let c = ctx();
        c.borrow_mut().node_rows = res
            .node_ids
            .iter()
            .enumerate()
            .map(|(k, id)| (id.clone(), k))
            .collect();
        c.borrow_mut().results = Some(res);
        let _ = run(
            &c,
            include_str!("../../../plugins/examples/low_pressure.rhai"),
            "Below 120",
        )
        .unwrap();
        let b = c.borrow();
        let (_, rows) = b.table.clone().expect("a table");
        assert!(rows.len() > 1, "{rows:?}");
        assert!(b.select.is_some());
    }

    #[test]
    fn every_repository_plugin_runs_every_command() {
        let out = eui_engine::run_blocking(eui_core::examples::NET1).unwrap();
        let res = SimResults::from_out(&out.out).unwrap();
        let rows = |ids: &[String]| {
            ids.iter()
                .enumerate()
                .map(|(k, id)| (id.clone(), k))
                .collect()
        };
        for (file, src) in crate::plugin_manager::BUILTIN {
            let p = describe(file, src).unwrap_or_else(|e| panic!("{file}: {e}"));
            for item in &p.menu {
                for with_results in [false, true] {
                    let c = ctx();
                    if with_results {
                        let mut b = c.borrow_mut();
                        b.node_rows = rows(&res.node_ids);
                        b.link_rows = rows(&res.link_ids);
                        b.results = Some(res.clone());
                    }
                    let e = engine(c.clone());
                    let ast = e.compile(*src).unwrap();
                    let _ = e
                        .call_fn::<Dynamic>(
                            &mut Scope::new(),
                            &ast,
                            "run",
                            (ImmutableString::from(item.as_str()),),
                        )
                        .unwrap_or_else(|x| panic!("{file} / {item}: {x}"));
                    let b = c.borrow();
                    assert!(
                        b.table.is_some() || !b.messages.is_empty(),
                        "{file} / {item} showed nothing"
                    );
                }
            }
        }
    }

    #[test]
    fn reads_and_changes_the_network() {
        let c = ctx();
        assert_eq!(eval(&c, "node_count()").unwrap().as_int().unwrap(), 11);
        assert_eq!(
            eval(&c, r#"node_value("10", "Elevation")"#)
                .unwrap()
                .as_float()
                .unwrap(),
            710.0
        );
        let _ = eval(&c, r#"set_node_value("10", "Elevation", 715.5)"#).unwrap();
        assert!(c.borrow().changed);
        assert_eq!(c.borrow().net.nodes[0].field(0), "715.5");
        assert!(
            eval(&c, r#"node_value("nope", "Elevation")"#)
                .unwrap_err()
                .contains("no node")
        );
        assert!(eval(&c, "loop {}").unwrap_err().contains("operations"));
    }
}
