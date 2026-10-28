//! Project Setup, as EPANET-UI's `projectsetup`: hydraulic options, ID
//! prefixes and properties for new objects, and the map's dimensions, units
//! and EPSG code. Changing the flow or pressure units converts the project's
//! data with EPANET itself (as EPANET-UI does), and changing the head loss
//! formula can convert pipe roughness.

use egui::{Color32, RichText, Ui};
use eui_core::{LinkKind, Network, NodeKind};

use crate::app::EpanetApp;

const FLOW_UNITS: [&str; 11] = [
    "cfs", "gpm", "mgd", "imgd", "afd", "lps", "lpm", "mld", "cmh", "cmd", "cms",
];
const PRESSURE_UNITS: [&str; 5] = ["psi", "kpa", "meters", "bar", "feet"];
const HEADLOSS: [&str; 3] = ["H-W", "D-W", "C-M"];
const MAP_UNITS: [&str; 4] = ["None", "Feet", "Meters", "Degrees"];

const OPTION_NAMES: [&str; 9] = [
    "Flow Units",
    "Pressure Units",
    "Head Loss Formula",
    "Specific Gravity",
    "Specific Viscosity",
    "Maximum Trials",
    "Accuracy",
    "Head Tolerance",
    "Flow Tolerance",
];
/// [OPTIONS] keywords of the options above.
const OPTION_KEYS: [&str; 9] = [
    "UNITS",
    "PRESSURE",
    "HEADLOSS",
    "SPECIFIC GRAVITY",
    "VISCOSITY",
    "TRIALS",
    "ACCURACY",
    "HEADERROR",
    "FLOWCHANGE",
];
const PREFIX_NAMES: [&str; 8] = [
    "Junctions",
    "Reservoirs",
    "Tanks",
    "Pipes",
    "Pumps",
    "Valves",
    "Patterns",
    "Curves",
];
const PROPERTY_NAMES: [&str; 6] = [
    "Node Elevation",
    "Tank Height",
    "Tank Diameter",
    "Pipe Length",
    "Pipe Diameter",
    "Pipe Roughness",
];
const MAP_NAMES: [&str; 6] = [
    "Lower Left X",
    "Lower Left Y",
    "Upper Right X",
    "Upper Right Y",
    "Map Units",
    "CRS",
];
const HINTS: [&str; 4] = [
    "Project hydraulic options",
    "ID prefixes for new objects",
    "Properties for new objects",
    "Map dimensions and units",
];

/// ID prefixes and properties for new objects (EPANET-UI's `IDprefix` and
/// `DefProps`), plus the options new projects start with.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SetupDefaults {
    pub prefixes: [String; 8],
    pub properties: [String; 6],
    /// Hydraulic options for new projects ("Save as defaults").
    pub options: Option<[String; 9]>,
}

impl Default for SetupDefaults {
    fn default() -> Self {
        SetupDefaults {
            prefixes: Default::default(),
            properties: ["0", "20", "50", "1000", "12", "130"].map(String::from),
            options: None,
        }
    }
}

impl SetupDefaults {
    fn prefix(&self, k: usize) -> &str {
        self.prefixes[k].trim()
    }
    fn property(&self, k: usize) -> String {
        self.properties[k].trim().to_string()
    }

    /// Adds a node with the project's ID prefix and default properties.
    pub fn add_node(&self, net: &mut Network, kind: NodeKind, pos: [f64; 2]) -> usize {
        let k = match kind {
            NodeKind::Junction => 0,
            NodeKind::Reservoir => 1,
            NodeKind::Tank => 2,
        };
        let i = net.add_node_as(kind, pos, self.prefix(k));
        let f = &mut net.nodes[i].fields;
        // elevation, or a reservoir's head
        f[0] = self.property(0);
        if kind == NodeKind::Tank {
            f[3] = self.property(1);
            f[4] = self.property(2);
        }
        i
    }

    /// Adds a link with the project's ID prefix and default properties; a
    /// pipe gets the default length unless Auto-Length measures it.
    pub fn add_link(
        &self,
        net: &mut Network,
        kind: LinkKind,
        from: usize,
        to: usize,
        auto_length: bool,
    ) -> usize {
        let k = match kind {
            LinkKind::Pipe => 3,
            LinkKind::Pump => 4,
            LinkKind::Valve => 5,
        };
        let i = net.add_link_as(kind, from, to, self.prefix(k));
        if kind == LinkKind::Pipe {
            let f = &mut net.links[i].fields;
            if !auto_length {
                f[0] = self.property(3);
            }
            f[1] = self.property(4);
            f[2] = self.property(5);
        }
        i
    }

    /// Splits pipe `link` on its segment `seg` near `pos` with a new node of
    /// `kind`, see [`Network::split_link`]. Returns the node's index.
    pub fn split_link(
        &self,
        net: &mut Network,
        link: usize,
        seg: usize,
        kind: NodeKind,
        pos: [f64; 2],
    ) -> Option<usize> {
        if net.links[link].kind != LinkKind::Pipe {
            return None;
        }
        let node = self.add_node(net, kind, pos);
        net.split_link(link, seg, node, self.prefix(3))?;
        Some(node)
    }

    /// Sets the saved default options on a new project.
    pub fn apply_new_project_options(&self, net: &mut Network) {
        if let Some(opts) = &self.options {
            for (k, v) in opts.iter().enumerate() {
                net.set_option("OPTIONS", OPTION_KEYS[k], &v.to_ascii_uppercase());
            }
        }
    }
}

/// How to change pipe roughness when the head loss formula changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Roughness {
    Formula,
    Default,
    Keep,
}

/// The dialog's working copy.
#[derive(Clone, Debug)]
pub struct SetupEdit {
    page: usize,
    options: [String; 9],
    prefixes: [String; 8],
    properties: [String; 6],
    map: [String; 6],
    save_defaults: bool,
    error: Option<String>,
    /// Asking to confirm unit or formula changes.
    confirm: Option<String>,
    roughness: Roughness,
    old: [String; 3],
    /// The Coordinate Reference System Selector, while open for the CRS row.
    crs_selector: Option<crate::basemap::CrsSelector>,
}

/// A unit conversion in progress, finished by [`poll`].
pub struct SetupJob {
    handle: eui_engine::ConvertHandle,
    before: Network,
    roughness: Roughness,
    old_model: String,
    new_model: String,
    default_roughness: f64,
}

/// The value of an option as the dialog shows it.
fn option_value(net: &Network, k: usize) -> String {
    let v = net.option("OPTIONS", OPTION_KEYS[k]);
    match k {
        0 => v.unwrap_or_else(|| "GPM".into()).to_ascii_lowercase(),
        1 => v
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| if net.is_si() { "meters" } else { "psi" }.into()),
        2 => v.unwrap_or_else(|| "H-W".into()).to_ascii_uppercase(),
        _ => v.unwrap_or_else(|| ["1.0", "1.0", "200", "0.001", "0", "0"][k - 3].into()),
    }
}

/// Opens Project Setup.
pub fn open(app: &mut EpanetApp) {
    let net = &app.net;
    let options = std::array::from_fn(|k| crate::style::show_num(&option_value(net, k)));
    let dims: Option<[f64; 4]> = net.option("BACKDROP", "DIMENSIONS").and_then(|s| {
        let v: Vec<f64> = s
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
    });
    let ext = dims
        .or(net.bounds())
        .unwrap_or([0.0, 0.0, 10000.0, 10000.0]);
    let units = net
        .option("BACKDROP", "UNITS")
        .map(|u| {
            MAP_UNITS
                .iter()
                .find(|m| m.eq_ignore_ascii_case(&u))
                .unwrap_or(&"None")
                .to_string()
        })
        .unwrap_or_else(|| "None".into());
    // the EPSG code, or the proj string of a custom system
    let crs = crate::basemap::crs_def(app).unwrap_or_default();
    let epsg = crs.strip_prefix("EPSG:").unwrap_or(&crs).to_string();
    let old = [options[0].clone(), options[1].clone(), options[2].clone()];
    app.setup_edit = Some(SetupEdit {
        page: 0,
        options,
        prefixes: app.setup.prefixes.clone(),
        properties: app
            .setup
            .properties
            .clone()
            .map(|p| crate::style::show_num(&p)),
        map: [
            crate::style::show_num(&format!("{:.6}", ext[0])),
            crate::style::show_num(&format!("{:.6}", ext[1])),
            crate::style::show_num(&format!("{:.6}", ext[2])),
            crate::style::show_num(&format!("{:.6}", ext[3])),
            units,
            epsg,
        ],
        save_defaults: false,
        error: None,
        confirm: None,
        roughness: Roughness::Formula,
        old,
        crs_selector: None,
    });
    app.dialog = crate::app::Dialog::Setup;
}

/// A two-column "property / value" table, as EPANET-UI's value list editor.
/// A row given a text by `button` shows a button with that text instead of a
/// field; returns the row whose button was clicked.
fn table(
    ui: &mut Ui,
    head: [&str; 2],
    names: &[&str],
    values: &mut [String],
    choices: &dyn Fn(usize) -> Option<&'static [&'static str]>,
    button: &dyn Fn(usize) -> Option<String>,
) -> Option<usize> {
    let mut clicked = None;
    let dark = ui.visuals().dark_mode;
    let white = if dark {
        ui.visuals().extreme_bg_color
    } else {
        Color32::WHITE
    };
    let line = Color32::from_gray(if dark { 80 } else { 200 });
    egui::Frame::NONE
        .fill(white)
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(150)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(300.0);
            let w = ui.available_width() / 2.0;
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            let row = |ui: &mut Ui,
                       left: &mut dyn FnMut(&mut Ui),
                       right: &mut dyn FnMut(&mut Ui),
                       fill: Option<Color32>| {
                let (r, _) =
                    ui.allocate_exact_size(egui::vec2(w * 2.0, 34.0), egui::Sense::hover());
                if let Some(f) = fill {
                    ui.painter().rect_filled(r, 0.0, f);
                }
                ui.painter()
                    .hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, line));
                ui.painter()
                    .vline(r.center().x, r.y_range(), egui::Stroke::new(1.0, line));
                let lr = egui::Rect::from_min_max(
                    r.min + egui::vec2(6.0, 0.0),
                    egui::pos2(r.center().x - 4.0, r.max.y),
                );
                let rr = egui::Rect::from_min_max(
                    egui::pos2(r.center().x + 4.0, r.min.y + 4.0),
                    r.max - egui::vec2(4.0, 4.0),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(lr)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| left(ui),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(rr)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| right(ui),
                );
            };
            let head_fill = if dark {
                ui.visuals().panel_fill
            } else {
                Color32::from_rgb(0xf3, 0xf3, 0xf3)
            };
            row(
                ui,
                &mut |ui| {
                    ui.label(head[0]);
                },
                &mut |ui| {
                    ui.label(head[1]);
                },
                Some(head_fill),
            );
            for (k, name) in names.iter().enumerate() {
                let v = &mut values[k];
                row(
                    ui,
                    &mut |ui| {
                        ui.label(*name);
                    },
                    &mut |ui| match (button(k), choices(k)) {
                        (Some(text), _) => {
                            if ui
                                .add_sized(
                                    [ui.available_width() - 4.0, 26.0],
                                    egui::Button::new(text).truncate(),
                                )
                                .clicked()
                            {
                                clicked = Some(k);
                            }
                        }
                        (None, Some(list)) => {
                            egui::ComboBox::from_id_salt(("setup_choice", *name))
                                .selected_text(v.clone())
                                .width(ui.available_width() - 4.0)
                                .show_ui(ui, |ui| {
                                    for c in list {
                                        if ui
                                            .selectable_label(v.eq_ignore_ascii_case(c), *c)
                                            .clicked()
                                        {
                                            *v = c.to_string();
                                        }
                                    }
                                });
                        }
                        (None, None) => {
                            ui.add(
                                egui::TextEdit::singleline(v)
                                    .frame(egui::Frame::NONE)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    },
                    None,
                );
            }
        });
    clicked
}

/// Whether flow units are SI (index 5 onwards, as in EPANET).
fn is_si(flow: &str) -> bool {
    FLOW_UNITS
        .iter()
        .position(|u| u.eq_ignore_ascii_case(flow))
        .is_some_and(|i| i >= 5)
}

/// Checks the entries (EPANET-UI's ValidateEditorValues); returns the page
/// and message of the first problem.
fn validate(e: &SetupEdit) -> Result<(), (usize, String)> {
    let num = eui_core::num::parse;
    for (k, p) in e.properties.iter().enumerate() {
        match num(p) {
            Some(x) if k == 0 || x > 0.0 => {}
            _ => return Err((2, format!("{} is not a valid number", p.trim()))),
        }
    }
    let mut ext = [0.0; 4];
    for (x, s) in ext.iter_mut().zip(&e.map) {
        *x = num(s).ok_or((3, format!("{} is not a valid number", s.trim())))?;
    }
    if e.map[4] == "Degrees"
        && !(ext[0].abs() <= 180.0
            && ext[2].abs() <= 180.0
            && ext[1].abs() <= 90.0
            && ext[3].abs() <= 90.0)
    {
        return Err((
            3,
            "Map coordinates are not valid longitudes/latitudes".into(),
        ));
    }
    let crs = e.map[5].trim();
    if !crs.is_empty() && !crs.starts_with('+') && crs.parse::<u32>().is_err() {
        return Err((3, format!("{crs} is not a valid EPSG code")));
    }
    for k in 3..9 {
        if num(&e.options[k]).is_none() {
            return Err((0, format!("{} is not a valid number", e.options[k].trim())));
        }
    }
    if e.prefixes
        .iter()
        .any(|p| p.contains(' ') || p.contains(';'))
    {
        return Err((1, "ID prefixes cannot contain spaces or semicolons".into()));
    }
    Ok(())
}

/// The confirmation message for unit and formula changes (EPANET-UI's
/// ConfirmChanges), or None if there are none.
fn changes_message(e: &SetupEdit, has_results: bool) -> Option<String> {
    let mut parts = Vec::new();
    if !e.options[0].eq_ignore_ascii_case(&e.old[0]) {
        let mut s = format!("- all flow data will be converted to {}", e.options[0]);
        match (is_si(&e.old[0]), is_si(&e.options[0])) {
            (false, true) => s.push_str("\n- all other data will be converted to SI units"),
            (true, false) => s.push_str("\n- all other data will be converted to US units"),
            _ => {}
        }
        parts.push(s);
    }
    if !e.options[1].eq_ignore_ascii_case(&e.old[1]) {
        parts.push(format!(
            "- all pressure data will be converted to {}",
            e.options[1]
        ));
    }
    if !e.options[2].eq_ignore_ascii_case(&e.old[2]) {
        parts.push(format!(
            "The project's head loss formula was changed from {} to {}.",
            e.old[2], e.options[2]
        ));
    }
    if parts.is_empty() {
        return None;
    }
    let mut msg = format!(
        "Setup will modify your project data as follows:\n\n{}",
        parts.join("\n\n")
    );
    if has_results {
        msg.push_str("\n\nBecause of these changes, simulation results will be removed.");
    }
    Some(msg)
}

/// The Project Setup window.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    let has_results = app.results.is_some();
    let empty = app.net.nodes.is_empty();
    let crs_recent = app.prefs.crs_recent.clone();
    let Some(e) = app.setup_edit.as_mut() else {
        return true;
    };
    ui.set_width(500.0);
    let red = Color32::from_rgb(0xc0, 0x39, 0x2b);

    // Confirmation of unit / formula changes
    if let Some(msg) = e.confirm.clone() {
        ui.label(RichText::new("Confirm Setup Actions").strong().size(15.0));
        ui.add_space(8.0);
        ui.label(&msg);
        let formula_changed = !e.options[2].eq_ignore_ascii_case(&e.old[2]);
        if formula_changed {
            let cm = e.old[2] == "C-M" || e.options[2] == "C-M";
            ui.add_space(8.0);
            ui.label("Select how pipe roughness should be changed:");
            if !cm {
                crate::widgets::radio_value(
                    ui,
                    &mut e.roughness,
                    Roughness::Formula,
                    "Use a built-in conversion formula",
                );
            } else if e.roughness == Roughness::Formula {
                e.roughness = Roughness::Default;
            }
            crate::widgets::radio_value(
                ui,
                &mut e.roughness,
                Roughness::Default,
                format!(
                    "Use the default roughness value of {}",
                    e.properties[5].trim()
                ),
            );
            crate::widgets::radio_value(
                ui,
                &mut e.roughness,
                Roughness::Keep,
                "Do not make any roughness changes",
            );
        }
        ui.add_space(12.0);
        match crate::dialogs::buttons(ui, &["OK", "Cancel"]) {
            Some(0) => {
                apply(app, &ctx);
                return true;
            }
            Some(_) => e.confirm = None,
            None => {}
        }
        return false;
    }

    // Page selector
    egui::Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(210)))
        .inner_margin(egui::Margin::symmetric(8, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                for (k, name) in ["Options", "ID Labels", "Properties", "Map"]
                    .iter()
                    .enumerate()
                {
                    crate::widgets::radio_value(ui, &mut e.page, k, *name);
                    ui.add_space(24.0);
                }
            });
        });
    let _ = match e.page {
        0 => table(
            ui,
            ["Project Option", "Value"],
            &OPTION_NAMES,
            &mut e.options,
            &|k| match k {
                0 => Some(&FLOW_UNITS),
                1 => Some(&PRESSURE_UNITS),
                2 => Some(&HEADLOSS),
                _ => None,
            },
            &|_| None,
        ),
        1 => table(
            ui,
            ["Object Type", "ID Prefix"],
            &PREFIX_NAMES,
            &mut e.prefixes,
            &|_| None,
            &|_| None,
        ),
        2 => table(
            ui,
            ["Object Property", "Value"],
            &PROPERTY_NAMES,
            &mut e.properties,
            &|_| None,
            &|_| None,
        ),
        _ => {
            // the coordinate reference system, chosen as from the status bar
            let crs = e.map[5].trim().to_string();
            let crs = if crs == "0" { String::new() } else { crs };
            let label = crate::basemap::crs_label(&crs);
            let clicked = table(
                ui,
                ["Map Property", "Value"],
                &MAP_NAMES,
                &mut e.map,
                &|k| (k == 4).then_some(&MAP_UNITS[..]),
                &|k| (k == 5).then(|| label.clone()),
            );
            if clicked == Some(5) {
                e.crs_selector = Some(crate::basemap::CrsSelector::new(&crs));
            }
            None
        }
    };
    if let Some(sel) = &mut e.crs_selector
        && let Some(choice) = crate::basemap::crs_selector(&ctx, sel, &crs_recent)
    {
        e.crs_selector = None;
        if let Some(c) = choice {
            e.map[4] = MAP_UNITS[crate::basemap::crs_units(&c)].to_string();
            e.map[5] = c;
        }
    }
    // hint panel
    egui::Frame::NONE
        .fill(if ui.visuals().dark_mode {
            Color32::from_rgb(0x3a, 0x38, 0x2a)
        } else {
            crate::style::HINT_BG
        })
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(200)))
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(HINTS[e.page]);
        });
    ui.add_space(10.0);
    ui.label(format!(
        "Unit System (determined by Flow Units):  {}",
        if is_si(&e.options[0]) { "SI" } else { "US" }
    ));
    ui.add_space(6.0);
    crate::widgets::checkbox(
        ui,
        &mut e.save_defaults,
        "Save as defaults for new projects",
    );
    if let Some(err) = &e.error {
        ui.label(RichText::new(err).color(red));
    }
    ui.add_space(10.0);
    let (mut close, mut ok) = (false, false);
    ui.horizontal(|ui| {
        if ui
            .add_sized([96.0, 30.0], egui::Button::new("Help"))
            .clicked()
        {
            crate::help::open(&ctx, "Project Setup");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            close = ui
                .add_sized([96.0, 30.0], egui::Button::new("Cancel"))
                .clicked();
            ok = ui
                .add_sized([96.0, 30.0], egui::Button::new("OK"))
                .clicked();
        });
    });
    if ok {
        match validate(e) {
            Err((page, msg)) => {
                e.page = page;
                e.error = Some(msg);
            }
            Ok(()) => {
                e.error = None;
                match changes_message(e, has_results) {
                    Some(msg) if !empty => e.confirm = Some(msg),
                    _ => {
                        apply(app, &ctx);
                        close = true;
                    }
                }
            }
        }
    }
    if close {
        app.setup_edit = None;
    }
    close
}

/// Puts the dialog's choices into the project.
fn apply(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(mut e) = app.setup_edit.take() else {
        return;
    };
    // numbers typed with a decimal comma are stored with a point
    for v in e.options[3..]
        .iter_mut()
        .chain(&mut e.properties)
        .chain(&mut e.map[..4])
    {
        *v = eui_core::num::normalize(v);
    }
    app.checkpoint();
    for (key, value) in OPTION_KEYS.iter().zip(&e.options).skip(3) {
        app.net.set_option("OPTIONS", key, value.trim());
    }
    app.setup.prefixes = e.prefixes.clone();
    app.setup.properties = e.properties.clone();
    if e.save_defaults {
        app.prefs.setup = SetupDefaults {
            prefixes: e.prefixes.clone(),
            properties: e.properties.clone(),
            options: Some(e.options.clone()),
        };
    }
    // map
    let ext: Vec<f64> = e.map[..4]
        .iter()
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    if ext.len() == 4 {
        let dims = format!("{:.6} {:.6} {:.6} {:.6}", ext[0], ext[1], ext[2], ext[3]);
        if app.net.option("BACKDROP", "DIMENSIONS").as_deref() != Some(dims.as_str()) {
            app.net.set_option("BACKDROP", "DIMENSIONS", &dims);
        }
    }
    app.net
        .set_option("BACKDROP", "UNITS", &e.map[4].to_ascii_uppercase());
    let crs = e.map[5].trim();
    match crs.parse::<u32>() {
        Ok(code) if code > 0 => app.set_project_crs(format!("EPSG:{code}")),
        _ if crs.starts_with('+') => app.set_project_crs(crs.to_string()),
        _ if e.map[4] == "Degrees" => app.set_project_crs("EPSG:4326".into()),
        _ => {}
    }
    // units and head loss formula, converted by EPANET
    let index = |list: &[&str], v: &str| {
        list.iter()
            .position(|u| u.eq_ignore_ascii_case(v))
            .map_or(-1, |i| i as i32)
    };
    let flow_changed = !e.options[0].eq_ignore_ascii_case(&e.old[0]);
    let press_changed = !e.options[1].eq_ignore_ascii_case(&e.old[1]);
    let model_changed = !e.options[2].eq_ignore_ascii_case(&e.old[2]);
    if !(flow_changed || press_changed || model_changed) {
        return;
    }
    if app.net.nodes.is_empty() {
        // nothing to convert
        app.net
            .set_option("OPTIONS", "UNITS", &e.options[0].to_ascii_uppercase());
        app.net
            .set_option("OPTIONS", "PRESSURE", &e.options[1].to_ascii_uppercase());
        app.net
            .set_option("OPTIONS", "HEADLOSS", &e.options[2].to_ascii_uppercase());
        return;
    }
    let change = eui_engine::UnitChange {
        flow_units: if flow_changed {
            index(&FLOW_UNITS, &e.options[0])
        } else {
            -1
        },
        pressure_units: if press_changed {
            index(&PRESSURE_UNITS, &e.options[1])
        } else {
            -1
        },
        headloss: if model_changed {
            index(&HEADLOSS, &e.options[2])
        } else {
            -1
        },
    };
    let repaint = ctx.clone();
    app.setup_job = Some(SetupJob {
        handle: eui_engine::spawn_convert(app.net.to_inp(), change, move || {
            repaint.request_repaint()
        }),
        before: app.net.clone(),
        roughness: if model_changed {
            e.roughness
        } else {
            Roughness::Keep
        },
        old_model: e.old[2].clone(),
        new_model: e.options[2].clone(),
        default_roughness: e.properties[5].trim().parse().unwrap_or(130.0),
    });
    app.status = "Converting the project's units…".into();
}

/// Finishes a unit conversion when EPANET is done with it.
pub fn poll(app: &mut EpanetApp) {
    let Some(job) = app.setup_job.as_mut() else {
        return;
    };
    let Some(result) = job.handle.try_take() else {
        return;
    };
    let job = app.setup_job.take().unwrap();
    let text = match result {
        Ok(t) => t,
        Err(e) => {
            app.status = format!("Could not convert the project: {e}");
            return;
        }
    };
    let mut net = match Network::from_inp(&text) {
        Ok(n) => n,
        Err(e) => {
            app.status = format!("Could not read the converted project: {e}");
            return;
        }
    };
    // Sections EPANET does not keep come back from the project as it was.
    for s in &job.before.raw {
        if !net.raw.iter().any(|r| r.name == s.name) {
            net.raw.push(s.clone());
        }
    }
    restore_junction_demands(&mut net);
    convert_roughness(
        &mut net,
        job.roughness,
        &job.old_model,
        &job.new_model,
        job.default_roughness,
    );
    app.net = net;
    app.results = None;
    app.touch();
    app.status = "Project setup applied".into();
}

/// EPANET writes every demand to [DEMANDS]; a junction with a single demand
/// gets it back on its own line (the same to EPANET), so the Property Editor
/// shows it as Base Demand. With several categories the first one is shown.
fn restore_junction_demands(net: &mut Network) {
    let Some(sec) = net.raw.iter().position(|s| s.name == "DEMANDS") else {
        return;
    };
    let entries: Vec<(usize, Vec<String>)> = net.raw[sec]
        .lines
        .iter()
        .enumerate()
        .filter_map(|(k, l)| {
            let data = l.split(';').next().unwrap_or("");
            let t: Vec<String> = data.split_whitespace().map(String::from).collect();
            (t.len() >= 2).then_some((k, t))
        })
        .collect();
    let mut remove = Vec::new();
    for (k, t) in &entries {
        let count = entries.iter().filter(|(_, u)| u[0] == t[0]).count();
        let Some(i) = net.node_index(&t[0]) else {
            continue;
        };
        let node = &mut net.nodes[i];
        if node.kind != NodeKind::Junction || !node.field(1).is_empty() {
            continue;
        }
        node.fields.resize(3, String::new());
        node.fields[1] = t[1].clone();
        node.fields[2] = t.get(2).cloned().unwrap_or_default();
        if count == 1 {
            remove.push(*k);
        }
    }
    let lines = &mut net.raw[sec].lines;
    for k in remove.into_iter().rev() {
        lines.remove(k);
    }
}

/// Changes pipe roughness for a new head loss formula (EPANET-UI's
/// ConvertRoughness, with the Adams (2016) formulas between H-W and D-W).
fn convert_roughness(net: &mut Network, how: Roughness, old: &str, new: &str, default: f64) {
    let si = net.is_si();
    for l in net.links.iter_mut().filter(|l| l.kind == LinkKind::Pipe) {
        let (Some(d), Some(r)) = (l.num(1), l.num(2)) else {
            continue;
        };
        let value = match how {
            Roughness::Keep => continue,
            Roughness::Default => default,
            Roughness::Formula => {
                let dm = d * if si { 0.001 } else { 0.0254 };
                if old.eq_ignore_ascii_case("H-W") && new.eq_ignore_ascii_case("D-W") {
                    // C to roughness height (mm or millifeet)
                    let e = 3.7 * dm * (-r * dm.powf(0.068) / 13.9).exp();
                    e * if si { 1000.0 } else { 1000.0 / 0.3048 }
                } else if old.eq_ignore_ascii_case("D-W") && new.eq_ignore_ascii_case("H-W") {
                    let e = r * if si { 0.001 } else { 0.0003048 };
                    -13.9 * dm.powf(-0.068) * (e / 3.7 / dm).ln()
                } else {
                    continue;
                }
            }
        };
        l.fields[2] = format!("{value:.4}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demands_return_to_junction_lines() {
        let mut net = Network::from_inp(eui_core::examples::NET1).unwrap();
        let i = net.node_index("11").unwrap();
        let demand = net.nodes[i].field(1).to_string();
        net.nodes[i].fields[1].clear();
        net.set_option("DEMANDS", "11", &format!("{demand} 1"));
        restore_junction_demands(&mut net);
        assert_eq!(net.nodes[i].field(1), demand);
        assert_eq!(net.raw_lines("DEMANDS").count(), 0);
    }

    #[test]
    fn roughness_round_trip() {
        let mut net = Network::from_inp(eui_core::examples::NET1).unwrap();
        let before: Vec<f64> = net.links.iter().filter_map(|l| l.num(2)).collect();
        convert_roughness(&mut net, Roughness::Formula, "H-W", "D-W", 0.0);
        convert_roughness(&mut net, Roughness::Formula, "D-W", "H-W", 0.0);
        let after: Vec<f64> = net.links.iter().filter_map(|l| l.num(2)).collect();
        for (a, b) in before.iter().zip(&after) {
            assert!((a - b).abs() < 0.01, "{a} vs {b}");
        }
    }
}
