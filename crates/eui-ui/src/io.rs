//! Opening and saving files on desktop and in the browser.
//!
//! Browsers only allow asynchronous file access, so everything that reads a
//! file delivers into an [`Inbox`] that the app drains once per frame.

use std::cell::RefCell;
use std::rc::Rc;

/// A file that has been read: (file name, contents).
pub type Loaded = (String, Vec<u8>);

#[derive(Clone, Default)]
pub struct Inbox(Rc<RefCell<Vec<Result<Loaded, String>>>>);

impl Inbox {
    pub fn push(&self, item: Result<Loaded, String>) {
        self.0.borrow_mut().push(item);
    }
    pub fn drain(&self) -> Vec<Result<Loaded, String>> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

/// Decodes INP bytes; old files are often Latin-1 rather than UTF-8.
pub fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Shows an "open file" dialog. Returns the chosen path on desktop.
pub fn open_dialog(inbox: &Inbox, ctx: &egui::Context) -> Option<std::path::PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        let path = rfd::FileDialog::new()
            .add_filter("EPANET input file", &["inp", "INP"])
            .add_filter("All files", &["*"])
            .pick_file()?;
        inbox.push(read_path(&path));
        Some(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let inbox = inbox.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("EPANET input file", &["inp", "INP"])
                .pick_file()
                .await
            {
                inbox.push(Ok((file.file_name(), file.read().await)));
                ctx.request_repaint();
            }
        });
        None
    }
}

/// Shows an "open file" dialog for any kind of text file; the file is
/// delivered into `inbox`.
pub fn open_file(inbox: &Inbox, ctx: &egui::Context, kind: &str, exts: &[&str]) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(kind, exts)
            .add_filter("All files", &["*"])
            .pick_file()
        {
            inbox.push(read_path(&path));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let inbox = inbox.clone();
        let ctx = ctx.clone();
        let kind = kind.to_string();
        let exts: Vec<String> = exts.iter().map(|e| e.to_string()).collect();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter(&kind, &exts)
                .pick_file()
                .await
            {
                inbox.push(Ok((file.file_name(), file.read().await)));
                ctx.request_repaint();
            }
        });
    }
}

/// Saves a report or data as a text file (asks where on desktop, downloads
/// in the browser).
pub fn save_report(text: &str, suggested_name: &str, ctx: &egui::Context) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        let ext = std::path::Path::new(suggested_name)
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "txt".into());
        let Some(target) = rfd::FileDialog::new()
            .add_filter("Text file", &[ext.as_str()])
            .add_filter("All files", &["*"])
            .set_file_name(suggested_name)
            .save_file()
        else {
            return Ok(());
        };
        std::fs::write(&target, text).map_err(|e| format!("{}: {e}", target.display()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let ext = suggested_name
            .rsplit('.')
            .next()
            .unwrap_or("txt")
            .to_string();
        save_in_browser(text, suggested_name, "Text file", &ext, true, None, ctx);
        Ok(())
    }
}

/// Saves binary data such as an image: asks where on desktop, downloads in
/// the browser.
pub fn save_bytes(
    bytes: &[u8],
    name: &str,
    kind: &str,
    ext: &str,
    mime: &str,
) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = mime;
        let Some(target) = rfd::FileDialog::new()
            .add_filter(kind, &[ext])
            .set_file_name(name)
            .save_file()
        else {
            return Ok(());
        };
        std::fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (kind, ext);
        download_bytes(name, bytes, mime).map_err(|e| format!("{e:?}"))
    }
}

#[cfg(target_arch = "wasm32")]
fn download_bytes(name: &str, bytes: &[u8], mime: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    web_sys::Url::revoke_object_url(&url)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_path(path: &std::path::Path) -> Result<Loaded, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::fs::read(path)
        .map(|b| (name, b))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads files dropped onto the window.
pub fn take_dropped(inbox: &Inbox, ctx: &egui::Context) -> Option<std::path::PathBuf> {
    let files = ctx.input(|i| i.raw.dropped_files.clone());
    let file = files.into_iter().next()?;
    let name = file
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    #[cfg(not(target_arch = "wasm32"))]
    {
        inbox.push(file.bytes().map(|b| (name, b)));
        Some(file.path().to_path_buf())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let inbox = inbox.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            inbox.push(file.bytes_async().await.map(|b| (name, b)));
            ctx.request_repaint();
        });
        None
    }
}

/// Shows an "open files" dialog that allows several files; they arrive in
/// `inbox`. On the desktop, choosing a .shp also brings the .dbf, .prj (and
/// other files) of the same name beside it.
pub fn open_files(inbox: &Inbox, ctx: &egui::Context, kind: &str, exts: &[&str]) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        let Some(paths) = rfd::FileDialog::new()
            .add_filter(kind, exts)
            .add_filter("All files", &["*"])
            .pick_files()
        else {
            return;
        };
        let mut all = paths.clone();
        for p in &paths {
            for e in exts {
                let s = p.with_extension(e);
                if s.exists() && !all.contains(&s) {
                    all.push(s);
                }
            }
        }
        for p in all {
            inbox.push(read_path(&p));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let inbox = inbox.clone();
        let ctx = ctx.clone();
        let kind = kind.to_string();
        let exts: Vec<String> = exts.iter().map(|e| e.to_string()).collect();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(files) = rfd::AsyncFileDialog::new()
                .add_filter(&kind, &exts)
                .pick_files()
                .await
            {
                for f in files {
                    inbox.push(Ok((f.file_name(), f.read().await)));
                }
                ctx.request_repaint();
            }
        });
    }
}

/// Saves text. On desktop, writes to `path` (or asks for one); in the browser,
/// downloads the file. Returns the path written on desktop.
pub fn save_text(
    text: &str,
    suggested_name: &str,
    path: Option<&std::path::Path>,
    ask: bool,
    token: (u64, u64),
    ctx: &egui::Context,
) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (token, ctx);
        let target = match path {
            Some(p) if !ask => p.to_path_buf(),
            _ => match rfd::FileDialog::new()
                .add_filter("EPANET input file", &["inp"])
                .set_file_name(suggested_name)
                .save_file()
            {
                Some(p) => p,
                None => return Ok(None),
            },
        };
        std::fs::write(&target, text).map_err(|e| format!("{}: {e}", target.display()))?;
        Ok(Some(target))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        save_in_browser(
            text,
            suggested_name,
            "EPANET input file",
            "inp",
            ask,
            Some(token),
            ctx,
        );
        Ok(None)
    }
}

#[cfg(target_arch = "wasm32")]
fn download(name: &str, text: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/plain");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    web_sys::Url::revoke_object_url(&url)?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// File handle of the project, so that Save writes back to the same file.
    static PROJECT_HANDLE: std::cell::RefCell<Option<wasm_bindgen::JsValue>> =
        const { std::cell::RefCell::new(None) };
    static PROJECT_EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static SAVE_EVENTS: RefCell<Vec<SaveEvent>> = const { RefCell::new(Vec::new()) };
}

pub struct SaveEvent {
    pub token: Option<(u64, u64)>,
    /// None means the user cancelled; errors never mark the document saved.
    pub result: Result<Option<String>, String>,
}

/// The outcome of a save that finished in the background (browser only).
pub fn take_save_events() -> Vec<SaveEvent> {
    #[cfg(target_arch = "wasm32")]
    {
        SAVE_EVENTS.with(|m| std::mem::take(&mut *m.borrow_mut()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

/// Forgets the project's file (a new or newly opened project asks again).
pub fn forget_project_file() {
    #[cfg(target_arch = "wasm32")]
    {
        PROJECT_HANDLE.with(|h| *h.borrow_mut() = None);
        PROJECT_EPOCH.with(|e| e.set(e.get() + 1));
    }
}

/// Saves text in the browser with a "Save as" window where the browser has
/// one (Chrome, Edge: File System Access API); elsewhere it downloads the
/// file. With `project`, the chosen file is remembered so that a later Save
/// (with `ask` false) writes to it again without asking.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
fn save_in_browser(
    text: &str,
    name: &str,
    kind: &str,
    ext: &str,
    ask: bool,
    token: Option<(u64, u64)>,
    ctx: &egui::Context,
) {
    use js_sys::{Array, Function, Object, Promise, Reflect};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    let text = text.to_string();
    let name = name.to_string();
    let kind = kind.to_string();
    let ext = ext.to_string();
    let ctx = ctx.clone();
    let project = token.is_some();
    let epoch = PROJECT_EPOCH.with(|e| e.get());
    wasm_bindgen_futures::spawn_local(async move {
        let window: JsValue = js_sys::global().into();
        let picker = Reflect::get(&window, &"showSaveFilePicker".into()).ok();
        let Some(picker) = picker.and_then(|p| p.dyn_into::<Function>().ok()) else {
            let result = download(&name, &text)
                .map(|()| Some(name.clone()))
                .map_err(|e| format!("{e:?}"));
            SAVE_EVENTS.with(|m| m.borrow_mut().push(SaveEvent { token, result }));
            ctx.request_repaint();
            return;
        };
        let call = |f: &JsValue, method: &str, args: &[JsValue]| -> Result<Promise, JsValue> {
            let m: Function = Reflect::get(f, &method.into())?.dyn_into()?;
            let r = m.apply(f, &args.iter().cloned().collect::<Array>())?;
            r.dyn_into::<Promise>()
        };
        let result: Result<String, JsValue> = async {
            let known = if project && !ask {
                PROJECT_HANDLE.with(|h| h.borrow().clone())
            } else {
                None
            };
            let handle = match known {
                Some(h) => h,
                None => {
                    let accept = Object::new();
                    Reflect::set(
                        &accept,
                        &"text/plain".into(),
                        &Array::of1(&format!(".{ext}").into()),
                    )?;
                    let ty = Object::new();
                    Reflect::set(&ty, &"description".into(), &kind.clone().into())?;
                    Reflect::set(&ty, &"accept".into(), &accept)?;
                    let opts = Object::new();
                    Reflect::set(&opts, &"suggestedName".into(), &name.clone().into())?;
                    Reflect::set(&opts, &"types".into(), &Array::of1(&ty))?;
                    let p: Promise = picker.call1(&window, &opts)?.dyn_into()?;
                    JsFuture::from(p).await?
                }
            };
            if project && PROJECT_EPOCH.with(|e| e.get()) != epoch {
                return Err(JsValue::from_str(
                    "The project changed before the save completed",
                ));
            }
            let writable = JsFuture::from(call(&handle, "createWritable", &[])?).await?;
            JsFuture::from(call(&writable, "write", &[text.clone().into()])?).await?;
            JsFuture::from(call(&writable, "close", &[])?).await?;
            if project && PROJECT_EPOCH.with(|e| e.get()) == epoch {
                PROJECT_HANDLE.with(|h| *h.borrow_mut() = Some(handle.clone()));
            }
            let saved = Reflect::get(&handle, &"name".into())?
                .as_string()
                .unwrap_or(name.clone());
            Ok(saved)
        }
        .await;
        let result = match result {
            Ok(saved) => Ok(Some(saved)),
            // The user closed the Save window: nothing to report.
            Err(e)
                if Reflect::get(&e, &"name".into())
                    .ok()
                    .and_then(|n| n.as_string())
                    .as_deref()
                    == Some("AbortError") =>
            {
                Ok(None)
            }
            Err(e) => Err(format!("{e:?}")),
        };
        SAVE_EVENTS.with(|m| m.borrow_mut().push(SaveEvent { token, result }));
        ctx.request_repaint();
    });
}
