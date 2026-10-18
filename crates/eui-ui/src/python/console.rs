//! The Python Console (Plugins > Python Console): a panel under
//! the map where Python lines run one after the other in one session, with
//! `wepanet` imported. Each line sees the project as it is then: when it has
//! changed (or been analysed) since the last line, its files go first. What a
//! line asks WEPANET to show (a table, a selection, a new project) is shown
//! as from a plugin; what it prints, in the console.

use std::sync::mpsc;

use egui::{Color32, Key, Modifiers, RichText, Ui};
use serde::Deserialize;

use super::{Msg, Response, backend, project_files};
use crate::app::EpanetApp;

const BANNER: &str = "Python Console. `wepanet` gives the project and its results \
(help(wepanet)); each line sees the project as it is then. \
`pip install <package>` installs a package.";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Input,
    Output,
    Error,
    Note,
}

#[derive(Default)]
pub struct ConsoleState {
    pub open: bool,
    log: Vec<(Kind, String)>,
    input: String,
    history: Vec<String>,
    /// The history entry shown in the input (Up / Down).
    browse: Option<usize>,
    /// A block goes on (the `...` prompt).
    more: bool,
    busy: bool,
    focus: bool,
    session: Option<backend::Session>,
    /// What the session's project was read from: (document, its edits, its
    /// analysis, the period shown).
    synced: Option<(u64, u64, u64, usize)>,
}

/// What the console answers to a line.
#[derive(Debug, Deserialize)]
struct Reply {
    #[serde(default)]
    output: String,
    #[serde(default)]
    more: bool,
    response: Option<Response>,
    network: Option<String>,
}

/// Opens or closes the console (closing it ends its Python on the desktop).
pub fn toggle(app: &mut EpanetApp) {
    let c = &mut app.python.console;
    c.open = !c.open;
    if c.open {
        if c.log.is_empty() {
            c.log.push((Kind::Note, BANNER.into()));
        }
        c.focus = true;
    } else {
        c.session = None;
        c.synced = None;
        c.busy = false;
        c.more = false;
    }
}

fn submit(app: &mut EpanetApp, line: String) {
    let key = (
        app.document_id,
        app.generation,
        app.results_generation,
        app.period,
    );
    let stale = app.python.console.synced != Some(key);
    let files = stale.then(|| project_files(app, "Python Console", "<console>", None));
    let c = &mut app.python.console;
    let prompt = if c.more { "..." } else { ">>>" };
    c.log.push((Kind::Input, format!("{prompt} {line}")));
    if !line.trim().is_empty() && c.history.last() != Some(&line) {
        c.history.push(line.clone());
    }
    c.browse = None;
    if c.session.is_none() {
        match backend::Session::start() {
            Ok(s) => c.session = Some(s),
            Err(e) => {
                c.log.push((Kind::Error, e));
                return;
            }
        }
    }
    let s = c.session.as_mut().expect("a session");
    match s.push(files, &line) {
        Ok(()) => {
            c.busy = true;
            c.synced = Some(key);
        }
        Err(e) => {
            c.log.push((Kind::Error, e));
            c.session = None;
            c.synced = None;
        }
    }
}

/// Shows the console's answers.
pub(super) fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(s) = &app.python.console.session else {
        return;
    };
    let mut got = Vec::new();
    loop {
        match s.rx.try_recv() {
            Ok(m) => got.push(m),
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                got.push(Msg::Console(Err("Python stopped".into())));
                break;
            }
        }
    }
    for m in got {
        match m {
            Msg::Progress(t) => app.status = t,
            Msg::Console(Ok(json)) => answer(app, &json),
            Msg::Console(Err(e)) => {
                let c = &mut app.python.console;
                c.log.push((Kind::Error, e));
                c.session = None;
                c.synced = None;
                c.busy = false;
                c.more = false;
            }
            Msg::Done(_) => {}
        }
    }
    if app.python.console.busy {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn answer(app: &mut EpanetApp, json: &str) {
    let c = &mut app.python.console;
    c.busy = false;
    c.focus = true;
    let r: Reply = match serde_json::from_str(json) {
        Ok(r) => r,
        Err(e) => {
            c.log
                .push((Kind::Error, format!("a bad answer from Python: {e}")));
            return;
        }
    };
    c.more = r.more;
    let out = r.output.trim_end();
    if !out.is_empty() {
        let error = out.contains("Traceback (most recent call last)")
            || out.lines().last().is_some_and(|l| {
                l.split(':')
                    .next()
                    .is_some_and(|k| k.ends_with("Error") || k.ends_with("Exception"))
            });
        let kind = if error { Kind::Error } else { Kind::Output };
        c.log.push((kind, out.to_string()));
    }
    if let Some(resp) = r.response {
        let problems = super::effects(
            app,
            "The Python Console",
            resp.status,
            resp.select,
            resp.table,
            r.network,
        );
        let c = &mut app.python.console;
        for p in problems {
            c.log.push((Kind::Error, p));
        }
    }
}

/// The console, as a panel under the map.
pub fn show(app: &mut EpanetApp, ui: &mut Ui) {
    if !app.python.console.open {
        return;
    }
    egui::Panel::bottom("python_console")
        .default_size(220.0)
        .size_range(140.0..=700.0)
        .resizable(true)
        .show(ui, |ui| body(app, ui));
}

fn body(app: &mut EpanetApp, ui: &mut Ui) {
    // EPANET-UI's window chrome: the title and the red close button, with
    // Clear (and the spinner while Python runs) left of it
    let (bar, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), crate::dialogs::TITLE_H),
        egui::Sense::hover(),
    );
    let fill = ui.visuals().panel_fill;
    let close = crate::dialogs::title_bar(ui, bar, "Python Console", fill, None, true).close;
    let tools = egui::Rect::from_min_max(
        egui::pos2(bar.center().x, bar.top()),
        egui::pos2(bar.right() - 52.0, bar.bottom()),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(tools)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            if ui.button("Clear").clicked() {
                app.python.console.log.clear();
            }
            if app.python.console.busy {
                ui.spinner();
            }
        },
    );
    if close {
        toggle(app);
        return;
    }
    let mono = egui::FontId::monospace(13.0);
    let dark = ui.visuals().dark_mode;
    let (weak, text) = (ui.visuals().weak_text_color(), ui.visuals().text_color());
    let color = |k: Kind| match k {
        Kind::Input if dark => Color32::from_rgb(0x8a, 0xb4, 0xf8),
        Kind::Input => Color32::from_rgb(0x1a, 0x5f, 0xb4),
        Kind::Error if dark => Color32::from_rgb(0xf2, 0x8b, 0x82),
        Kind::Error => Color32::from_rgb(0xc0, 0x39, 0x2b),
        Kind::Note => weak,
        Kind::Output => text,
    };
    let id = egui::Id::new("python_console_input");
    let c = &mut app.python.console;
    // Up / Down go through the history
    if ui.memory(|m| m.has_focus(id)) && !c.history.is_empty() {
        let up = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp));
        let down = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown));
        if up {
            let k = c
                .browse
                .map_or(c.history.len() - 1, |k| k.saturating_sub(1));
            c.browse = Some(k);
            c.input = c.history[k].clone();
        } else if down {
            match c.browse {
                Some(k) if k + 1 < c.history.len() => {
                    c.browse = Some(k + 1);
                    c.input = c.history[k + 1].clone();
                }
                Some(_) => {
                    c.browse = None;
                    c.input.clear();
                }
                None => {}
            }
        }
    }
    let mut send = None;
    // From the bottom up: the prompt, then the lines filling the rest, so
    // the content is as tall as the panel (a shorter one shrank it).
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        let c = &mut app.python.console;
        // a row as tall as the text field, which is taller than a button
        ui.spacing_mut().interact_size.y = 26.0;
        ui.horizontal(|ui| {
            let prompt = if c.more { "..." } else { ">>>" };
            ui.label(
                RichText::new(prompt)
                    .font(mono.clone())
                    .color(color(Kind::Input)),
            );
            let r = ui.add_enabled(
                !c.busy,
                egui::TextEdit::singleline(&mut c.input)
                    .id(id)
                    .font(mono.clone())
                    .desired_width(f32::INFINITY)
                    .hint_text(if c.busy { "running…" } else { "" }),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                send = Some(std::mem::take(&mut c.input));
            }
            if c.focus && !c.busy {
                r.request_focus();
                c.focus = false;
            }
        });
        // the frame's margins and the spacing above it
        let h = ui.available_height() - 8.0 - ui.spacing().item_spacing.y;
        egui::Frame::NONE
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(4)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("python_console_log")
                    .stick_to_bottom(true)
                    .auto_shrink(false)
                    .max_height(h.max(20.0))
                    // never taller than the room left (it covered the prompt)
                    .min_scrolled_height(h.clamp(0.0, 20.0))
                    .show(ui, |ui| {
                        for (k, text) in &app.python.console.log {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).font(mono.clone()).color(color(*k)),
                                )
                                .wrap()
                                .selectable(true),
                            );
                        }
                    });
            });
    });
    if let Some(line) = send {
        submit(app, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A console with the Python of `WEPANET_PYTHON` answers line by line,
    /// keeps its variables, sees the project, and shows what it asks for.
    #[test]
    fn the_console_runs_lines_in_one_session() {
        if std::env::var("WEPANET_PYTHON").is_err() {
            return;
        }
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.load_example("Net1", eui_core::examples::NET1);
        toggle(&mut app);
        let run = |app: &mut EpanetApp, line: &str| -> Vec<(Kind, String)> {
            let before = app.python.console.log.len();
            submit(app, line.to_string());
            let t = std::time::Instant::now();
            while app.python.console.busy && t.elapsed().as_secs() < 60 {
                std::thread::sleep(std::time::Duration::from_millis(20));
                poll(app, &ctx);
            }
            app.python.console.log[before..].to_vec()
        };
        let out = run(&mut app, "x = len(wepanet.node_ids())");
        assert_eq!(out.len(), 1, "{out:?}");
        let out = run(&mut app, "print(x * 2)");
        assert_eq!(out[1], (Kind::Output, "22".into()), "{out:?}");
        // a block, ended by an empty line
        assert_eq!(run(&mut app, "for i in range(2):").len(), 1);
        assert!(app.python.console.more);
        run(&mut app, "    print('n', i)");
        let out = run(&mut app, "");
        assert_eq!(out[1].1, "n 0\nn 1", "{out:?}");
        assert!(!app.python.console.more);
        // errors
        let out = run(&mut app, "1/0");
        assert_eq!(out[1].0, Kind::Error, "{out:?}");
        // what it asks for is shown
        run(
            &mut app,
            "wepanet.select_node('10'); wepanet.table('T', ['a'], [[1]])",
        );
        assert_eq!(app.dialog, crate::app::Dialog::PluginTable);
        assert!(app.selection.is_some());
        // the project as it is now: an edit is seen by the next line
        let n = app.net.nodes.len();
        app.checkpoint();
        app.net.nodes.truncate(n - 1);
        app.touch();
        let out = run(&mut app, "print(len(wepanet.node_ids()))");
        assert_eq!(out[1].1, (n - 1).to_string(), "{out:?}");
        // and it can replace the project
        let before = app.net.nodes.len();
        run(
            &mut app,
            "wepanet.set_network(open(wepanet.inp_file).read().replace('[JUNCTIONS]', '[JUNCTIONS]\\n J99 1 0', 1))",
        );
        assert_eq!(app.net.nodes.len(), before + 1);
        toggle(&mut app);
        assert!(app.python.console.session.is_none());
    }

    /// The panel shows the banner, the lines and the prompt, under the map.
    #[test]
    fn the_panel_shows_the_session() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        toggle(&mut app);
        let c = &mut app.python.console;
        c.log.push((Kind::Input, ">>> 1/0".into()));
        c.log
            .push((Kind::Error, "ZeroDivisionError: division by zero".into()));
        c.more = true;
        let mut texts = Vec::new();
        let mut panel = egui::Rect::NOTHING;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                show(&mut app, ui);
                panel = ui.available_rect_before_wrap();
            });
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect();
        }
        for want in ["Python Console", "Clear", "...", ">>> 1/0"] {
            assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
        }
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("Python Console. `wepanet`"))
        );
        // it takes the bottom of the window, leaving the rest to the map
        assert!(
            panel.height() > 300.0 && panel.height() < 500.0,
            "{panel:?}"
        );
        toggle(&mut app);
        assert!(!app.python.console.open);
    }

    /// However short the panel, the prompt shows under the lines (a log
    /// that would not get shorter covered it).
    #[test]
    fn the_prompt_shows_in_a_short_panel() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        toggle(&mut app);
        for h in [110.0f32, 140.0, 180.0, 220.0] {
            let mut rects = (
                egui::Rect::NOTHING,
                egui::Rect::NOTHING,
                egui::Rect::NOTHING,
            );
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 800.0),
                    )),
                    ..Default::default()
                };
                let mut panel = egui::Rect::NOTHING;
                let mut out = ctx.run_ui(input, |ui| {
                    egui::Panel::bottom(egui::Id::new("short").with(h as i32))
                        .exact_size(h)
                        .show(ui, |ui| {
                            panel = ui.max_rect();
                            body(&mut app, ui)
                        });
                });
                out.textures_delta.clear();
                let at = |want: &str| {
                    out.shapes
                        .iter()
                        .find_map(|c| match &c.shape {
                            egui::Shape::Text(t) if t.galley.text().starts_with(want) => {
                                Some(t.visual_bounding_rect())
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("no {want:?}"))
                };
                rects = (panel, at(">>>"), at("Python Console. "));
            }
            let (panel, prompt, banner) = rects;
            assert!(
                prompt.bottom() <= panel.bottom(),
                "{h}: {prompt:?} {panel:?}"
            );
            assert!(banner.bottom() < prompt.top(), "{h}: {banner:?} {prompt:?}");
        }
    }

    /// The panel keeps the height it is given, frame after frame (it shrank
    /// when its content came out shorter than the panel).
    #[test]
    fn the_panel_keeps_its_height() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        toggle(&mut app);
        for k in 0..5 {
            app.python
                .console
                .log
                .push((Kind::Output, format!("line {k}")));
        }
        let mut heights = Vec::new();
        for _ in 0..30 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 800.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                show(&mut app, ui);
                heights.push(800.0 - ui.available_rect_before_wrap().height());
            });
            out.textures_delta.clear();
        }
        let first = heights[2];
        assert!(
            heights[2..].iter().all(|h| (h - first).abs() < 0.5),
            "{heights:?}"
        );
        assert!((first - 220.0).abs() < 2.0, "{heights:?}");
    }
}
