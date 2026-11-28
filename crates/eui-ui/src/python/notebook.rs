//! The Python Notebook (Plugins > Python Notebook): a panel of the dock
//! whose cells of Python run, one after the other, in one session with
//! `wepanet` imported; Shift+Enter runs a cell and goes to the next. Opening
//! it runs its first cell, which opens the project with EPyT. Each cell sees
//! the project as it is then: when it has changed (or been analysed) since
//! the last cell, its files go first. What a cell asks WEPANET to show (a
//! table, a selection, a new project) is shown as from a plugin, and so is
//! a change a cell makes to an EPyT project: the map shows it at once (Undo
//! takes it back). What a cell prints, the value of its last line and the
//! figures it draws are shown under it. Notebooks open from and save to
//! `.ipynb` files, and EPyT's example notebooks open from its repository.

mod files;

use std::collections::VecDeque;
use std::sync::{Arc, mpsc};

use egui::{Color32, Key, Modifiers, RichText, Ui, vec2};
use serde::Deserialize;

use super::{Msg, Response, backend, project_files};
use crate::app::EpanetApp;
use crate::icons::Icon;

/// The cells a new notebook has: the first, run when the notebook opens,
/// opens the project with EPyT; the second analyses it and plots, the third
/// changes it (on the map at once).
const STARTER: [&str; 4] = [
    "# EPyT, with the project open in WEPANET
from epyt import epanet
d = epanet(wepanet.inp_file)
d.getNodeCount(), d.getLinkCount()",
    "# a hydraulic analysis with EPyT: the pressure at each node over time
import matplotlib.pyplot as plt
H = d.getComputedHydraulicTimeSeries()
plt.plot(H.Time / 3600, H.Pressure)
plt.xlabel(\"Time (hours)\")
plt.ylabel(\"Pressure\")
plt.title(\"Pressures (EPyT)\");",
    "# change the project with EPyT: the map shows it at once (Undo takes it back)
d.setNodeElevations(1, d.getNodeElevations(1) + 10)
float(d.getNodeElevations(1))",
    "",
];

/// The cells the Examples menu adds: they work on any project (the results
/// ones after an analysis, the EPyT ones after the first starter cell).
const EXAMPLES: [&str; 12] = [
    "help(wepanet)",
    "len(wepanet.node_ids()), len(wepanet.link_ids())",
    "wepanet.flow_units()",
    "[(n, wepanet.node_type(n)) for n in wepanet.node_ids()[:10]]",
    "wepanet.has_results(), wepanet.periods(), wepanet.current_period()",
    "n = wepanet.node_ids()[0]\nwepanet.node_result(n, 'Pressure', wepanet.current_period())",
    "wepanet.link_series(wepanet.link_ids()[0], 'Flow')",
    "p = wepanet.current_period()\nwepanet.table('Pressures', ['Node', 'Pressure'],\n\
              [[n, wepanet.node_result(n, 'Pressure', p)] for n in wepanet.node_ids()])",
    "wepanet.select_node(wepanet.node_ids()[0])",
    "print(open(wepanet.inp_file).read()[:600])",
    "d.getNodeNameID()[:10], d.getNodeElevations()[:10]",
    "d.getLinkLength()[:10], d.getLinkDiameter()[:10]",
];

/// What a cell showed: what it printed (or the error it stopped with), and
/// the value of its last line (`Out[n]:`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Output,
    Error,
    Value,
}

/// A cell: its code (or Markdown) and what its last run gave.
struct Cell {
    id: u64,
    source: String,
    /// A cell of Markdown: text, shown formatted; it does not run.
    markdown: bool,
    /// A Markdown cell shown as typed, to edit it (double-click); running it
    /// shows it formatted again.
    raw: bool,
    output: Vec<(Kind, String)>,
    /// The figures it drew, under what it printed.
    figures: Vec<Figure>,
    /// Its place in the session's runs (`In [3]`), once run.
    count: Option<u32>,
}

#[derive(Default)]
pub struct NotebookState {
    pub open: bool,
    cells: Vec<Cell>,
    next_id: u64,
    /// The runs of this session.
    runs: u32,
    /// The cells waiting to run.
    queue: VecDeque<u64>,
    /// The cell running.
    running: Option<u64>,
    /// What Python says while the running cell starts (loading, installing).
    progress: String,
    /// The cell to put the cursor in.
    focus: Option<u64>,
    /// The cell chosen (the bar at its left), the last one clicked or typed in.
    selected: Option<u64>,
    session: Option<backend::Session>,
    /// What the session's project was read from: (document, its edits, its
    /// analysis, the period shown).
    synced: Option<(u64, u64, u64, usize)>,
    /// The notebook's name: of the file it was opened from, for saving it.
    name: String,
    /// Notebook files opened from the computer.
    opened: crate::io::Inbox,
    /// Downloads (EPyT's examples).
    fetched: files::Inbox,
    /// EPyT's example notebooks (name, URL), once listed.
    epyt: Option<Result<Vec<(String, String)>, String>>,
    /// Asking for the list, or downloading one (its name).
    listing: bool,
    downloading: Option<String>,
    /// The address typed to open a notebook from.
    url: String,
    /// Why a notebook could not be opened or saved.
    problem: Option<String>,
    /// The dpi of the figures, for this screen (see [`figure_dpi`]), and of
    /// the cell running.
    dpi: u32,
    sent_dpi: u32,
    /// The figure opened in its window, and its zoom (none: to fit).
    viewing: Option<(Figure, Option<f32>)>,
}

impl NotebookState {
    fn add(&mut self, at: usize, source: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.cells.insert(
            at.min(self.cells.len()),
            Cell {
                id,
                source: source.to_string(),
                markdown: false,
                raw: false,
                output: Vec::new(),
                figures: Vec::new(),
                count: None,
            },
        );
        id
    }

    /// Replaces the cells by a notebook's, in a new session.
    fn load(&mut self, name: &str, cells: Vec<files::FileCell>) {
        self.restart();
        self.cells.clear();
        for (markdown, source) in cells {
            let id = self.add(self.cells.len(), &source);
            if let Some(c) = self.cell(id) {
                c.markdown = markdown;
            }
        }
        if self.cells.is_empty() {
            self.add(0, "");
        }
        self.name = name.to_string();
        self.selected = self.cells.first().map(|c| c.id);
        self.problem = None;
    }

    /// The notebook opened from a file, or downloaded.
    fn receive(&mut self) {
        for item in self.opened.drain() {
            match item {
                Ok((name, bytes)) => {
                    let name = name.strip_suffix(".ipynb").unwrap_or(&name).to_string();
                    match files::parse(&crate::io::decode_text(&bytes)) {
                        Ok(cells) => self.load(&name, cells),
                        Err(e) => self.problem = Some(format!("{name}: {e}")),
                    }
                }
                Err(e) => self.problem = Some(e),
            }
        }
        let fetched = match self.fetched.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return,
        };
        for f in fetched {
            match f {
                files::Fetched::List(list) => {
                    self.listing = false;
                    self.epyt = Some(list);
                }
                files::Fetched::Notebook(name, cells) => {
                    self.downloading = None;
                    match cells {
                        Ok(cells) => self.load(&name, cells),
                        Err(e) => self.problem = Some(format!("{name}: {e}")),
                    }
                }
            }
        }
    }

    fn cell(&mut self, id: u64) -> Option<&mut Cell> {
        self.cells.iter_mut().find(|c| c.id == id)
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.cells.iter().position(|c| c.id == id)
    }

    /// Ends the session (on the desktop, its Python); the next run starts a
    /// new one.
    fn restart(&mut self) {
        self.session = None;
        self.synced = None;
        self.queue.clear();
        self.running = None;
        self.progress.clear();
        self.runs = 0;
    }

    fn busy(&self) -> bool {
        self.running.is_some() || !self.queue.is_empty()
    }
}

/// What Python answers to a cell.
#[derive(Debug, Deserialize)]
struct Reply {
    #[serde(default)]
    output: String,
    /// The value of the last line.
    result: Option<String>,
    /// The figures drawn, PNG in base64.
    #[serde(default)]
    images: Vec<String>,
    #[serde(default)]
    error: bool,
    response: Option<Response>,
    network: Option<String>,
}

/// Opens the notebook, running its first cell (which loads EPyT), or closes
/// it (ending its Python on the desktop).
pub fn toggle(app: &mut EpanetApp) {
    let nb = &mut app.python.notebook;
    nb.open = !nb.open;
    if !nb.open {
        nb.restart();
        return;
    }
    app.prefs.docks.activate(crate::dock::Pane::Notebook);
    if app.python.notebook.cells.is_empty() {
        starter(app);
    } else {
        let first = app.python.notebook.cells[0].id;
        run(app, first);
    }
}

/// The starter cells, in place of the cells there are, in a new session:
/// the first runs.
fn starter(app: &mut EpanetApp) {
    let nb = &mut app.python.notebook;
    nb.load("", STARTER.iter().map(|s| (false, s.to_string())).collect());
    let first = nb.cells[0].id;
    nb.focus = nb.cells.get(1).map(|c| c.id);
    run(app, first);
}

/// Puts a cell in the queue and starts it when Python is free.
fn run(app: &mut EpanetApp, id: u64) {
    let nb = &mut app.python.notebook;
    // a Markdown cell is shown formatted
    if let Some(c) = nb.cell(id)
        && c.markdown
    {
        c.raw = false;
        return;
    }
    if nb.running != Some(id) && !nb.queue.contains(&id) {
        nb.queue.push_back(id);
        if let Some(c) = nb.cell(id) {
            c.output.clear();
            c.figures.clear();
            c.count = None;
        }
    }
    next(app);
}

/// Sends the next cell of the queue to Python, unless one is running.
fn next(app: &mut EpanetApp) {
    while app.python.notebook.running.is_none() {
        let Some(id) = app.python.notebook.queue.pop_front() else {
            return;
        };
        let Some(source) = app.python.notebook.cell(id).map(|c| c.source.clone()) else {
            continue;
        };
        let key = (
            app.document_id,
            app.generation,
            app.results_generation,
            app.period,
        );
        let stale = app.python.notebook.synced != Some(key);
        let files = stale.then(|| project_files(app, "Python Notebook", "<notebook>", None));
        let nb = &mut app.python.notebook;
        if nb.session.is_none() {
            match backend::Session::start() {
                Ok(s) => nb.session = Some(s),
                Err(e) => {
                    fail(nb, id, e);
                    continue;
                }
            }
        }
        // before the panel is drawn (the first cell), as for a screen of
        // 1 pixel per point
        if nb.dpi == 0 {
            nb.dpi = figure_dpi(1.0);
        }
        let s = nb.session.as_mut().expect("a session");
        match s.push(files, &source, nb.dpi) {
            Ok(()) => {
                nb.running = Some(id);
                nb.synced = Some(key);
                nb.sent_dpi = nb.dpi;
            }
            Err(e) => {
                nb.session = None;
                nb.synced = None;
                fail(nb, id, e);
            }
        }
    }
}

/// Shows why a cell could not run, and drops the cells queued after it.
fn fail(nb: &mut NotebookState, id: u64, e: String) {
    nb.queue.clear();
    if let Some(c) = nb.cell(id) {
        c.output.push((Kind::Error, e));
    }
}

/// Shows the notebook's answers.
pub(super) fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(s) = &app.python.notebook.session else {
        return;
    };
    let mut got = Vec::new();
    loop {
        match s.rx.try_recv() {
            Ok(m) => got.push(m),
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                got.push(Msg::Cell(Err("Python stopped".into())));
                break;
            }
        }
    }
    for m in got {
        match m {
            Msg::Progress(t) => app.python.notebook.progress = t,
            Msg::Cell(Ok(json)) => answer(app, ctx, &json),
            Msg::Cell(Err(e)) => {
                let nb = &mut app.python.notebook;
                if let Some(id) = nb.running {
                    fail(nb, id, e);
                }
                nb.restart();
            }
            Msg::Done(_) => {}
        }
    }
    next(app);
    if app.python.notebook.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Bytes from base64.
fn base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
    {
        acc = (acc << 6) | u32::from(value(c)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// A figure a cell drew: its picture, and its PNG (to save it).
#[derive(Clone)]
struct Figure {
    tex: egui::TextureHandle,
    png: Arc<Vec<u8>>,
    /// Its size in the cell, in points per pixel of the picture.
    points: f32,
}

/// The dpi Python draws figures at: twice the screen's pixels, so that a
/// figure is sharp in its cell (each pixel of the screen gets 2 × 2 of the
/// picture's) and still when zoomed in.
fn figure_dpi(pixels_per_point: f32) -> u32 {
    (96.0 * 2.0 * pixels_per_point).round() as u32
}

/// A figure Python drew at `dpi` (PNG in base64): it is shown as large as
/// at 96 dpi.
fn figure(ctx: &egui::Context, name: String, png: &str, dpi: u32) -> Option<Figure> {
    let bytes = base64(png)?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(Figure {
        // smaller than drawn: mipmaps keep its lines and text smooth
        tex: ctx.load_texture(
            name,
            color,
            egui::TextureOptions {
                mipmap_mode: Some(egui::TextureFilter::Linear),
                ..egui::TextureOptions::LINEAR
            },
        ),
        png: Arc::new(bytes),
        points: 96.0 / dpi.max(1) as f32,
    })
}

/// The window of a figure opened from a cell: it zooms (its buttons, or
/// Ctrl and the wheel), scrolls, and saves the figure as PNG.
fn viewer(ctx: &egui::Context, nb: &mut NotebookState) {
    let Some((fig, zoom)) = &mut nb.viewing else {
        return;
    };
    let mut open = true;
    let mut problem = None;
    egui::Window::new(crate::i18n::tr("Figure"))
        .id(egui::Id::new("python_notebook_figure"))
        .open(&mut open)
        .resizable(true)
        .default_size([820.0, 620.0])
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            let size = fig.tex.size_vec2();
            // the whole figure under the tools
            let fit = {
                let avail = ui.available_size() - vec2(0.0, 36.0);
                (avail.x / size.x).min(avail.y / size.y).max(0.05)
            };
            let now = zoom.unwrap_or(fit);
            ui.horizontal(|ui| {
                if crate::icons::icon_button(ui, Icon::ZoomIn, "Zoom In").clicked() {
                    *zoom = Some(now * 1.25);
                }
                if crate::icons::icon_button(ui, Icon::ZoomOut, "Zoom Out").clicked() {
                    *zoom = Some(now / 1.25);
                }
                if crate::icons::icon_button(ui, Icon::FullExtent, "Fit").clicked() {
                    *zoom = None;
                }
                if crate::icons::icon_button(ui, Icon::Save, "Save PNG").clicked()
                    && let Err(e) = crate::io::save_bytes(
                        &fig.png,
                        "figure.png",
                        "PNG image",
                        "png",
                        "image/png",
                    )
                {
                    problem = Some(e);
                }
                ui.label(RichText::new(format!("{:.0}%", now / fig.points * 100.0)).weak());
            });
            let wheel = ui.input(|i| i.zoom_delta());
            if wheel != 1.0 && ui.rect_contains_pointer(ui.max_rect()) {
                *zoom = Some(zoom.unwrap_or(now) * wheel);
            }
            if let Some(z) = zoom {
                *z = z.clamp(0.05, 8.0);
            }
            let z = zoom.unwrap_or(fit);
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                ui.add(egui::Image::new(&fig.tex).fit_to_exact_size(size * z));
            });
        });
    if let Some(e) = problem {
        nb.problem = Some(e);
    }
    if !open {
        nb.viewing = None;
    }
}

fn answer(app: &mut EpanetApp, ctx: &egui::Context, json: &str) {
    let nb = &mut app.python.notebook;
    let Some(id) = nb.running.take() else {
        return;
    };
    nb.progress.clear();
    nb.runs += 1;
    let runs = nb.runs;
    let r: Reply = match serde_json::from_str(json) {
        Ok(r) => r,
        Err(e) => {
            fail(nb, id, format!("a bad answer from Python: {e}"));
            return;
        }
    };
    let mut output = Vec::new();
    let out = r.output.trim_end();
    if !out.is_empty() {
        let kind = if r.error { Kind::Error } else { Kind::Output };
        output.push((kind, out.to_string()));
    }
    if let Some(v) = r.result {
        output.push((Kind::Value, v));
    }
    if r.error {
        // the cells after it would run on what it did not do
        nb.queue.clear();
    }
    let figures: Vec<Figure> = r
        .images
        .iter()
        .enumerate()
        .filter_map(|(k, png)| {
            let name = format!("notebook_figure_{id}_{runs}_{k}");
            figure(ctx, name, png, nb.sent_dpi)
        })
        .collect();
    if let Some(resp) = r.response {
        let problems = super::effects(
            app,
            "The Python Notebook",
            resp.status,
            resp.select,
            resp.table,
            r.network,
        );
        output.extend(problems.into_iter().map(|p| (Kind::Error, p)));
    }
    if let Some(c) = app.python.notebook.cell(id) {
        c.output = output;
        c.figures = figures;
        c.count = Some(runs);
    }
}

/// A small button with an icon, for a cell.
fn cell_button(ui: &mut Ui, id: egui::Id, icon: Icon, tip: &str) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
    let resp = ui.interact(rect, id, egui::Sense::click());
    let style = ui.style().interact(&resp);
    if resp.hovered() {
        ui.painter().rect(
            rect,
            3.0,
            style.weak_bg_fill,
            style.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }
    crate::icons::paint(ui.painter(), icon, rect.center(), 16.0, style.text_color());
    resp.on_hover_text(crate::i18n::tr(tip))
}

/// The notebook's colors, light or dark.
struct Colors {
    /// `In [n]:`
    input: Color32,
    /// `Out[n]:`
    value: Color32,
    text: Color32,
    weak: Color32,
    error: Color32,
    error_fill: Color32,
    code_fill: Color32,
    code_stroke: Color32,
    /// The bar at the left of the cell chosen, and of the one typed in.
    chosen: Color32,
    editing: Color32,
}

impl Colors {
    fn of(ui: &Ui) -> Colors {
        let v = ui.visuals();
        if v.dark_mode {
            Colors {
                input: Color32::from_rgb(0x8c, 0x9e, 0xff),
                value: Color32::from_rgb(0xff, 0x8a, 0x65),
                text: v.text_color(),
                weak: v.weak_text_color(),
                error: Color32::from_rgb(0xf2, 0x8b, 0x82),
                error_fill: Color32::from_rgb(0x3c, 0x22, 0x22),
                code_fill: Color32::from_gray(0x2a),
                code_stroke: Color32::from_gray(0x4a),
                chosen: Color32::from_rgb(0x42, 0xa5, 0xf5),
                editing: Color32::from_rgb(0x66, 0xbb, 0x6a),
            }
        } else {
            Colors {
                input: Color32::from_rgb(0x30, 0x3f, 0x9f),
                value: Color32::from_rgb(0xd8, 0x43, 0x15),
                text: v.text_color(),
                weak: v.weak_text_color(),
                error: Color32::from_rgb(0xb7, 0x1c, 0x1c),
                error_fill: Color32::from_rgb(0xff, 0xdd, 0xdd),
                code_fill: Color32::from_gray(0xf7),
                code_stroke: Color32::from_gray(0xcf),
                chosen: Color32::from_rgb(0x42, 0xa5, 0xf5),
                editing: Color32::from_rgb(0x66, 0xbb, 0x6a),
            }
        }
    }
}

/// Width of the prompts (`In [12]:`) left of the cells.
const PROMPT_W: f32 = 64.0;

/// The notebook's contents, in its panel of the dock: the tools, then the
/// cells.
pub fn pane(app: &mut EpanetApp, ui: &mut Ui) {
    let mut runs: Vec<u64> = Vec::new();
    let mut add: Option<(usize, String, bool)> = None;
    app.python.notebook.dpi = figure_dpi(ui.ctx().pixels_per_point());
    let mut starter_now = false;
    // a figure clicked: (its cell, its place)
    let mut open_figure: Option<(u64, usize)> = None;
    app.python.notebook.receive();
    let busy = app.python.notebook.busy();
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let nb = &mut app.python.notebook;
                file_tools(ui, nb);
                ui.separator();
                if crate::icons::icon_button(ui, Icon::Analyze, "Run All")
                    .on_hover_text(crate::i18n::tr("Runs the cells from the first"))
                    .clicked()
                {
                    runs.extend(nb.cells.iter().map(|c| c.id));
                }
                if crate::icons::icon_button(ui, Icon::Add, "Add Cell").clicked() {
                    // under the cell chosen, as in a notebook
                    let at = nb
                        .selected
                        .and_then(|s| nb.index(s))
                        .map_or(nb.cells.len(), |k| k + 1);
                    add = Some((at, String::new(), false));
                }
                let examples = ui.button(crate::i18n::tr("WEPANET Examples"));
                egui::Popup::menu(&examples).show(|ui| {
                    if ui
                        .button(crate::i18n::tr("Starter Notebook"))
                        .on_hover_text(crate::i18n::tr(
                            "The cells the notebook opens with: EPyT on the project, a plot, a change on the map. They replace these.",
                        ))
                        .clicked()
                    {
                        starter_now = true;
                    }
                    ui.separator();
                    ui.label(
                        RichText::new(crate::i18n::tr("Click one to add it as a cell and run it."))
                            .weak(),
                    );
                    for code in EXAMPLES {
                        let text = RichText::new(code).font(egui::FontId::monospace(12.0));
                        let button = egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend);
                        if ui.add(button).clicked() {
                            add = Some((nb.cells.len(), code.to_string(), true));
                        }
                    }
                });
                if ui
                    .button(crate::i18n::tr("Restart"))
                    .on_hover_text(crate::i18n::tr(
                        "Starts Python again: the variables of the cells are forgotten",
                    ))
                    .clicked()
                {
                    nb.restart();
                    for c in &mut nb.cells {
                        c.count = None;
                    }
                }
                if ui.button(crate::i18n::tr("Clear")).clicked() {
                    for c in &mut nb.cells {
                        c.output.clear();
                        c.figures.clear();
                    }
                }
                if busy {
                    ui.spinner();
                }
                if !nb.name.is_empty() {
                    ui.label(RichText::new(&nb.name).weak());
                }
            });
            if let Some(p) = &app.python.notebook.problem {
                ui.colored_label(ui.visuals().error_fg_color, p);
            }
        });
    ui.separator();
    let mono = egui::FontId::monospace(13.0);
    let col = Colors::of(ui);
    let mut delete = None;
    let mut move_to = None;
    let mut chosen = None;
    let mut edit_now = None;
    egui::ScrollArea::vertical()
        .id_salt("python_notebook_cells")
        .auto_shrink(false)
        .show(ui, |ui| {
            let nb = &mut app.python.notebook;
            let (running, queue, selected) = (nb.running, nb.queue.clone(), nb.selected);
            let focus = nb.focus.take();
            let n = nb.cells.len();
            for (k, cell) in nb.cells.iter_mut().enumerate() {
                let id = egui::Id::new("python_notebook_cell").with(cell.id);
                let waiting = running == Some(cell.id) || queue.contains(&cell.id);
                let editing = ui.memory(|m| m.has_focus(id));
                // Shift+Enter runs the cell and goes to the next, Ctrl+Enter
                // runs it and stays (before the editor takes the Enter)
                if editing {
                    if ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Enter)) {
                        runs.push(cell.id);
                        if k + 1 == n {
                            add = Some((n, String::new(), false));
                        } else {
                            move_to = Some(k + 1);
                        }
                    } else if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter)) {
                        runs.push(cell.id);
                    }
                }
                let shown = egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 8,
                        right: 4,
                        top: 4,
                        bottom: 4,
                    })
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            let label = match (cell.markdown, waiting, cell.count) {
                                (true, _, _) => String::new(),
                                (false, true, _) => "In [*]:".to_string(),
                                (false, false, Some(c)) => format!("In [{c}]:"),
                                (false, false, None) => "In [ ]:".to_string(),
                            };
                            prompt(ui, &label, col.input, &mono);
                            let w = (ui.available_width() - 30.0).max(80.0);
                            if cell.markdown && !cell.raw {
                                let shown = ui
                                    .vertical(|ui| {
                                        ui.set_width(w);
                                        markdown(ui, &cell.source, &col);
                                    })
                                    .response;
                                // a double-click shows it as typed, to edit it
                                let twice = ui.input(|i| {
                                    i.pointer
                                        .button_double_clicked(egui::PointerButton::Primary)
                                        && i.pointer
                                            .interact_pos()
                                            .is_some_and(|p| shown.rect.contains(p))
                                });
                                if twice {
                                    cell.raw = true;
                                    edit_now = Some(cell.id);
                                }
                            } else {
                                let rows = cell.source.lines().count().max(1);
                                let font = if cell.markdown {
                                    egui::FontId::proportional(14.0)
                                } else {
                                    mono.clone()
                                };
                                let edit = egui::Frame::NONE
                                    .fill(col.code_fill)
                                    .stroke(egui::Stroke::new(1.0, col.code_stroke))
                                    .corner_radius(2)
                                    .inner_margin(4)
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::TextEdit::multiline(&mut cell.source)
                                                .id(id)
                                                .code_editor()
                                                .frame(egui::Frame::NONE)
                                                .font(font)
                                                .desired_rows(rows)
                                                .desired_width(w - 10.0)
                                                .lock_focus(true),
                                        )
                                    })
                                    .inner;
                                if focus == Some(cell.id) {
                                    edit.request_focus();
                                }
                                if edit.has_focus() || edit.clicked() {
                                    chosen = Some(cell.id);
                                }
                            }
                            ui.vertical(|ui| {
                                if !cell.markdown
                                    && cell_button(
                                        ui,
                                        id.with("run"),
                                        Icon::Analyze,
                                        "Run the cell (Shift+Enter)",
                                    )
                                    .clicked()
                                {
                                    runs.push(cell.id);
                                }
                                if cell_button(
                                    ui,
                                    id.with("delete"),
                                    Icon::Delete,
                                    "Delete the cell",
                                )
                                .clicked()
                                {
                                    delete = Some(cell.id);
                                }
                            });
                        });
                        if running == Some(cell.id) && cell.output.is_empty() {
                            if !nb.progress.is_empty() {
                                ui.horizontal_top(|ui| {
                                    ui.add_space(PROMPT_W + ui.spacing().item_spacing.x);
                                    ui.label(
                                        RichText::new(&nb.progress)
                                            .font(mono.clone())
                                            .color(col.weak),
                                    );
                                });
                            }
                        } else {
                            if let Some(k) = outputs(ui, id, cell, &col, &mono) {
                                open_figure = Some((cell.id, k));
                            }
                        }
                    });
                let rect = shown.response.rect;
                // the bar of the cell chosen: green while typing in it
                if selected == Some(cell.id) || editing {
                    let bar = egui::Rect::from_min_size(rect.min, vec2(4.0, rect.height()));
                    let c = if editing { col.editing } else { col.chosen };
                    ui.painter().rect_filled(bar, 0.0, c);
                }
                // a click anywhere in the cell chooses it (and still reaches
                // what is under the pointer)
                let clicked = ui.input(|i| {
                    i.pointer.primary_clicked()
                        && i.pointer.interact_pos().is_some_and(|p| rect.contains(p))
                });
                if clicked && ui.is_rect_visible(rect) {
                    chosen = Some(cell.id);
                }
            }
            ui.add_space(8.0);
        });
    let nb = &mut app.python.notebook;
    if let Some(c) = chosen {
        nb.selected = Some(c);
    }
    if let Some(k) = move_to {
        nb.focus = nb.cells.get(k).map(|c| c.id);
    }
    if edit_now.is_some() {
        nb.focus = edit_now;
    }
    if let Some(id) = delete
        && let Some(k) = nb.index(id)
    {
        nb.cells.remove(k);
        nb.queue.retain(|q| *q != id);
    }
    if let Some((at, source, now)) = add {
        let id = nb.add(at, &source);
        nb.focus = Some(id);
        if now {
            runs.push(id);
        }
    }
    if let Some(f) = nb.focus {
        nb.selected = Some(f);
    }
    if let Some(k) = open_figure
        && let Some(f) = nb
            .cells
            .iter()
            .find_map(|c| c.figures.get(k.1).filter(|_| c.id == k.0))
    {
        nb.viewing = Some((f.clone(), None));
    }
    viewer(ui.ctx(), nb);
    if starter_now {
        starter(app);
    }
    for id in runs {
        run(app, id);
    }
    if app.python.notebook.focus.is_some() {
        ui.ctx().request_repaint();
    }
}

/// The tools for notebook files: open one, save this one, open one of
/// EPyT's examples.
fn file_tools(ui: &mut Ui, nb: &mut NotebookState) {
    let id = egui::Id::new("python_notebook_file");
    // a notebook (.ipynb) from this computer or from an address
    let open = "Opens a notebook (.ipynb): its cells replace these";
    let button = cell_button(ui, id.with("open"), Icon::Open, open);
    egui::Popup::menu(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(420.0);
            if ui.button(crate::i18n::tr("From This Computer…")).clicked() {
                crate::io::open_file(&nb.opened, ui.ctx(), "Notebook", &["ipynb"]);
                ui.close();
            }
            ui.separator();
            ui.label(crate::i18n::tr(
                "From an address: a GitHub link or a raw URL",
            ));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut nb.url)
                    .hint_text("https://github.com/…/notebook.ipynb")
                    .desired_width(f32::INFINITY),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if (ui.button(crate::i18n::tr("Open")).clicked() || enter) && !nb.url.trim().is_empty()
            {
                let (name, url) = files::address(&nb.url);
                nb.downloading = Some(name.clone());
                files::download(&name, &url, &nb.fetched, ui.ctx());
                ui.close();
            }
        });
    let save = "Saves the cells as a notebook (.ipynb)";
    if cell_button(ui, id.with("save"), Icon::Save, save).clicked() {
        let cells: Vec<(bool, &str)> = nb
            .cells
            .iter()
            .map(|c| (c.markdown, c.source.as_str()))
            .collect();
        let name = if nb.name.is_empty() {
            "notebook"
        } else {
            &nb.name
        };
        let file = format!("{name}.ipynb");
        if let Err(e) = crate::io::save_report(&files::write(&cells), &file, ui.ctx()) {
            nb.problem = Some(e);
        }
    }
    let examples = ui
        .button(crate::i18n::tr("EPyT Examples"))
        .on_hover_text(crate::i18n::tr(
            "EPyT's example notebooks, from its repository: click one to open it",
        ));
    egui::Popup::menu(&examples).show(|ui| {
        // as wide while it loads as once listed
        ui.set_min_width(300.0);
        if nb.epyt.is_none() && !nb.listing {
            nb.listing = true;
            files::list_epyt_examples(&nb.fetched, ui.ctx());
        }
        match &nb.epyt {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(crate::i18n::tr("Loading…"));
                });
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
                if ui.button(crate::i18n::tr("Try Again")).clicked() {
                    nb.epyt = None;
                }
            }
            Some(Ok(list)) => {
                ui.label(
                    RichText::new(crate::i18n::tr(
                        "Its cells replace these; Run All runs them.",
                    ))
                    .weak(),
                );
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .min_scrolled_height(420.0)
                    .show(ui, |ui| {
                        for (name, url) in list {
                            if ui.button(name).clicked() {
                                nb.downloading = Some(name.clone());
                                files::download(name, url, &nb.fetched, ui.ctx());
                            }
                        }
                    });
            }
        }
    });
    if let Some(name) = &nb.downloading {
        ui.spinner();
        ui.label(RichText::new(name).weak());
    }
}

/// A Markdown cell, formatted: headings larger, list items with bullets.
fn markdown(ui: &mut Ui, text: &str, col: &Colors) {
    let plain = |s: &str| s.replace("**", "").replace('`', "");
    if text.trim().is_empty() {
        ui.label(RichText::new(crate::i18n::tr("Double-click to edit")).color(col.weak));
    }
    for line in text.lines() {
        let t = line.trim_end();
        let hashes = t.chars().take_while(|c| *c == '#').count();
        let item = t.trim_start();
        if hashes > 0 && t[hashes..].starts_with(' ') {
            let size = [22.0, 19.0, 16.0][(hashes - 1).min(2)];
            let heading = RichText::new(plain(t[hashes..].trim())).size(size).strong();
            ui.label(heading.color(col.text));
        } else if let Some(i) = item.strip_prefix("- ").or_else(|| item.strip_prefix("* ")) {
            ui.add(egui::Label::new(format!("  •  {}", plain(i))).wrap());
        } else if t.is_empty() {
            ui.add_space(4.0);
        } else {
            ui.add(egui::Label::new(plain(t)).wrap());
        }
    }
}

/// A prompt left of a cell or of its value, right-aligned as in a notebook.
fn prompt(ui: &mut Ui, text: &str, color: Color32, font: &egui::FontId) {
    ui.allocate_ui_with_layout(
        vec2(PROMPT_W, 22.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.set_min_width(PROMPT_W);
            ui.label(RichText::new(text).font(font.clone()).color(color));
        },
    );
}

/// What a cell showed, under it: what it printed, its errors (on red), its
/// value after `Out[n]:`, its figures; and the button that copies the text.
fn outputs(
    ui: &mut Ui,
    id: egui::Id,
    cell: &Cell,
    col: &Colors,
    mono: &egui::FontId,
) -> Option<usize> {
    if cell.output.is_empty() && cell.figures.is_empty() {
        return None;
    }
    let count = cell.count.map_or(String::new(), |c| c.to_string());
    for (k, (kind, t)) in cell.output.iter().enumerate() {
        ui.horizontal_top(|ui| {
            if *kind == Kind::Value {
                prompt(ui, &format!("Out[{count}]:"), col.value, mono);
            } else {
                ui.add_space(PROMPT_W);
            }
            // the copy button at the right of the first block
            let w = ui.available_width() - 22.0 - ui.spacing().item_spacing.x;
            let color = if *kind == Kind::Error {
                col.error
            } else {
                col.text
            };
            let fill = if *kind == Kind::Error {
                col.error_fill
            } else {
                Color32::TRANSPARENT
            };
            egui::Frame::NONE
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.set_width(w.max(80.0) - 8.0);
                    ui.add(
                        egui::Label::new(RichText::new(t).font(mono.clone()).color(color))
                            .wrap()
                            .selectable(true),
                    );
                });
            if k == 0
                && cell_button(ui, id.with("copy"), Icon::Copy, "Copy what the cell showed")
                    .clicked()
            {
                let all: Vec<&str> = cell.output.iter().map(|(_, t)| t.as_str()).collect();
                ui.ctx().copy_text(all.join("\n"));
            }
        });
    }
    let mut clicked = None;
    for (k, fig) in cell.figures.iter().enumerate() {
        ui.horizontal_top(|ui| {
            ui.add_space(PROMPT_W + ui.spacing().item_spacing.x);
            let w = (ui.available_width() - 8.0).max(40.0);
            let mut size = fig.tex.size_vec2() * fig.points;
            if size.x > w {
                size *= w / size.x;
            }
            let image = egui::Image::new(&fig.tex)
                .fit_to_exact_size(size)
                .sense(egui::Sense::click());
            let resp = ui
                .add(image)
                .on_hover_cursor(egui::CursorIcon::ZoomIn)
                .on_hover_text(crate::i18n::tr("Click to open it larger"));
            if resp.clicked() {
                clicked = Some(k);
            }
        });
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the queued cells with the Python of `WEPANET_PYTHON`.
    fn finish(app: &mut EpanetApp, ctx: &egui::Context) {
        let t = std::time::Instant::now();
        while app.python.notebook.busy() && t.elapsed().as_secs() < 180 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            poll(app, ctx);
        }
        assert!(!app.python.notebook.busy(), "the cells did not finish");
    }

    /// Adds a cell at the end, runs it, and gives what it showed.
    fn run_new(app: &mut EpanetApp, ctx: &egui::Context, source: &str) -> Vec<(Kind, String)> {
        let nb = &mut app.python.notebook;
        let id = nb.add(nb.cells.len(), source);
        run(app, id);
        finish(app, ctx);
        let c = app.python.notebook.cell(id).expect("the cell");
        assert!(c.count.is_some(), "{source}: not run {:?}", c.output);
        c.output.clone()
    }

    /// With the Python of `WEPANET_PYTHON` (with EPyT): opening the notebook
    /// loads EPyT, the cells share their variables, show their last value
    /// and their errors, see the project as it is, and show what they ask.
    #[test]
    fn the_notebook_runs_cells_in_one_session() {
        if std::env::var("WEPANET_PYTHON").is_err() {
            return;
        }
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        toggle(&mut app);
        finish(&mut app, &ctx);
        let first = &app.python.notebook.cells[0];
        assert_eq!(first.count, Some(1), "{:?}", first.output);
        assert!(
            first.output.iter().all(|(k, _)| *k != Kind::Error)
                && first.output.last() == Some(&(Kind::Value, "(11, 13)".into())),
            "{:?}",
            first.output
        );
        // the second starter cell analyses with EPyT and plots, the third
        // changes the project
        let (second, third) = (
            app.python.notebook.cells[1].id,
            app.python.notebook.cells[2].id,
        );
        let elevation = |app: &EpanetApp| app.net.nodes[0].num(0).unwrap();
        let before = elevation(&app);
        // an analysis leaves the project as it is (EPANET saves a curve's
        // type after it)
        let g = app.generation;
        run(&mut app, second);
        finish(&mut app, &ctx);
        assert_eq!(app.generation, g);
        run(&mut app, third);
        finish(&mut app, &ctx);
        let c = app.python.notebook.cell(second).unwrap();
        assert!(c.output.is_empty(), "{:?}", c.output);
        assert_eq!(c.figures.len(), 1);
        let c = app.python.notebook.cell(third).unwrap();
        assert_eq!(c.output, [(Kind::Value, format!("{:?}", before + 10.0))]);
        assert_eq!(elevation(&app), before + 10.0);
        app.undo();
        // variables last, the last line's value shows, `;` hides it
        assert!(run_new(&mut app, &ctx, "x = len(wepanet.node_ids())").is_empty());
        assert_eq!(
            run_new(&mut app, &ctx, "y = x * 2\ny"),
            [(Kind::Value, "22".into())]
        );
        assert!(run_new(&mut app, &ctx, "y;").is_empty());
        assert_eq!(
            run_new(&mut app, &ctx, "for i in range(2):\n    print('n', i)"),
            [(Kind::Output, "n 0\nn 1".into())]
        );
        // an error shows from the cell's line, and stops the cells after it
        let nb = &mut app.python.notebook;
        let bad = nb.add(nb.cells.len(), "a = 1\n1/0");
        let after = nb.add(nb.cells.len(), "print('after')");
        run(&mut app, bad);
        run(&mut app, after);
        finish(&mut app, &ctx);
        let out = &app.python.notebook.cell(bad).unwrap().output;
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].0, Kind::Error);
        assert!(
            out[0].1.contains("<cell") && out[0].1.ends_with("ZeroDivisionError: division by zero"),
            "{out:?}"
        );
        assert!(!out[0].1.contains("python_runner"), "{out:?}");
        assert_eq!(app.python.notebook.cell(after).unwrap().count, None);
        // the project as it is now: an edit is seen by the next cell
        let n = app.net.nodes.len();
        app.checkpoint();
        app.net.nodes.truncate(n - 1);
        app.touch();
        assert_eq!(
            run_new(&mut app, &ctx, "len(wepanet.node_ids())"),
            [(Kind::Value, (n - 1).to_string())]
        );
        // a cell can replace the project
        let before = app.net.nodes.len();
        run_new(
            &mut app,
            &ctx,
            "wepanet.set_network(open(wepanet.inp_file).read().replace('[JUNCTIONS]', '[JUNCTIONS]\\n J99 1 0', 1))",
        );
        assert_eq!(app.net.nodes.len(), before + 1);
        // the examples run without errors, with results and EPyT's `d`
        app.dirty = false;
        app.load_example("Net1", eui_core::examples::NET1);
        let out = eui_engine::run_blocking(eui_core::examples::NET1).unwrap();
        app.results = Some(eui_core::SimResults::from_out(&out.out).unwrap());
        app.results_generation = app.generation;
        let first = app.python.notebook.cells[0].id;
        run(&mut app, first);
        finish(&mut app, &ctx);
        for code in EXAMPLES {
            let out = run_new(&mut app, &ctx, code);
            assert!(
                out.iter().all(|(k, _)| *k != Kind::Error),
                "{code}: {out:?}"
            );
            assert!(
                !out.is_empty() || code.contains("select_node") || code.contains(".table("),
                "{code}: {out:?}"
            );
        }
        // a change to the EPyT project shows on the map at once, Undo takes
        // it back; a cell that does not change it changes nothing
        let elevation = |app: &EpanetApp| app.net.nodes[0].num(0);
        let before = elevation(&app);
        run_new(&mut app, &ctx, "d.setNodeElevations(1, 999.5)");
        assert_eq!(elevation(&app), Some(999.5));
        let g = app.generation;
        run_new(&mut app, &ctx, "d.getNodeElevations(1)");
        assert_eq!(app.generation, g);
        app.undo();
        assert_eq!(elevation(&app), before);
        // other networks a notebook opens (EPyT's examples open EPyT's own)
        // stay off the map
        let g = app.generation;
        let other = "e = epanet('Net2.inp')\ne.setNodeElevations(1, 1.5)";
        run_new(&mut app, &ctx, other);
        assert_eq!(app.generation, g);
        // a module that is missing: how to install it
        let missing = run_new(&mut app, &ctx, "import PIL_is_not_a_module");
        assert!(
            missing[0].1.ends_with("%pip install PIL_is_not_a_module"),
            "{missing:?}"
        );
        // figures are shown under their cell
        let nb = &mut app.python.notebook;
        let fig = nb.add(
            nb.cells.len(),
            "import matplotlib.pyplot as plt
plt.plot([1, 3, 2]);",
        );
        run(&mut app, fig);
        finish(&mut app, &ctx);
        let c = app.python.notebook.cell(fig).unwrap();
        assert!(c.output.is_empty(), "{:?}", c.output);
        assert_eq!(c.figures.len(), 1);
        assert!(c.figures[0].tex.size()[0] > 100);
        // Restart forgets the variables
        app.python.notebook.restart();
        let out = run_new(&mut app, &ctx, "x");
        assert!(out[0].1.contains("NameError"), "{out:?}");
        toggle(&mut app);
        assert!(app.python.notebook.session.is_none());
    }

    /// The texts drawn, and where.
    fn texts(out: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        out.shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => Some((
                    t.galley.text().to_string(),
                    t.visual_bounding_rect().intersect(c.clip_rect),
                )),
                _ => None,
            })
            .collect()
    }

    /// A frame of the dock (the notebook floats in it).
    fn frame(
        ctx: &egui::Context,
        app: &mut EpanetApp,
        f: usize,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events,
            time: Some(f as f64 * 0.1),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::dock::show(app, ui));
        out.textures_delta.clear();
        out
    }

    /// The notebook is a panel of the dock, under the map at first (also in
    /// layouts saved before it), with its tools, its cells, their numbers
    /// and what they showed under them, which its button copies.
    #[test]
    fn the_notebook_is_a_panel_of_the_dock() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        let nb = &mut app.python.notebook;
        nb.open = true;
        let a = nb.add(0, "x = 1\nx + 1");
        nb.add(1, "1/0");
        nb.cell(a).unwrap().count = Some(3);
        nb.cell(a).unwrap().output = vec![(Kind::Value, "2".into())];
        nb.cells[1].output = vec![(Kind::Error, "ZeroDivisionError: division by zero".into())];
        let notebook = crate::dock::Group {
            panes: vec![crate::dock::Pane::Notebook],
            active: crate::dock::Pane::Notebook,
        };
        assert_eq!(app.prefs.docks.bottom, std::slice::from_ref(&notebook));
        let mut out = frame(&ctx, &mut app, 0, Vec::new());
        for f in 1..3 {
            out = frame(&ctx, &mut app, f, Vec::new());
        }
        let shown = texts(&out);
        for want in [
            "Python Notebook",
            "Run All",
            "Add Cell",
            "WEPANET Examples",
            "Restart",
            "Clear",
            "In [3]:",
            "In [ ]:",
            "x = 1\nx + 1",
            "2",
            "Out[3]:",
            "ZeroDivisionError: division by zero",
        ] {
            assert!(
                shown.iter().any(|(t, r)| t == want && r.height() > 0.0),
                "{want}: {shown:?}"
            );
        }
        let at = |s: &str| shown.iter().find(|(t, _)| t == s).unwrap().1;
        assert!(at("x = 1\nx + 1").bottom() <= at("2").top());
        assert!(at("2").bottom() <= at("1/0").top());
        // under the map: at the bottom of the window
        assert!(at("ZeroDivisionError: division by zero").bottom() < 800.0);
        assert!(at("Python Notebook").top() > 400.0, "{shown:?}");
        // the copy button copies the error
        let error = app.python.notebook.cells[1].id;
        let copy = egui::Id::new("python_notebook_cell")
            .with(error)
            .with("copy");
        let c = ctx
            .read_response(copy)
            .expect("a copy button")
            .rect
            .center();
        let button = |pressed| egui::Event::PointerButton {
            pos: c,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut app, 3, vec![egui::Event::PointerMoved(c)]);
        frame(&ctx, &mut app, 4, vec![button(true)]);
        let out = frame(&ctx, &mut app, 5, vec![button(false)]);
        assert!(
            out.platform_output.commands.iter().any(|c| matches!(
                c,
                egui::OutputCommand::CopyText(t) if t == "ZeroDivisionError: division by zero"
            )),
            "{:?}",
            out.platform_output.commands
        );
        // a layout saved without it gets it, under the map
        app.prefs.docks.bottom.clear();
        frame(&ctx, &mut app, 6, Vec::new());
        assert_eq!(app.prefs.docks.bottom, [notebook]);
    }

    /// Shift+Enter runs the cell and goes on to the next (a new one after
    /// the last), without breaking the line.
    #[test]
    fn shift_enter_runs_the_cell() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        let nb = &mut app.python.notebook;
        nb.open = true;
        let a = nb.add(0, "1 + 1");
        let b = nb.add(1, "2 + 2");
        nb.focus = Some(a);
        // a cell "running", so that none is sent to Python
        nb.running = Some(u64::MAX);
        frame(&ctx, &mut app, 0, Vec::new());
        let shift_enter = || egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::SHIFT,
        };
        frame(&ctx, &mut app, 1, vec![shift_enter()]);
        let nb = &app.python.notebook;
        assert_eq!(nb.queue, [a]);
        assert_eq!(nb.cells[0].source, "1 + 1");
        assert_eq!(nb.focus, Some(b));
        frame(&ctx, &mut app, 2, Vec::new());
        frame(&ctx, &mut app, 3, vec![shift_enter()]);
        let nb = &app.python.notebook;
        assert_eq!(nb.queue, [a, b]);
        assert_eq!(nb.cells[1].source, "2 + 2");
        assert_eq!(nb.cells.len(), 3);
        assert_eq!(nb.focus, Some(nb.cells[2].id));
        let out = frame(&ctx, &mut app, 4, Vec::new());
        assert_eq!(
            texts(&out).iter().filter(|(t, _)| t == "In [*]:").count(),
            2
        );
    }

    #[test]
    fn a_figure_opens_larger_in_its_window() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        let nb = &mut app.python.notebook;
        nb.open = true;
        let a = nb.add(0, "plt.plot([1, 2])");
        // a figure of 600 × 300 pixels at 192 dpi: 300 × 150 in the cell
        let color = egui::ColorImage::new([600, 300], vec![Color32::RED; 600 * 300]);
        let tex = ctx.load_texture("test_figure", color, egui::TextureOptions::LINEAR);
        let tid = tex.id();
        nb.cell(a).unwrap().figures = vec![Figure {
            tex,
            png: Arc::new(vec![1, 2, 3]),
            points: 0.5,
        }];
        nb.running = Some(u64::MAX);
        let image = |out: &egui::FullOutput| {
            let mut rects = Vec::new();
            for s in &out.shapes {
                if let egui::Shape::Rect(r) = &s.shape
                    && r.fill_texture_id() == tid
                {
                    rects.push(r.rect.intersect(s.clip_rect));
                }
            }
            rects
        };
        frame(&ctx, &mut app, 0, Vec::new());
        let out = frame(&ctx, &mut app, 1, Vec::new());
        let shown = image(&out);
        assert_eq!(shown.len(), 1, "{shown:?}");
        // (its top half, above the bottom of the panel)
        assert_eq!(shown[0].width(), 300.0);
        let c = shown[0].center();
        let click = |pressed| egui::Event::PointerButton {
            pos: c,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut app, 2, vec![egui::Event::PointerMoved(c)]);
        frame(&ctx, &mut app, 3, vec![click(true)]);
        frame(&ctx, &mut app, 4, vec![click(false)]);
        assert!(app.python.notebook.viewing.is_some());
        let out = frame(&ctx, &mut app, 5, Vec::new());
        let shown = texts(&out);
        for want in ["Figure", "Zoom In", "Zoom Out", "Fit", "Save PNG"] {
            assert!(shown.iter().any(|(t, _)| t == want), "{want}: {shown:?}");
        }
        // the window shows it larger than the cell, to fit
        let big = image(&out);
        assert!(big.iter().any(|r| r.width() > 500.0), "{big:?}");
    }

    #[test]
    fn the_starter_notebook_comes_back() {
        let (_ctx, mut app) = crate::msx::tests::new_app();
        let nb = &mut app.python.notebook;
        nb.load("EX1", vec![(false, "x = 1".into())]);
        starter(&mut app);
        let nb = &app.python.notebook;
        let cells: Vec<&str> = nb.cells.iter().map(|c| c.source.as_str()).collect();
        assert_eq!(cells, STARTER);
        assert!(nb.name.is_empty());
        assert_eq!(nb.focus, Some(nb.cells[1].id));
    }

    #[test]
    fn notebook_files_open_with_their_markdown() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        let nb = &mut app.python.notebook;
        nb.open = true;
        nb.add(0, "old = 1");
        let file = r##"{"cells": [
            {"cell_type": "markdown", "source": ["# Leakage\n", "- a list item"]},
            {"cell_type": "code", "source": ["x = 2\n", "x * 3"]}
        ], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"##;
        nb.opened
            .push(Ok(("EX28_Leakage.ipynb".into(), file.as_bytes().to_vec())));
        frame(&ctx, &mut app, 0, Vec::new());
        let out = frame(&ctx, &mut app, 1, Vec::new());
        let nb = &app.python.notebook;
        assert_eq!(nb.name, "EX28_Leakage");
        assert_eq!(nb.cells.len(), 2);
        assert!(nb.cells[0].markdown && !nb.cells[1].markdown);
        let shown = texts(&out);
        for want in [
            "EPyT Examples",
            "EX28_Leakage",
            "Leakage",
            "  •  a list item",
            "x = 2\nx * 3",
            "In [ ]:",
        ] {
            assert!(shown.iter().any(|(t, _)| t == want), "{want}: {shown:?}");
        }
        // the Markdown is formatted, not typed
        assert!(!shown.iter().any(|(t, _)| t.contains("# Leakage")));
        assert!(!shown.iter().any(|(t, _)| t == "old = 1"));
        // running a Markdown cell sends nothing to Python
        let md = nb.cells[0].id;
        app.python.notebook.cells[0].raw = true;
        run(&mut app, md);
        let nb = &app.python.notebook;
        assert!(nb.queue.is_empty() && nb.running.is_none() && nb.session.is_none());
        assert!(!nb.cells[0].raw);
        // a file that is not a notebook
        nb.opened.push(Ok(("x.ipynb".into(), b"print(1)".to_vec())));
        frame(&ctx, &mut app, 2, Vec::new());
        let nb = &app.python.notebook;
        assert_eq!(nb.cells.len(), 2);
        assert!(
            nb.problem
                .as_ref()
                .unwrap()
                .starts_with("x: not a notebook")
        );
    }
}
