//! EPANET-MSX, the multi-species extension of EPANET, in the app.
//!
//! The project's MSX input file lives with the network ([`Network::msx`]), so
//! undo, renaming and deleting elements and saving cover it. Its species are
//! set up like EPANET's water quality: per node an initial concentration and
//! a source, per pipe an initial concentration, per pipe and tank the model's
//! parameters, all in the Property Editor (see `explorer.rs`). The
//! EPANET-MSX dialog loads, writes or edits the file, sets the global initial
//! concentrations and the coefficients, lists the sources, and explores the
//! results: a species on the map at the time shown or as its minimum,
//! maximum or mean, and plots of species at chosen nodes.
//!
//! [`Network::msx`]: eui_core::Network::msx

use egui::{RichText, Ui};
use eui_core::msx::SECTIONS;
use eui_core::{MsxFile, MsxModel, SimResults};

use crate::app::EpanetApp;

/// The EPANET-MSX user manual.
pub const MANUAL: &str = "https://github.com/USEPA/EPANETMSX/blob/master/Doc/EPANETMSX.pdf";

/// A page of the Multi-Species Quality Editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// The file, its title and whether it runs.
    #[default]
    General,
    /// A section of the file as a table (index into [`SECTIONS`]).
    Section(usize),
}

#[derive(Default)]
pub struct MsxState {
    /// Run EPANET-MSX with each analysis (if the project has an MSX file).
    pub enabled: bool,
    /// The text being edited in the dialog.
    pub edit: Option<String>,
    /// The MSX file of a project being opened, attached to its network once
    /// it replaces the current one.
    pub pending: Option<MsxFile>,
    pub page: Page,
    /// The editor's working copy of the MSX file (Save and Close makes it
    /// the project's, Cancel drops it).
    work: Option<MsxFile>,
    /// The table being edited: section, rows (the comment last).
    draft: Option<(usize, Vec<Vec<String>>)>,
    /// The row selected in the table, and a copied row.
    row: Option<usize>,
    clip: Option<Vec<String>>,
    inbox: crate::io::Inbox,
}

/// What a new MSX file starts with: a single decaying species.
const TEMPLATE: &str = "\
[TITLE]
Chlorine decay

[OPTIONS]
  AREA_UNITS  M2        ;pipe wall area units
  RATE_UNITS  HR        ;reaction rates per hour
  SOLVER      RK5       ;Runge-Kutta 5th order
  TIMESTEP    300       ;water quality time step (sec)

[SPECIES]
  BULK  CL2  MG         ;free chlorine (mg/L)

[COEFFICIENTS]
  CONSTANT  Kb  0.3     ;bulk decay rate (1/hr)

[PIPES]
  RATE  CL2  -Kb*CL2

[TANKS]
  RATE  CL2  -Kb*CL2

[QUALITY]
  GLOBAL  CL2  0.0      ;initial concentration everywhere

[SOURCES]
";

/// The MSX text to run with an analysis, if any.
pub fn run_input(app: &EpanetApp) -> Option<String> {
    app.net
        .msx
        .as_ref()
        .filter(|_| app.msx.enabled)
        .map(|m| m.text.clone())
}

/// Makes `file` the project's MSX file, as an edit (undo takes it back).
pub fn set_file(app: &mut EpanetApp, file: Option<MsxFile>) {
    app.checkpoint();
    app.msx.enabled = file.is_some();
    app.net.msx = file;
    app.msx.edit = None;
    forget_legends(app);
}

/// Changes the project's MSX file, as an edit; `f` returns an error to
/// leave it as it was.
pub fn edit(
    app: &mut EpanetApp,
    f: impl FnOnce(&mut MsxFile) -> Result<(), String>,
) -> Result<(), String> {
    let Some(mut m) = app.net.msx.clone() else {
        return Err("the project has no MSX file".into());
    };
    f(&mut m)?;
    if app.net.msx.as_ref() != Some(&m) {
        app.checkpoint();
        app.net.msx = Some(m);
    }
    Ok(())
}

/// Species legends belong to one MSX model.
pub fn forget_legends(app: &mut EpanetApp) {
    use crate::style::{LinkTheme, NodeTheme};
    app.prefs
        .node_legends
        .retain(|t, _| !matches!(t, NodeTheme::Msx(_)));
    app.prefs
        .link_legends
        .retain(|t, _| !matches!(t, LinkTheme::Msx(_)));
}

/// The MSX file of the project at `path` (same name, `.msx`), if there is
/// one: EPANET-MSX projects keep the two files side by side.
#[cfg(not(target_arch = "wasm32"))]
pub fn sibling_file(path: &std::path::Path) -> Option<MsxFile> {
    let name = path.file_name()?.to_string_lossy();
    let name = name.trim_end_matches(".gz");
    let base = std::path::Path::new(name).file_stem()?.to_string_lossy();
    let dir = path.parent()?;
    ["msx", "MSX"].iter().find_map(|ext| {
        let p = dir.join(format!("{base}.{ext}"));
        let bytes = std::fs::read(&p).ok()?;
        let name = p.file_name()?.to_string_lossy().into_owned();
        Some(MsxFile::new(&name, &crate::io::decode_text(&bytes)))
    })
}

/// Saves the project's MSX file beside its INP file (`project.msx`), as
/// part of saving the project. Returns the file name written.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_beside(app: &mut EpanetApp, inp: &std::path::Path) -> Result<Option<String>, String> {
    let Some(m) = &mut app.net.msx else {
        return Ok(None);
    };
    let path = inp.with_extension("msx");
    std::fs::write(&path, &m.text).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    m.name = name.clone();
    Ok(Some(name))
}

/// Takes in an MSX file chosen with Open.
pub fn poll(app: &mut EpanetApp) {
    for item in app.msx.inbox.drain() {
        match item {
            Ok((name, bytes)) if app.msx.work.is_some() => {
                // Load in the editor: into its working copy
                app.msx.work = Some(MsxFile::new(&name, &crate::io::decode_text(&bytes)));
                app.msx.draft = None;
            }
            Ok((name, bytes)) => take_file(app, &name, &bytes),
            Err(e) => app.status = format!("Could not open the MSX file: {e}"),
        }
    }
    // EPANET's own quality is not computed while EPANET-MSX runs
    if app.msx_active() {
        use crate::style::{LinkTheme, NodeTheme};
        if app.prefs.node_theme == NodeTheme::Quality {
            app.prefs.node_theme = NodeTheme::None;
        }
        if app.prefs.link_theme == LinkTheme::Quality {
            app.prefs.link_theme = LinkTheme::None;
        }
    } else {
        // and its species are not shown while it does not run
        use crate::style::{LinkTheme, NodeTheme};
        if matches!(app.prefs.node_theme, NodeTheme::Msx(_)) {
            app.prefs.node_theme = NodeTheme::None;
        }
        if matches!(app.prefs.link_theme, LinkTheme::Msx(_)) {
            app.prefs.link_theme = LinkTheme::None;
        }
    }
}

/// Whether EPANET-MSX runs, shown plainly: a colored bar saying so, and a
/// button to turn it on or off.
fn status_bar(app: &mut EpanetApp, ui: &mut Ui) {
    let has_model = app
        .msx
        .work
        .as_ref()
        .or(app.net.msx.as_ref())
        .is_some_and(|w| !w.model().species.is_empty());
    let on = app.msx.enabled && has_model;
    let (fill, text) = if on {
        (
            egui::Color32::from_rgb(0xd4, 0xed, 0xda),
            "EPANET-MSX is ON — it runs with every analysis, instead of EPANET's \
             single species quality.",
        )
    } else if has_model {
        (
            egui::Color32::from_rgb(0xfd, 0xe8, 0xc8),
            "EPANET-MSX is OFF — analyses ignore this model.",
        )
    } else {
        (
            egui::Color32::from_gray(0xee),
            "No multi-species model yet — add species and reactions, or start from \
             a template, then Save and Close.",
        )
    };
    let dark = ui.visuals().dark_mode;
    egui::Frame::NONE
        .fill(if dark {
            fill.gamma_multiply(0.35)
        } else {
            fill
        })
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(170)))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            // a fixed width: a long text must not widen the editor
            ui.set_width(790.0);
            ui.horizontal(|ui| {
                let dot = if on {
                    egui::Color32::from_rgb(0x27, 0xae, 0x60)
                } else {
                    egui::Color32::from_gray(140)
                };
                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 6.0, dot);
                let color = if dark {
                    egui::Color32::LIGHT_GRAY
                } else {
                    egui::Color32::from_gray(30)
                };
                // in a fixed space, cut short rather than widen the editor
                ui.allocate_ui(egui::vec2(660.0, 20.0), |ui| {
                    ui.add(egui::Label::new(RichText::new(text).color(color)).truncate())
                        .on_hover_text(text);
                });
                if has_model {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if on { "Turn Off" } else { "Turn On" };
                        if ui.button(label).clicked() {
                            app.msx.enabled = !on;
                            app.mark_results_stale();
                        }
                    });
                }
            });
        });
}

/// Makes an MSX file that was read (opened, or dropped on the window) the
/// project's.
pub fn take_file(app: &mut EpanetApp, name: &str, bytes: &[u8]) {
    let file = MsxFile::new(name, &crate::io::decode_text(bytes));
    let n = file.model().species.len();
    app.status = if n == 0 {
        format!("{name} declares no species — check its [SPECIES] section")
    } else {
        format!("Loaded EPANET-MSX file {name} ({n} species) — run an analysis")
    };
    set_file(app, Some(file));
}

/// Opens the arsenic oxidation/adsorption example of the EPANET-MSX manual:
/// its network, with its MSX file.
#[cfg(test)]
pub fn open_example(app: &mut EpanetApp) {
    use eui_core::msx::examples;
    app.msx.pending = Some(MsxFile::new("example.msx", examples::MSX));
    app.load_document("MSX Example.inp", examples::INP, None);
}

// ----- results ------------------------------------------------------------

/// The species the results hold for nodes (bulk species only, as wall
/// species live on pipe walls) or for links, as indices into the species.
pub fn result_species(res: Option<&SimResults>, nodes: bool) -> Vec<usize> {
    let Some(m) = res.and_then(|r| r.msx.as_ref()) else {
        return Vec::new();
    };
    (0..m.species().len())
        .filter(|&s| !nodes || !m.species()[s].wall)
        .collect()
}

/// The species ID and concentration units of species `s`.
pub fn species_label(res: Option<&SimResults>, s: usize) -> Option<(String, String)> {
    let m = res?.msx.as_ref()?;
    Some((m.species().get(s)?.id.clone(), m.units(s)))
}

/// A species theme's name: the species ID.
pub fn theme_name(app: &EpanetApp, s: usize) -> Option<String> {
    species_label(app.results.as_ref(), s).map(|l| l.0)
}

/// The value a species theme shows for result row `r` at the map's time.
pub fn theme_value(app: &EpanetApp, nodes: bool, s: usize, r: usize) -> Option<f32> {
    let m = app.results.as_ref()?.msx.as_ref()?;
    if nodes {
        m.node(app.period, s, r)
    } else {
        m.link(app.period, s, r)
    }
}

/// Legend intervals for species `s`: four equal steps from 0 to the largest
/// value of any period (rounded), so that colors keep their meaning while
/// the map is animated.
pub fn default_breaks(res: Option<&SimResults>, s: usize, nodes: bool) -> Option<[f32; 4]> {
    let m = res?.msx.as_ref()?;
    let (lo, hi) = if nodes {
        m.node_range(s)?
    } else {
        m.link_range(s)?
    };
    let lo = lo.min(0.0);
    let span = hi - lo;
    if span.is_nan() || span <= 0.0 {
        return None;
    }
    let step = nice(span / 5.0);
    Some([1.0, 2.0, 3.0, 4.0].map(|k| lo + k * step))
}

/// Rounds a positive step to 1, 2 or 5 times a power of ten.
fn nice(x: f32) -> f32 {
    let p = 10f32.powf(x.log10().floor());
    let m = x / p;
    p * if m < 1.5 {
        1.0
    } else if m < 3.5 {
        2.0
    } else if m < 7.5 {
        5.0
    } else {
        10.0
    }
}

// ----- dialog -------------------------------------------------------------

/// Opens the MSX Editor on a working copy of the project's MSX file (an
/// empty one if it has none).
pub fn open_editor(app: &mut EpanetApp) {
    app.msx.work = Some(
        app.net
            .msx
            .clone()
            .unwrap_or_else(|| MsxFile::new(&format!("{}.msx", app.file_name), "[TITLE]\n\n")),
    );
    app.msx.draft = None;
    app.msx.row = None;
    app.msx.edit = None;
    app.dialog = crate::app::Dialog::Msx;
}

/// The page names of the editor's Data Category list.
fn page_title(page: Page) -> &'static str {
    match page {
        Page::General => "Introduction",
        Page::Section(k) => match SECTIONS[k].name {
            "TERMS" => "Reaction Terms",
            _ => SECTIONS[k].title,
        },
    }
}

/// The Data Category list, in EPANET-UI's order.
fn pages() -> Vec<Page> {
    let at = |name: &str| Page::Section(SECTIONS.iter().position(|s| s.name == name).unwrap_or(0));
    let mut v = vec![Page::General];
    for name in [
        "OPTIONS",
        "SPECIES",
        "PIPES",
        "TANKS",
        "TERMS",
        "COEFFICIENTS",
        "PARAMETERS",
        "QUALITY",
        "SOURCES",
        "PATTERNS",
        "DIFFUSIVITY",
        "REPORT",
    ] {
        v.push(at(name));
    }
    v
}

/// EPANET-UI's MSX Editor: the data categories on the left, the selected
/// one's table (with a toolbar and a hint) on the right, and Help, Clear,
/// Load, Save As, Save and Close and Cancel below. Edits go to a working
/// copy until Save and Close.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    poll(app);
    if app.msx.work.is_none() {
        open_editor(app);
    }
    let ctx = ui.ctx().clone();
    ui.set_width(810.0);
    status_bar(app, ui);
    ui.add_space(6.0);
    ui.allocate_ui_with_layout(
        egui::vec2(810.0, 370.0),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            ui.set_height(370.0);
            // the Data Category list
            ui.vertical(|ui| {
                ui.set_width(150.0);
                ui.label(RichText::new("Data Category").strong());
                ui.add_space(4.0);
                for page in pages() {
                    if ui
                        .add_sized(
                            [150.0, 24.0],
                            egui::Button::selectable(app.msx.page == page, page_title(page)),
                        )
                        .clicked()
                    {
                        app.msx.page = page;
                        app.msx.row = None;
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(640.0);
                ui.set_height(370.0);
                match app.msx.page {
                    Page::General => general_page(app, ui),
                    Page::Section(k) => section_page(app, ui, k),
                }
            });
        },
    );
    ui.add_space(8.0);
    let mut close = false;
    ui.horizontal(|ui| {
        let b = |ui: &mut Ui, t: &str| ui.add_sized([96.0, 26.0], egui::Button::new(t)).clicked();
        if b(ui, "Help") {
            crate::help::open(&ctx, "Multi-Species Quality");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if b(ui, "Cancel") {
                app.msx.work = None;
                app.msx.edit = None;
                close = true;
            }
            if b(ui, "Save and Close") {
                save_and_close(app);
                close = true;
            }
            if b(ui, "Save As")
                && let Some(w) = &app.msx.work
                && let Err(e) = crate::io::save_report(&w.text, &w.name, &ctx)
            {
                app.status = format!("Could not save the MSX file: {e}");
            }
            if b(ui, "Load") {
                crate::io::open_file(&app.msx.inbox, &ctx, "EPANET-MSX file", &["msx", "MSX"]);
            }
            if b(ui, "Clear") {
                if let Some(w) = &mut app.msx.work {
                    w.text = "[TITLE]\n\n".into();
                }
                app.msx.draft = None;
                app.msx.row = None;
            }
        });
    });
    close
}

/// Makes the working copy the project's MSX file (as an edit, for undo),
/// and on the desktop writes it beside the project's file.
fn save_and_close(app: &mut EpanetApp) {
    let Some(w) = app.msx.work.take() else {
        return;
    };
    app.msx.edit = None;
    let empty = w.model().species.is_empty() && w.text.trim() == "[TITLE]";
    let new = (!empty).then_some(w);
    if new != app.net.msx {
        let keep = app.msx.enabled || app.net.msx.is_none();
        set_file(app, new);
        app.msx.enabled = keep && app.net.msx.is_some();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = app.path.clone()
            && let Ok(Some(name)) = save_beside(app, &path)
        {
            app.status = format!("Saved {name}");
        }
    }
}

/// The yellow hint at the bottom of a page.
fn hint(ui: &mut Ui, text: &str) {
    let bg = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(0x3a, 0x38, 0x2a)
    } else {
        crate::style::HINT_BG
    };
    egui::Frame::NONE
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(190)))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(36.0);
            ui.label(text);
        });
}

/// The Introduction page: what EPANET-MSX does, the file, its title and
/// whether it runs, and ways to start a model.
fn general_page(app: &mut EpanetApp, ui: &mut Ui) {
    let Some(w) = app.msx.work.clone() else {
        return;
    };
    let model = w.model();
    ui.label(
        RichText::new("Multi-species water quality (EPANET-MSX 2.0)")
            .font(crate::fonts::bold(14.0)),
    );
    ui.add_space(4.0);
    ui.label(
        "EPANET-MSX models several interacting chemical and biological species, \
         in the water and on the pipe walls. Each page on the left holds one \
         part of the model: at least its Species and Pipe Reactions are needed. \
         With a model saved, every analysis also runs EPANET-MSX on its \
         hydraulics, and a node's Property Editor sets its initial \
         concentrations and sources of the species.",
    );
    ui.add_space(10.0);
    let mut title = None;
    egui::Grid::new("msx_intro")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("File");
            ui.label(&w.name);
            ui.end_row();
            ui.label("Title");
            let key = egui::Id::new("msx_title");
            let mut buf: String = ui
                .data_mut(|d| d.get_temp(key))
                .unwrap_or_else(|| model.title.clone());
            let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(420.0));
            if resp.lost_focus() {
                if buf.trim() != model.title {
                    title = Some(buf.clone());
                }
                ui.data_mut(|d| d.remove::<String>(key));
            } else if resp.has_focus() {
                ui.data_mut(|d| d.insert_temp(key, buf));
            }
            ui.end_row();
            ui.label("Species");
            ui.label(if model.species.is_empty() {
                "none yet".to_string()
            } else {
                model
                    .species
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            ui.end_row();
        });
    if let Some(t) = title
        && let Some(w) = &mut app.msx.work
    {
        w.set_title(&t);
    }
    ui.add_space(8.0);
    let mut on = app.msx.enabled;
    if crate::widgets::checkbox(ui, &mut on, "Run EPANET-MSX with each analysis").changed() {
        app.msx.enabled = on;
        app.mark_results_stale();
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui
            .button("Start from a Template")
            .on_hover_text("A single species, chlorine, decaying in the water")
            .clicked()
            && let Some(w) = &mut app.msx.work
        {
            w.text = TEMPLATE.to_string();
            app.msx.draft = None;
        }
        if ui.button("Edit as Text…").clicked() {
            app.msx.edit = Some(w.text.clone());
        }
    });
    if app.msx.edit.is_some() {
        text_editor(app, ui);
    }
    ui.add_space(8.0);
    if ui.link("EPANET-MSX User Manual ↗").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(MANUAL));
    }
}

/// What each page is for (its hint).
fn section_note(name: &str) -> &'static str {
    match name {
        "OPTIONS" => {
            "AREA_UNITS FT2, M2 or CM2 · RATE_UNITS SEC, MIN, HR or DAY · SOLVER EUL, \
             RK5 or ROS2 · COUPLING FULL or NONE · TIMESTEP in seconds · ATOL, RTOL."
        }
        "SPECIES" => {
            "Enter each specie: BULK in the water or WALL on pipe walls, its name, \
             mass units (MG, UG, MOLE, MMOL) and optional tolerances."
        }
        "COEFFICIENTS" => {
            "Enter each reaction coefficient: a CONSTANT, or a PARAMETER that can take \
             other values for single pipes and tanks."
        }
        "TERMS" => "Enter intermediate expressions, used by name in the reactions.",
        "PIPES" => "Enter a pipe reaction expression for each specie.",
        "TANKS" => "Enter a tank reaction expression for each bulk specie.",
        "DIFFUSIVITY" => "Enter the molecular diffusivity of bulk species, for dispersion.",
        "QUALITY" => {
            "Enter initial concentrations: GLOBAL for every node and pipe, NODE or \
             LINK for one (also in their Property Editor)."
        }
        "SOURCES" => {
            "Enter sources at nodes: CONCEN, MASS, SETPOINT or FLOWPACED, a strength \
             and an optional pattern (also in a node's Property Editor)."
        }
        "PARAMETERS" => "Enter values of PARAMETER coefficients for single pipes or tanks.",
        "PATTERNS" => "Enter multipliers for each pattern time step (a pattern may span rows).",
        "REPORT" => {
            "Enter what is written to the Status Report: NODES and LINKS (ALL or IDs), \
             SPECIES name YES or NO."
        }
        _ => "",
    }
}

/// A toolbar button: a symbol with a tooltip.
fn tool(ui: &mut Ui, enabled: bool, symbol: &str, tip: &str) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(symbol).size(15.0)).min_size(egui::vec2(32.0, 26.0)),
    )
    .on_hover_text(tip)
    .clicked()
}

/// A section of the MSX file as an editable table, the comment last.
fn section_page(app: &mut EpanetApp, ui: &mut Ui, k: usize) {
    let s = &SECTIONS[k];
    let Some(work) = &app.msx.work else {
        return;
    };
    if app.msx.draft.as_ref().is_none_or(|d| d.0 != k) {
        app.msx.draft = Some((k, work.rows(s)));
        app.msx.row = None;
    }
    let n = s.columns.len();
    let mut changed = false;
    let (_, rows) = app.msx.draft.as_mut().expect("the draft");
    let sel = app.msx.row.filter(|r| *r < rows.len());
    // EPANET-UI's toolbar: cut, copy, paste, insert, delete, move
    ui.horizontal(|ui| {
        if tool(ui, sel.is_some(), "✂", "Cut the row")
            && let Some(r) = sel
        {
            app.msx.clip = Some(rows.remove(r));
            changed = true;
        }
        if tool(ui, sel.is_some(), "⎘", "Copy the row")
            && let Some(r) = sel
        {
            app.msx.clip = Some(rows[r].clone());
        }
        if tool(ui, app.msx.clip.is_some(), "📋", "Paste the row")
            && let Some(c) = app.msx.clip.clone()
        {
            let at = sel.map_or(rows.len(), |r| r + 1);
            rows.insert(at, c);
            app.msx.row = Some(at);
            changed = true;
        }
        ui.separator();
        if tool(ui, true, "➕", "Insert a row") {
            let at = sel.map_or(rows.len(), |r| r + 1);
            rows.insert(at, vec![String::new(); n + 1]);
            app.msx.row = Some(at);
        }
        if tool(ui, sel.is_some(), "✖", "Delete the row")
            && let Some(r) = sel
        {
            rows.remove(r);
            app.msx.row = None;
            changed = true;
        }
        if tool(ui, sel.is_some_and(|r| r > 0), "⬆", "Move the row up")
            && let Some(r) = sel
        {
            rows.swap(r, r - 1);
            app.msx.row = Some(r - 1);
            changed = true;
        }
        if tool(
            ui,
            sel.is_some_and(|r| r + 1 < rows.len()),
            "⬇",
            "Move the row down",
        ) && let Some(r) = sel
        {
            rows.swap(r, r + 1);
            app.msx.row = Some(r + 1);
            changed = true;
        }
    });
    let widths: Vec<f32> = (0..=n)
        .map(|c| {
            if c == n {
                150.0 // comment
            } else if s.rest && c + 1 == n {
                200.0
            } else if c == 0 && !s.keywords.is_empty() {
                100.0
            } else {
                90.0
            }
        })
        .collect();
    egui::Frame::NONE
        .fill(if ui.visuals().dark_mode {
            ui.visuals().extreme_bg_color
        } else {
            egui::Color32::WHITE
        })
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(160)))
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt(("msx_section", k))
                .max_height(250.0)
                .min_scrolled_height(250.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new(("msx_grid", k))
                        .num_columns(n + 1)
                        .striped(true)
                        .spacing([4.0, 3.0])
                        .show(ui, |ui| {
                            for (c, h) in s.columns.iter().chain(["Comment"].iter()).enumerate() {
                                ui.add_sized(
                                    [widths[c], 18.0],
                                    egui::Label::new(RichText::new(*h).strong()),
                                );
                            }
                            ui.end_row();
                            for (r, row) in rows.iter_mut().enumerate() {
                                row.resize(n + 1, String::new());
                                for c in 0..=n {
                                    let cell = &mut row[c];
                                    if c == 0 && !s.keywords.is_empty() {
                                        let shown = if cell.is_empty() {
                                            "…".to_string()
                                        } else {
                                            cell.to_ascii_uppercase()
                                        };
                                        let resp = egui::ComboBox::from_id_salt(("msx_kw", k, r))
                                            .selected_text(shown)
                                            .width(widths[c])
                                            .show_ui(ui, |ui| {
                                                for kw in s.keywords {
                                                    if ui
                                                        .selectable_label(
                                                            cell.eq_ignore_ascii_case(kw),
                                                            *kw,
                                                        )
                                                        .clicked()
                                                    {
                                                        *cell = kw.to_string();
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        if resp.response.clicked() {
                                            app.msx.row = Some(r);
                                        }
                                        continue;
                                    }
                                    let mut edit =
                                        egui::TextEdit::singleline(cell).desired_width(widths[c]);
                                    if c < n {
                                        edit = edit.font(egui::TextStyle::Monospace);
                                    }
                                    let resp = ui.add(edit);
                                    if resp.gained_focus() {
                                        app.msx.row = Some(r);
                                    }
                                    if resp.changed() {
                                        changed = true;
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    if changed {
        let rows = rows.clone();
        if let Some(w) = &mut app.msx.work {
            w.set_rows(s, &rows);
        }
    }
    ui.add_space(6.0);
    hint(ui, section_note(s.name));
}

/// The MSX file's text, edited on the Introduction page; Apply puts it in
/// the working copy.
fn text_editor(app: &mut EpanetApp, ui: &mut Ui) {
    let mut apply = false;
    let mut cancel = false;
    if let Some(text) = &mut app.msx.edit {
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("msx_text")
            .max_height(150.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(text)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
            });
        ui.horizontal(|ui| {
            let species = MsxModel::parse(text).species.len();
            ui.label(RichText::new(format!("{species} species")).weak());
            apply = ui.button("Apply").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    }
    if apply && let Some(text) = app.msx.edit.take() {
        if let Some(w) = &mut app.msx.work {
            w.text = text;
        }
        app.msx.draft = None;
    } else if cancel {
        app.msx.edit = None;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::style::{LinkTheme, NodeTheme};

    pub(crate) fn new_app() -> (egui::Context, EpanetApp) {
        let ctx = egui::Context::default();
        let app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        (ctx, app)
    }

    pub(crate) fn finish_run(app: &mut EpanetApp) {
        let start = std::time::Instant::now();
        while app.run.is_some() {
            assert!(
                start.elapsed().as_secs() < 60,
                "the analysis did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.poll_simulation();
        }
        app.refresh_maps();
    }

    /// The EPANET-MSX example, analyzed.
    pub(crate) fn example_app_with_results() -> (egui::Context, EpanetApp) {
        let (ctx, mut app) = new_app();
        open_example(&mut app);
        app.run_simulation(&ctx);
        finish_run(&mut app);
        (ctx, app)
    }

    /// The dialogs as drawn: their pieces of text.
    fn texts(app: &mut EpanetApp, ctx: &egui::Context) -> Vec<String> {
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
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect();
        }
        texts
    }

    #[test]
    fn nice_steps() {
        assert_eq!(nice(0.27), 0.2);
        assert_eq!(nice(4.0), 5.0);
        assert_eq!(nice(8.0), 10.0);
        assert_eq!(nice(0.011), 0.01);
    }

    #[test]
    fn the_template_declares_a_species() {
        let m = MsxModel::parse(TEMPLATE);
        assert_eq!(m.species.len(), 1);
        assert_eq!(m.units(0), "MG/L");
    }

    #[test]
    fn the_example_runs_and_its_species_are_map_themes() {
        let (_ctx, mut app) = example_app_with_results();
        assert_eq!(app.net.nodes.len(), 5);
        assert!(app.msx.enabled);
        assert_eq!(app.net.msx.as_ref().unwrap().name, "example.msx");
        let m = app.results.as_ref().unwrap().msx.as_ref().unwrap();
        assert_eq!(m.species().len(), 5);
        // bulk species at nodes, all species in links
        let nodes = app.node_themes();
        assert!(nodes.contains(&NodeTheme::Msx(2)) && !nodes.contains(&NodeTheme::Msx(3)));
        assert!(app.link_themes().contains(&LinkTheme::Msx(3)));
        app.prefs.node_theme = NodeTheme::Msx(2);
        assert_eq!(app.prefs.node_theme, NodeTheme::Msx(2));
        assert_eq!(app.node_theme_name(app.prefs.node_theme), "AStot");
        assert_eq!(app.node_theme_units(app.prefs.node_theme), "UG/L");
        let n = app.results.as_ref().unwrap().n_periods;
        app.period = n - 1;
        let c = app.net.node_index("C").unwrap();
        let v = app.node_theme_value(c).unwrap();
        assert!(v > 1.0 && v < 10.5, "AStot at C: {v}");
        // the legend covers the values of every period
        let b = app.node_legend().breaks;
        assert!(
            b.windows(2).all(|w| w[0] < w[1]) && b[3] <= 10.0 && b[3] >= 6.0,
            "{b:?}"
        );
        app.prefs.link_theme = LinkTheme::Msx(3);
        assert_eq!(app.link_theme_units(app.prefs.link_theme), "UG/M2");
        assert!(
            app.link_theme_value(app.net.link_index("5").unwrap())
                .unwrap()
                > 0.0
        );
        // turning EPANET-MSX off makes the results out of date
        assert!(!app.results_stale());
        app.msx.enabled = false;
        app.mark_results_stale();
        assert!(app.results_stale());
        // ... and its species are no longer themes: None, not "MSX Species"
        poll(&mut app);
        assert_eq!(app.prefs.node_theme, NodeTheme::None);
        assert_eq!(app.prefs.link_theme, LinkTheme::None);
        assert!(
            !app.node_themes()
                .iter()
                .any(|t| matches!(t, NodeTheme::Msx(_)))
        );
        app.msx.enabled = true;
        assert!(app.node_themes().contains(&NodeTheme::Msx(2)));
        app.msx.enabled = false;
        // another network has no MSX file: the species themes go
        app.load_example("Net1", eui_core::examples::NET1);
        assert!(app.net.msx.is_none());
        assert!(!matches!(app.prefs.node_theme, NodeTheme::Msx(_)));
        assert!(!matches!(app.prefs.link_theme, LinkTheme::Msx(_)));
    }

    #[test]
    fn plugins_read_the_species() {
        let (_ctx, mut app) = example_app_with_results();
        let src = r#"
            let s = msx_species();
            message(`${s.len()} ${s[4]}`);
            message(`${node_species("Source", "NH2CL", 0)}`);
            message(`${link_species("5", "AS5s", periods() - 1) > 0.0}`);
        "#;
        let out = crate::scripting::run_source(&mut app, "t", src, None).unwrap();
        assert!(
            out.contains("5 NH2CL") && out.contains("2.5") && out.contains("true"),
            "{out}"
        );
        let wall = r#"node_species("C", "AS5s", 0)"#;
        let e = crate::scripting::run_source(&mut app, "t", wall, None).unwrap_err();
        assert!(e.contains("wall species"), "{e}");
    }

    #[test]
    fn the_editor_shows_the_model_like_epanet_ui() {
        let (ctx, mut app) = new_app();
        open_editor(&mut app);
        let t = texts(&mut app, &ctx);
        // EPANET-UI's Data Category list and buttons
        for s in [
            "Data Category",
            "Introduction",
            "Options",
            "Species",
            "Pipe Reactions",
            "Tank Reactions",
            "Reaction Terms",
            "Coefficients",
            "Parameters",
            "Initial Quality",
            "Sources",
            "Help",
            "Clear",
            "Load",
            "Save As",
            "Save and Close",
            "Cancel",
            "Start from a Template",
        ] {
            assert!(t.iter().any(|x| x == s), "{s} missing from {t:?}");
        }
        open_example(&mut app);
        open_editor(&mut app);
        let t = texts(&mut app, &ctx);
        // whether it runs is shown plainly
        assert!(t.iter().any(|x| x.starts_with("EPANET-MSX is ON")), "{t:?}");
        assert!(t.iter().any(|x| x == "Turn Off"));
        app.msx.enabled = false;
        let t = texts(&mut app, &ctx);
        assert!(
            t.iter().any(|x| x.starts_with("EPANET-MSX is OFF")),
            "{t:?}"
        );
        assert!(t.iter().any(|x| x == "Turn On"));
        app.msx.enabled = true;
        for s in ["example.msx", "Run EPANET-MSX with each analysis"] {
            assert!(t.iter().any(|x| x == s), "{s} missing from {t:?}");
        }
        let page =
            |name: &str| Page::Section(SECTIONS.iter().position(|s| s.name == name).unwrap());
        app.msx.page = page("PIPES");
        let t = texts(&mut app, &ctx);
        for s in [
            "Type",
            "Species",
            "Expression",
            "Comment",
            "-Ka*AS3*NH2CL",
            "Arsenite oxidation",
            "EQUIL",
            "Enter a pipe reaction expression for each specie.",
        ] {
            assert!(t.iter().any(|x| x == s), "{s} missing from {t:?}");
        }
        app.msx.page = page("QUALITY");
        let t = texts(&mut app, &ctx);
        for s in ["NODE", "Source", "NH2CL", "2.5"] {
            assert!(t.iter().any(|x| x == s), "{s} missing from {t:?}");
        }
        // results are not part of the editor: they are on the map and in the reports
        assert!(!t.iter().any(|x| x == "Results"), "{t:?}");
    }

    /// The size the open dialog is drawn at.
    pub(crate) fn dialog_size(app: &mut EpanetApp, ctx: &egui::Context) -> egui::Vec2 {
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 2400.0),
            )),
            ..Default::default()
        };
        let mut size = egui::Vec2::ZERO;
        for _ in 0..3 {
            let mut out = ctx.run_ui(input(), |ui| crate::dialogs::show(app, ui.ctx()));
            out.textures_delta.clear();
            let r = out
                .shapes
                .iter()
                .map(|c| c.shape.visual_bounding_rect().intersect(c.clip_rect))
                .filter(|r| r.is_finite())
                .fold(egui::Rect::NOTHING, |a, b| a.union(b));
            size = r.size();
        }
        size
    }

    /// The editor keeps EPANET-UI's size, whatever the page.
    #[test]
    fn the_editor_has_a_fixed_size() {
        let (ctx, mut app) = new_app();
        open_example(&mut app);
        open_editor(&mut app);
        let mut sizes = Vec::new();
        for page in pages() {
            app.msx.page = page;
            sizes.push(dialog_size(&mut app, &ctx));
        }
        for s in &sizes {
            assert!(s.x < 920.0 && s.y < 580.0, "{sizes:?}");
            assert!((s.y - sizes[0].y).abs() < 2.0, "{sizes:?}");
        }
    }

    /// Edits go to a working copy: Cancel drops them, Save and Close makes
    /// them the project's (undo takes them back).
    #[test]
    fn the_editor_saves_or_cancels_its_working_copy() {
        let (_ctx, mut app) = new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        open_editor(&mut app);
        app.msx.work.as_mut().unwrap().text = TEMPLATE.into();
        app.msx.work = None; // Cancel
        assert!(app.net.msx.is_none());
        open_editor(&mut app);
        app.msx.work.as_mut().unwrap().text = TEMPLATE.into();
        save_and_close(&mut app);
        assert_eq!(app.net.msx.as_ref().unwrap().model().species[0].id, "CL2");
        assert!(app.msx.enabled);
        app.undo();
        assert!(app.net.msx.is_none());
    }

    /// A source set on a node (as in the Property Editor) runs, and undo
    /// takes it back.
    #[test]
    fn a_source_at_a_node_feeds_the_network() {
        let (ctx, mut app) = new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        set_file(&mut app, Some(MsxFile::new("Net1.msx", TEMPLATE)));
        edit(&mut app, |m| m.set_source("9", "CL2", "SETPOINT 1.2")).unwrap();
        assert!(edit(&mut app, |m| m.set_source("9", "CL2", "BOOST 1")).is_err());
        app.run_simulation(&ctx);
        finish_run(&mut app);
        let res = app.results.as_ref().expect("results");
        let m = res.msx.as_ref().expect("MSX results");
        assert_eq!(m.species()[0].id, "CL2");
        assert_eq!(m.n_periods, res.n_periods);
        assert!(!app.report.contains("EPANET-MSX:"), "{}", app.report);
        // EPANET's own chlorine (Net1 has one) is not run beside EPANET-MSX
        assert_eq!(res.quality_flag, 0);
        let src = res.node_ids.iter().position(|id| id == "9").unwrap();
        let far = res.node_ids.iter().position(|id| id == "32").unwrap();
        let last = m.n_periods - 1;
        assert!((m.node(last, 0, src).unwrap() - 1.2).abs() < 1e-3);
        let c = m.node(last, 0, far).unwrap();
        assert!(c > 0.0 && c < 1.2, "chlorine at node 32: {c}");
        // renaming the node follows it into the MSX file
        let i = app.net.node_index("9").unwrap();
        app.checkpoint();
        assert!(app.net.rename_node(i, "R9"));
        let msx = app.net.msx.as_ref().unwrap();
        assert_eq!(msx.source("R9", "CL2").as_deref(), Some("SETPOINT 1.2"));
        // undo restores the source's node, and further back no source at all
        app.undo();
        app.undo();
        assert_eq!(app.net.msx.as_ref().unwrap().source("9", "CL2"), None);
        // removed: the next analysis has no species
        set_file(&mut app, None);
        assert!(run_input(&app).is_none());
    }
}
