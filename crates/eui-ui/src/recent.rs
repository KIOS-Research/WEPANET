//! Recent projects in the browser, which gives the page no file paths. Where
//! the browser opens files by handle (Chrome, Edge), the handle of each
//! project opened or saved is kept (by `engine/recent.js`, in IndexedDB)
//! under its file name — where the file is, not a copy of it — and the
//! Recent Projects list holds those names; a project is read again from its
//! file, the browser asking the user to allow it.

use std::cell::RefCell;

use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::app::EpanetApp;
use crate::io::Inbox;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = euiRecent, js_name = supported)]
    fn js_supported() -> Result<bool, JsValue>;
    #[wasm_bindgen(catch, js_namespace = euiRecent, js_name = pick)]
    fn js_pick() -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = euiRecent, js_name = remember)]
    fn js_remember(handle: &JsValue) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = euiRecent, js_name = open)]
    fn js_open(name: &str) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = euiRecent, js_name = remove)]
    fn js_remove(name: &str) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = euiRecent, js_name = clear)]
    fn js_clear() -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = euiRecent, js_name = list)]
    fn js_list() -> Result<js_sys::Promise, JsValue>;
}

thread_local! {
    /// Projects opened or saved since the last frame, for the top of the list.
    static OPENED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The files the browser keeps, latest first, once read (the list is
    /// theirs: the preferences are saved only now and then).
    static LISTED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    static ASKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Reads the list of the files the browser keeps.
fn ask_list() {
    let Ok(p) = js_list() else {
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(v) = JsFuture::from(p).await {
            let names: Vec<String> = js_sys::Array::from(&v)
                .iter()
                .filter_map(|n| n.as_string())
                .collect();
            LISTED.with(|l| *l.borrow_mut() = Some(names));
        }
    });
}

/// Whether this browser can open a file again (Chrome, Edge).
pub fn supported() -> bool {
    js_supported().unwrap_or(false)
}

/// Lets a promise run to its end, whatever it comes to.
fn detach(p: Result<js_sys::Promise, JsValue>) {
    if let Ok(p) = p {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = JsFuture::from(p).await;
        });
    }
}

fn text(e: &JsValue) -> String {
    e.as_string()
        .or_else(|| Reflect::get(e, &"message".into()).ok()?.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}

/// Reads a `{name, bytes}` the page gave into `inbox`, and the name for the list.
fn deliver(inbox: &Inbox, v: JsValue) -> bool {
    let name = Reflect::get(&v, &"name".into())
        .ok()
        .and_then(|n| n.as_string());
    let bytes = Reflect::get(&v, &"bytes".into()).map(|b| Uint8Array::new(&b).to_vec());
    match (name, bytes) {
        (Some(name), Ok(bytes)) => {
            OPENED.with(|o| o.borrow_mut().push(name.clone()));
            inbox.push(Ok((name, bytes)));
            true
        }
        _ => false,
    }
}

/// The "open file" window, remembering the file chosen.
pub fn pick(inbox: &Inbox, ctx: &egui::Context) {
    let Ok(p) = js_pick() else {
        return;
    };
    let (inbox, ctx) = (inbox.clone(), ctx.clone());
    wasm_bindgen_futures::spawn_local(async move {
        match JsFuture::from(p).await {
            Ok(v) => {
                deliver(&inbox, v);
            }
            // the user closed the window
            Err(e)
                if Reflect::get(&e, &"name".into())
                    .ok()
                    .and_then(|n| n.as_string())
                    == Some("AbortError".into()) => {}
            Err(e) => inbox.push(Err(text(&e))),
        }
        ctx.request_repaint();
    });
}

/// Remembers the file a project was saved to.
pub fn remember(handle: &JsValue) {
    if let Some(name) = Reflect::get(handle, &"name".into())
        .ok()
        .and_then(|n| n.as_string())
    {
        OPENED.with(|o| o.borrow_mut().push(name));
        detach(js_remember(handle));
    }
}

/// Opens the recent project `name` from its file.
pub fn open(app: &mut EpanetApp, name: &str) {
    let Ok(p) = js_open(name) else {
        app.status = "This browser cannot open recent projects".into();
        return;
    };
    let (inbox, ctx) = (app.inbox.clone(), app.ctx.clone());
    let name = name.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        match JsFuture::from(p).await {
            Ok(v) if deliver(&inbox, v.clone()) => {}
            Ok(_) => inbox.push(Err(format!(
                "{name} is no longer among the recent projects"
            ))),
            Err(e) => inbox.push(Err(format!("Could not open {name}: {}", text(&e)))),
        }
        ctx.request_repaint();
    });
}

/// Puts the projects opened or saved since the last frame first in the list;
/// the files of those that fall off it are forgotten.
pub fn update(app: &mut EpanetApp) {
    if !ASKED.with(|a| a.replace(true)) && supported() {
        ask_list();
    }
    if let Some(listed) = LISTED.with(|l| l.borrow_mut().take()) {
        app.prefs.recent = listed;
        app.ctx.request_repaint();
    }
    let opened = OPENED.with(|o| std::mem::take(&mut *o.borrow_mut()));
    if opened.is_empty() {
        app.prefs.recent.truncate(crate::app::MAX_RECENT);
        return;
    }
    let recent = &mut app.prefs.recent;
    for name in opened {
        recent.retain(|r| *r != name);
        recent.insert(0, name);
    }
    for gone in recent.split_off(recent.len().min(crate::app::MAX_RECENT)) {
        detach(js_remove(&gone));
    }
}

/// Empties the list and forgets the files.
pub fn clear(app: &mut EpanetApp) {
    app.prefs.recent.clear();
    detach(js_clear());
}
