//! EPANET-UI's data forms, opened from the Project Explorer and from the
//! Property Editor: the Time Pattern and Data Curve Selectors and Editors,
//! the Single Species Water Quality Editor and the Simple and Rule-Based
//! Controls Editors.

use egui::{Color32, RichText, Ui, vec2};
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints};
use eui_core::num;
use eui_core::props::{self, Element, PropKey, PropKind};
use eui_core::{DataCurve, TimePattern};

use crate::app::{Dialog, EpanetApp};
use crate::style;

/// What a selector's choice is for (OK assigns it).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Pick {
    /// Opened from the Project Explorer: OK just closes.
    #[default]
    Browse,
    /// An element property, e.g. a junction's Demand Pattern.
    Prop(Element, PropKey),
    /// An analysis option, e.g. the Default Pattern.
    Option(&'static str, &'static str),
    /// The pattern of the source in the Source Editor.
    SourcePattern,
}

#[derive(Default)]
pub struct FormsState {
    pick: Pick,
    /// The row selected in a selector (with `<blank>` as row 0 when picking).
    selected: usize,
    /// A message shown in a selector (e.g. why a pattern cannot be deleted).
    note: Option<String>,
    pattern: Option<PatternEdit>,
    curve: Option<CurveEdit>,
    quality: Option<QualityEdit>,
    controls: Option<ControlsEdit>,
    rules: Option<RulesEdit>,
    inbox: crate::io::Inbox,
    /// The Property Editor panel's own selector row, note and editor, apart
    /// from those of a selector window.
    panel: PanelForms,
}

#[derive(Default)]
struct PanelForms {
    selected: usize,
    note: Option<String>,
    pattern: Option<PatternEdit>,
    curve: Option<CurveEdit>,
}

const RED: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);

/// EPANET-UI's typical diurnal pattern of water use, per hour.
const DIURNAL: [f64; 24] = [
    0.7, 0.6, 0.5, 0.5, 0.5, 0.6, 0.8, 1.0, 1.1, 1.25, 1.28, 1.2, 1.18, 1.16, 1.1, 1.0, 1.08, 1.15,
    1.3, 1.6, 1.4, 1.25, 0.9, 0.85,
];

/// A form's bottom row of buttons, EPANET-UI style: some on the left, some
/// on the right; in the panel they wrap. Returns the label clicked.
fn form_buttons(ui: &mut Ui, panel: bool, left: &[&str], right: &[(&str, bool)]) -> Option<String> {
    if !panel {
        return button_row(ui, left, right);
    }
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for (l, on) in left
            .iter()
            .map(|l| (l, true))
            .chain(right.iter().map(|(l, on)| (l, *on)))
        {
            if ui
                .add_enabled(on, egui::Button::new(*l).min_size(vec2(64.0, 26.0)))
                .clicked()
            {
                clicked = Some(l.to_string());
            }
        }
    });
    clicked
}

/// A form's bottom row of buttons, EPANET-UI style: some on the left, some
/// on the right. Returns the label clicked.
fn button_row(ui: &mut Ui, left: &[&str], right: &[(&str, bool)]) -> Option<String> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        for l in left {
            if ui.add_sized([72.0, 26.0], egui::Button::new(*l)).clicked() {
                clicked = Some(l.to_string());
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (l, on) in right.iter().rev() {
                if ui
                    .add_enabled(*on, egui::Button::new(*l).min_size(vec2(80.0, 26.0)))
                    .clicked()
                {
                    clicked = Some(l.to_string());
                }
            }
        });
    });
    clicked
}

// ----- selectors ------------------------------------------------------------

/// Opens the Time Pattern Selector; `pick` says what OK assigns.
pub fn open_patterns(app: &mut EpanetApp, pick: Pick) {
    let current = picked_value(app, &pick);
    let ids: Vec<String> = app.net.time_patterns().into_iter().map(|p| p.id).collect();
    app.forms.selected = selection_of(&pick, &ids, &current);
    app.forms.pick = pick;
    app.forms.note = None;
    app.dialog = Dialog::PatternSelector;
}

/// Opens the Data Curve Selector; `pick` says what OK assigns.
pub fn open_curves(app: &mut EpanetApp, pick: Pick) {
    let current = picked_value(app, &pick);
    let ids: Vec<String> = app.net.data_curves().into_iter().map(|c| c.id).collect();
    app.forms.selected = selection_of(&pick, &ids, &current);
    app.forms.pick = pick;
    app.forms.note = None;
    app.dialog = Dialog::CurveSelector;
}

/// The row to select first: the current value's (or `<blank>`).
fn selection_of(pick: &Pick, ids: &[String], current: &str) -> usize {
    let blank = usize::from(*pick != Pick::Browse);
    ids.iter()
        .position(|id| id == current)
        .map_or(0, |k| k + blank)
}

fn picked_value(app: &EpanetApp, pick: &Pick) -> String {
    match pick {
        Pick::Browse => String::new(),
        Pick::Prop(e, key) => app.net.get_prop(*e, *key),
        Pick::Option(sec, key) => app.net.option(sec, key).unwrap_or_default(),
        Pick::SourcePattern => app
            .source_edit
            .as_ref()
            .map(|e| e.pattern.clone())
            .unwrap_or_default(),
    }
}

/// Assigns the choice of a selector (empty for `<blank>`).
fn assign(app: &mut EpanetApp, value: &str) {
    match app.forms.pick.clone() {
        Pick::Browse => {}
        Pick::Prop(e, key) => {
            if app.net.get_prop(e, key) != value {
                let mut trial = app.net.clone();
                match trial.set_prop(e, key, value) {
                    Ok(()) => {
                        app.checkpoint();
                        app.net = trial;
                    }
                    Err(err) => app.prop_hint = err,
                }
            }
        }
        Pick::Option(sec, key) => {
            if app.net.option(sec, key).unwrap_or_default() != value {
                app.checkpoint();
                app.net.set_option(sec, key, value);
            }
        }
        Pick::SourcePattern => {
            if let Some(e) = &mut app.source_edit {
                e.pattern = value.to_string();
            }
        }
    }
}

/// Where a selector returns to when it closes.
fn after_selector(app: &mut EpanetApp) -> bool {
    if app.forms.pick == Pick::SourcePattern {
        app.dialog = Dialog::SourceEditor;
        return false;
    }
    true
}

/// A list cell: its text on the left, highlighted when selected (EPANET-UI's
/// grids), of a fixed width.
fn cell(ui: &mut Ui, width: f32, selected: bool, text: RichText) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), egui::Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, ui.visuals().selection.bg_fill);
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        width - 8.0,
        egui::TextStyle::Body,
    );
    let color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    ui.painter().galley(
        egui::pos2(rect.left() + 4.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    resp
}

/// A selector's list: a header and one selectable row per entry. Returns
/// the row double-clicked. Each list has its own `id`, so one does not take
/// the column widths of another shown in the same place before it.
fn list(
    ui: &mut Ui,
    id: &str,
    columns: &[(&str, f32)],
    rows: &[Vec<String>],
    selected: &mut usize,
    height: f32,
) -> Option<usize> {
    let mut open = None;
    egui::Frame::NONE
        .fill(if ui.visuals().dark_mode {
            ui.visuals().extreme_bg_color
        } else {
            Color32::WHITE
        })
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(160)))
        .show(ui, |ui| {
            let w = columns.iter().map(|c| c.1).sum::<f32>();
            ui.set_min_size(vec2(w, height));
            ui.set_max_width(w + 16.0);
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(height)
                .max_width(w + 16.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new(id)
                        .num_columns(columns.len())
                        .spacing([0.0, 2.0])
                        .show(ui, |ui| {
                            for (h, w) in columns {
                                ui.add_sized(
                                    [*w, 20.0],
                                    egui::Label::new(RichText::new(*h).strong()),
                                );
                            }
                            ui.end_row();
                            for (k, row) in rows.iter().enumerate() {
                                for (c, (_, w)) in columns.iter().enumerate() {
                                    let text = row.get(c).cloned().unwrap_or_default();
                                    let r = cell(ui, *w, *selected == k, RichText::new(text));
                                    if r.clicked() {
                                        *selected = k;
                                    }
                                    if r.double_clicked() {
                                        open = Some(k);
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    open
}

/// Runs `f` on the panel's own selector state (browsing, never picking),
/// leaving a selector window's state and the open dialog as they were.
fn in_panel(app: &mut EpanetApp, f: impl FnOnce(&mut EpanetApp)) {
    let swap = |app: &mut EpanetApp| {
        let f = &mut app.forms;
        std::mem::swap(&mut f.selected, &mut f.panel.selected);
        std::mem::swap(&mut f.note, &mut f.panel.note);
        std::mem::swap(&mut f.pattern, &mut f.panel.pattern);
        std::mem::swap(&mut f.curve, &mut f.panel.curve);
    };
    let dialog = app.dialog;
    let pick = std::mem::take(&mut app.forms.pick);
    swap(app);
    f(app);
    swap(app);
    app.forms.pick = pick;
    app.dialog = dialog;
}

/// The Time Patterns inside the Property Editor panel: the selector, or the
/// editor of the pattern added or edited.
pub fn patterns_panel(app: &mut EpanetApp, ui: &mut Ui) {
    let w = ui.available_width() - 20.0;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            in_panel(app, |app| {
                if app.forms.pattern.is_some() {
                    pattern_editor_body(app, ui, Some(w));
                } else {
                    pattern_selector_body(app, ui, Some(w));
                }
            })
        });
}

/// The Data Curves inside the Property Editor panel: the selector, or the
/// editor of the curve added or edited.
pub fn curves_panel(app: &mut EpanetApp, ui: &mut Ui) {
    let w = ui.available_width() - 20.0;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            in_panel(app, |app| {
                if app.forms.curve.is_some() {
                    curve_editor_body(app, ui, Some(w));
                } else {
                    curve_selector_body(app, ui, Some(w));
                }
            })
        });
}

/// A selector's Add / Edit / Delete (and Help) under its list, in the panel.
fn panel_actions(ui: &mut Ui, on: bool, action: &mut Option<&'static str>) {
    ui.horizontal_wrapped(|ui| {
        for (label, enabled) in [("Add", true), ("Edit", on), ("Delete", on), ("Help", true)] {
            if ui
                .add_enabled(enabled, egui::Button::new(label).min_size(vec2(64.0, 26.0)))
                .clicked()
            {
                *action = Some(label);
            }
        }
    });
}

/// The Time Pattern Selector.
pub fn pattern_selector(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    pattern_selector_body(app, ui, None)
}

/// The Time Pattern Selector, in a window or (`panel`: its width) in the
/// Property Editor panel.
fn pattern_selector_body(app: &mut EpanetApp, ui: &mut Ui, panel: Option<f32>) -> bool {
    let pats = app.net.time_patterns();
    let picking = app.forms.pick != Pick::Browse;
    let blank = usize::from(picking);
    let mut rows: Vec<Vec<String>> = Vec::new();
    if picking {
        rows.push(vec!["<blank>".into(), String::new()]);
    }
    rows.extend(
        pats.iter()
            .map(|p| vec![p.id.clone(), p.description.clone()]),
    );
    app.forms.selected = app.forms.selected.min(rows.len().saturating_sub(1));
    ui.label("Select a Time Pattern from the list below:");
    ui.add_space(4.0);
    let mut action = None;
    if let Some(w) = panel {
        let mut sel = app.forms.selected;
        let c = (w - 16.0).max(100.0);
        let cols = [("Name", c * 0.35), ("Description", c * 0.65)];
        if list(ui, "pattern_list", &cols, &rows, &mut sel, 180.0).is_some() {
            action = Some("Edit");
        }
        app.forms.selected = sel;
        ui.add_space(4.0);
        panel_actions(ui, sel >= blank && !pats.is_empty(), &mut action);
    } else {
        ui.horizontal_top(|ui| {
            let mut sel = app.forms.selected;
            if list(
                ui,
                "pattern_list",
                &[("Name", 140.0), ("Description", 260.0)],
                &rows,
                &mut sel,
                300.0,
            )
            .is_some()
            {
                action = Some("Edit");
            }
            app.forms.selected = sel;
            ui.add_space(10.0);
            ui.vertical(|ui| {
                let on_pattern = app.forms.selected >= blank && !pats.is_empty();
                for (label, on) in [("Add", true), ("Edit", on_pattern), ("Delete", on_pattern)] {
                    if ui
                        .add_enabled(on, egui::Button::new(label).min_size(vec2(90.0, 26.0)))
                        .clicked()
                    {
                        action = Some(label);
                    }
                }
                ui.add_space(80.0);
                for label in ["Help", "OK", "Cancel"] {
                    if ui
                        .add(egui::Button::new(label).min_size(vec2(90.0, 26.0)))
                        .clicked()
                    {
                        action = Some(label);
                    }
                }
            });
        });
    }
    let chosen = app
        .forms
        .selected
        .checked_sub(blank)
        .and_then(|k| pats.get(k));
    // a preview of the selected pattern
    ui.add_space(8.0);
    let size = vec2(panel.unwrap_or(500.0), 120.0);
    if let Some(p) = chosen {
        pattern_chart(ui, "pattern_preview", &p.values, size, &step_hours(app));
    } else {
        ui.allocate_space(size);
    }
    if let Some(n) = &app.forms.note {
        ui.label(RichText::new(n).color(RED));
    }
    match action {
        Some("Add") => {
            let ids: Vec<String> = pats.iter().map(|p| p.id.clone()).collect();
            let id = (1..)
                .map(|i| i.to_string())
                .find(|id| !ids.contains(id))
                .unwrap_or_default();
            app.forms.pattern = Some(PatternEdit::new(None, id, String::new(), vec![1.0]));
            app.dialog = Dialog::PatternEditor;
        }
        Some("Edit") => {
            if let Some(p) = chosen {
                app.forms.pattern = Some(PatternEdit::new(
                    Some(p.id.clone()),
                    p.id.clone(),
                    p.description.clone(),
                    p.values.clone(),
                ));
                app.dialog = Dialog::PatternEditor;
            }
        }
        Some("Delete") => {
            if let Some(p) = chosen {
                let users = app.net.pattern_users(&p.id);
                if users.is_empty() {
                    let rest: Vec<TimePattern> =
                        pats.iter().filter(|q| q.id != p.id).cloned().collect();
                    app.checkpoint();
                    app.net.set_time_patterns(&rest);
                    app.forms.note = None;
                } else {
                    app.forms.note =
                        Some(format!("Pattern {} is in use: {}", p.id, users.join(", ")));
                }
            }
        }
        Some("Help") => crate::help::open(ui.ctx(), "Time Patterns"),
        Some("OK") => {
            let value = chosen.map(|p| p.id.clone()).unwrap_or_default();
            assign(app, &value);
            return after_selector(app);
        }
        Some("Cancel") => return after_selector(app),
        _ => {}
    }
    false
}

/// The Data Curve Selector.
pub fn curve_selector(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    curve_selector_body(app, ui, None)
}

/// The Data Curve Selector, in a window or (`panel`: its width) in the
/// Property Editor panel.
fn curve_selector_body(app: &mut EpanetApp, ui: &mut Ui, panel: Option<f32>) -> bool {
    let curves = app.net.data_curves();
    let picking = app.forms.pick != Pick::Browse;
    let blank = usize::from(picking);
    let mut rows: Vec<Vec<String>> = Vec::new();
    if picking {
        rows.push(vec!["<blank>".into(), String::new(), String::new()]);
    }
    rows.extend(curves.iter().map(|c| {
        vec![
            c.id.clone(),
            curve_type_label(&c.kind).into(),
            c.description.clone(),
        ]
    }));
    app.forms.selected = app.forms.selected.min(rows.len().saturating_sub(1));
    ui.label("Select a Data Curve from the list below:");
    ui.add_space(4.0);
    let mut action = None;
    if let Some(w) = panel {
        let mut sel = app.forms.selected;
        let c = (w - 16.0).max(120.0);
        let cols = [
            ("Name", c * 0.3),
            ("Type", c * 0.25),
            ("Description", c * 0.45),
        ];
        if list(ui, "curve_list", &cols, &rows, &mut sel, 220.0).is_some() {
            action = Some("Edit");
        }
        app.forms.selected = sel;
        ui.add_space(4.0);
        panel_actions(ui, sel >= blank && !curves.is_empty(), &mut action);
    } else {
        ui.horizontal_top(|ui| {
            let mut sel = app.forms.selected;
            if list(
                ui,
                "curve_list",
                &[("Name", 120.0), ("Type", 80.0), ("Description", 220.0)],
                &rows,
                &mut sel,
                300.0,
            )
            .is_some()
            {
                action = Some("Edit");
            }
            app.forms.selected = sel;
            ui.add_space(10.0);
            ui.vertical(|ui| {
                let on_curve = app.forms.selected >= blank && !curves.is_empty();
                for (label, on) in [("Add", true), ("Edit", on_curve), ("Delete", on_curve)] {
                    if ui
                        .add_enabled(on, egui::Button::new(label).min_size(vec2(90.0, 26.0)))
                        .clicked()
                    {
                        action = Some(label);
                    }
                }
                ui.add_space(110.0);
                for label in ["Help", "OK", "Cancel"] {
                    if ui
                        .add(egui::Button::new(label).min_size(vec2(90.0, 26.0)))
                        .clicked()
                    {
                        action = Some(label);
                    }
                }
            });
        });
    }
    if let Some(n) = &app.forms.note {
        ui.label(RichText::new(n).color(RED));
    }
    let chosen = app
        .forms
        .selected
        .checked_sub(blank)
        .and_then(|k| curves.get(k));
    match action {
        Some("Add") => {
            let ids: Vec<String> = curves.iter().map(|c| c.id.clone()).collect();
            let id = (1..)
                .map(|i| i.to_string())
                .find(|id| !ids.contains(id))
                .unwrap_or_default();
            // a curve asked for by a pump is a pump curve, and so on
            let kind = match &app.forms.pick {
                Pick::Prop(_, PropKey::PumpParam("HEAD")) => "PUMP",
                Pick::Prop(_, PropKey::PumpEnergy("EFFIC")) => "EFFICIENCY",
                Pick::Prop(Element::Node(_), _) => "VOLUME",
                _ => "",
            };
            app.forms.curve = Some(CurveEdit::new(
                None,
                &DataCurve {
                    id,
                    kind: kind.into(),
                    ..Default::default()
                },
            ));
            app.dialog = Dialog::CurveEditor;
        }
        Some("Edit") => {
            if let Some(c) = chosen {
                app.forms.curve = Some(CurveEdit::new(Some(c.id.clone()), c));
                app.dialog = Dialog::CurveEditor;
            }
        }
        Some("Delete") => {
            if let Some(c) = chosen {
                let users = app.net.curve_users(&c.id);
                if users.is_empty() {
                    let rest: Vec<DataCurve> =
                        curves.iter().filter(|d| d.id != c.id).cloned().collect();
                    app.checkpoint();
                    app.net.set_data_curves(&rest);
                    app.forms.note = None;
                } else {
                    app.forms.note =
                        Some(format!("Curve {} is in use: {}", c.id, users.join(", ")));
                }
            }
        }
        Some("Help") => crate::help::open(ui.ctx(), "Data Curves"),
        Some("OK") => {
            let value = chosen.map(|c| c.id.clone()).unwrap_or_default();
            assign(app, &value);
            return after_selector(app);
        }
        Some("Cancel") => return after_selector(app),
        _ => {}
    }
    false
}

// ----- time pattern editor --------------------------------------------------

struct PatternEdit {
    /// The pattern's ID before editing (None for a new one).
    original: Option<String>,
    id: String,
    description: String,
    values: Vec<String>,
    error: Option<String>,
}

impl PatternEdit {
    fn new(original: Option<String>, id: String, description: String, values: Vec<f64>) -> Self {
        PatternEdit {
            original,
            id,
            description,
            values: values
                .iter()
                .map(|v| style::show_num(&format!("{v:.4}")))
                .collect(),
            error: None,
        }
    }
}

/// The pattern step in hours, for chart titles.
fn step_hours(app: &EpanetApp) -> String {
    let secs = app
        .net
        .option("TIMES", "PATTERN TIMESTEP")
        .and_then(|s| eui_core::analysis::parse_time(&s))
        .unwrap_or(3600);
    format!("{:.2}", secs as f64 / 3600.0)
}

fn pattern_chart(ui: &mut Ui, id: &str, values: &[f64], size: egui::Vec2, step: &str) {
    let fill = Color32::from_rgb(0xff, 0x80, 0x80);
    let bars: Vec<Bar> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            Bar::new(i as f64 + 0.5, v)
                .width(1.0)
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, Color32::BLACK))
        })
        .collect();
    Plot::new(id)
        .show_crosshair(false)
        .width(size.x)
        .height(size.y)
        .x_axis_label(format!("Time (Time Period = {step} hrs)"))
        .allow_scroll(false)
        .allow_drag(false)
        .allow_zoom(false)
        .show(ui, |p| p.bar_chart(BarChart::new("Multiplier", bars)));
}

/// The Time Pattern Editor. Accept and Cancel return to the selector.
pub fn pattern_editor(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    pattern_editor_body(app, ui, None)
}

/// The Time Pattern Editor, in a window or (`panel`: its width) in the
/// Property Editor panel.
fn pattern_editor_body(app: &mut EpanetApp, ui: &mut Ui, panel: Option<f32>) -> bool {
    let step = step_hours(app);
    take_loaded(app);
    let Some(e) = &mut app.forms.pattern else {
        app.dialog = Dialog::PatternSelector;
        return false;
    };
    let w = panel.unwrap_or(460.0);
    ui.set_width(w);
    if panel.is_some() {
        ui.label("Pattern ID");
        ui.add(egui::TextEdit::singleline(&mut e.id).desired_width(120.0));
        ui.label("Description");
        ui.add(egui::TextEdit::singleline(&mut e.description).desired_width(w));
        if ui.link("Use typical diurnal pattern").clicked() {
            e.values = DIURNAL
                .iter()
                .map(|v| style::show_num(&format!("{v:.4}")))
                .collect();
        }
    } else {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Pattern ID");
                ui.add(egui::TextEdit::singleline(&mut e.id).desired_width(120.0));
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Description");
                    ui.add_space(60.0);
                    if ui.link("Use typical diurnal pattern").clicked() {
                        e.values = DIURNAL
                            .iter()
                            .map(|v| style::show_num(&format!("{v:.4}")))
                            .collect();
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut e.description).desired_width(300.0));
            });
        });
    }
    ui.add_space(8.0);
    // Period / Multiplier, one column per period, plus an empty one to add
    if e.values.last().is_none_or(|v| !v.trim().is_empty()) {
        e.values.push(String::new());
    }
    egui::ScrollArea::horizontal()
        .id_salt("pattern_cells")
        .max_width(panel.unwrap_or(440.0))
        .show(ui, |ui| {
            egui::Grid::new("pattern_cells_grid")
                .striped(true)
                .spacing([2.0, 2.0])
                .show(ui, |ui| {
                    ui.add_sized([70.0, 20.0], egui::Label::new("Period"));
                    for k in 0..e.values.len() {
                        ui.add_sized([60.0, 20.0], egui::Label::new((k + 1).to_string()));
                    }
                    ui.end_row();
                    ui.add_sized([70.0, 20.0], egui::Label::new("Multiplier"));
                    for v in &mut e.values {
                        ui.add(egui::TextEdit::singleline(v).desired_width(60.0));
                    }
                    ui.end_row();
                });
        });
    // trailing empty cells are not periods
    while e.values.len() > 1
        && e.values.last().is_some_and(|v| v.trim().is_empty())
        && e.values[e.values.len() - 2].trim().is_empty()
    {
        e.values.pop();
    }
    let vals: Vec<f64> = e.values.iter().filter_map(|v| num::parse(v)).collect();
    ui.add_space(8.0);
    let size = panel.map_or(vec2(440.0, 200.0), |w| vec2(w, 160.0));
    pattern_chart(ui, "pattern_editor_chart", &vals, size, &step);
    if let Some(err) = &e.error {
        ui.label(RichText::new(err).color(RED));
    }
    ui.add_space(8.0);
    let clicked = form_buttons(
        ui,
        panel.is_some(),
        &["Help", "Load", "Save", "Clear"],
        &[("Accept", true), ("Cancel", true)],
    );
    match clicked.as_deref() {
        Some("Help") => crate::help::open(ui.ctx(), "Time Patterns"),
        Some("Load") => crate::io::open_file(
            &app.forms.inbox,
            ui.ctx(),
            "EPANET pattern file",
            &["pat", "PAT", "txt"],
        ),
        Some("Save") => {
            let text = pattern_file(&e.id, &e.description, &vals);
            if let Err(err) = crate::io::save_report(&text, &format!("{}.pat", e.id), ui.ctx()) {
                e.error = Some(err);
            }
        }
        Some("Clear") => e.values = vec![String::new()],
        Some("Accept") => match accept_pattern(app) {
            Ok(()) => {
                app.forms.pattern = None;
                app.dialog = Dialog::PatternSelector;
            }
            Err(err) => {
                if let Some(e) = &mut app.forms.pattern {
                    e.error = Some(err);
                }
            }
        },
        Some("Cancel") => {
            app.forms.pattern = None;
            app.dialog = Dialog::PatternSelector;
        }
        _ => {}
    }
    false
}

/// EPANET's pattern file: a header, the ID, the description, then one
/// multiplier per line.
fn pattern_file(id: &str, description: &str, values: &[f64]) -> String {
    let mut s = format!("EPANET Pattern Data\n{id}\n{description}\n");
    for v in values {
        s += &format!("{v}\n");
    }
    s
}

/// Reads an EPANET pattern file into the editor (or plain multipliers).
fn take_loaded(app: &mut EpanetApp) {
    for item in app.forms.inbox.drain() {
        let Ok((_, bytes)) = item else { continue };
        let text = crate::io::decode_text(&bytes);
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        if let Some(e) = &mut app.forms.pattern {
            let (desc, body) = if lines.first().is_some_and(|l| l.starts_with("EPANET")) {
                (
                    lines.get(2).copied().unwrap_or(""),
                    lines.get(3..).unwrap_or(&[]),
                )
            } else {
                ("", &lines[..])
            };
            let vals: Vec<String> = body
                .iter()
                .flat_map(|l| l.split_whitespace())
                .filter(|v| num::is_number(v))
                .map(|v| style::show_num(&num::normalize(v)))
                .collect();
            if !vals.is_empty() {
                e.values = vals;
                if !desc.is_empty() {
                    e.description = desc.to_string();
                }
            }
        }
    }
}

fn accept_pattern(app: &mut EpanetApp) -> Result<(), String> {
    let e = app.forms.pattern.as_ref().ok_or("no pattern")?;
    let id = e.id.trim().to_string();
    if id.is_empty() || id.contains(char::is_whitespace) || id.contains(';') {
        return Err("Enter a pattern ID without spaces".into());
    }
    let mut values = Vec::new();
    for v in e.values.iter().map(|v| v.trim()).filter(|v| !v.is_empty()) {
        values.push(num::parse(v).ok_or_else(|| format!("\"{v}\" is not a number"))?);
    }
    if values.is_empty() {
        return Err("Enter at least one multiplier".into());
    }
    let mut pats = app.net.time_patterns();
    if pats.iter().any(|p| p.id == id) && e.original.as_deref() != Some(&id) {
        return Err(format!("A pattern named {id} already exists"));
    }
    let description = e.description.trim().to_string();
    let original = e.original.clone();
    app.checkpoint();
    if let Some(old) = &original
        && *old != id
    {
        app.net.rename_pattern(old, &id);
        pats = app.net.time_patterns();
    }
    let p = TimePattern {
        id: id.clone(),
        description,
        values,
    };
    match pats.iter_mut().find(|q| q.id == id) {
        Some(q) => *q = p,
        None => pats.push(p),
    }
    app.net.set_time_patterns(&pats);
    Ok(())
}

// ----- data curve editor ------------------------------------------------------

struct CurveEdit {
    original: Option<String>,
    id: String,
    kind: String,
    description: String,
    points: Vec<[String; 2]>,
    error: Option<String>,
}

impl CurveEdit {
    fn new(original: Option<String>, c: &DataCurve) -> Self {
        CurveEdit {
            original,
            id: c.id.clone(),
            kind: c.kind.clone(),
            description: c.description.clone(),
            points: c
                .points
                .iter()
                .map(|[x, y]| [x, y].map(|v| style::show_num(&format!("{v:.4}"))))
                .collect(),
            error: None,
        }
    }
}

fn curve_type_label(kind: &str) -> &'static str {
    match kind.to_ascii_uppercase().as_str() {
        "PUMP" => "Pump",
        "EFFICIENCY" => "Efficiency",
        "VOLUME" => "Volume",
        "HEADLOSS" => "Head Loss",
        "VALVE" => "Valve",
        _ => "Generic",
    }
}

/// Axis titles of a curve type.
fn curve_axes(kind: &str) -> (&'static str, &'static str) {
    match kind.to_ascii_uppercase().as_str() {
        "PUMP" => ("Flow", "Head"),
        "EFFICIENCY" => ("Flow", "Efficiency"),
        "VOLUME" => ("Height", "Volume"),
        "HEADLOSS" => ("Flow", "Head Loss"),
        "VALVE" => ("% Open", "% Full Flow"),
        _ => ("X", "Y"),
    }
}

/// The points to draw a curve with: for a pump curve of one or three points
/// the continuous function EPANET fits through them (head = a - b·flow^c).
fn curve_line(kind: &str, pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if !kind.eq_ignore_ascii_case("PUMP") {
        return pts.to_vec();
    }
    let three = match pts {
        [[q, h]] if *q > 0.0 && *h > 0.0 => [[0.0, 1.33334 * h], [*q, *h], [2.0 * q, 0.0]],
        [a, b, c] if a[0] == 0.0 => [*a, *b, *c],
        _ => return pts.to_vec(),
    };
    let (h0, q1, h1, q2, h2) = (
        three[0][1],
        three[1][0],
        three[1][1],
        three[2][0],
        three[2][1],
    );
    let (d1, d2) = (h0 - h1, h0 - h2);
    if q1 <= 0.0 || q2 <= q1 || d1 <= 0.0 || d2 <= d1 {
        return pts.to_vec();
    }
    let c = (d2 / d1).ln() / (q2 / q1).ln();
    let b = d1 / q1.powf(c);
    let qmax = (h0 / b).powf(1.0 / c);
    (0..=50)
        .map(|k| {
            let q = qmax * k as f64 / 50.0;
            [q, (h0 - b * q.powf(c)).max(0.0)]
        })
        .collect()
}

/// The Data Curve Editor. OK and Cancel return to the selector.
pub fn curve_editor(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    curve_editor_body(app, ui, None)
}

/// The Data Curve Editor, in a window or (`panel`: its width) in the
/// Property Editor panel, where the chart goes under the points.
fn curve_editor_body(app: &mut EpanetApp, ui: &mut Ui, panel: Option<f32>) -> bool {
    let Some(e) = &mut app.forms.curve else {
        app.dialog = Dialog::CurveSelector;
        return false;
    };
    let w = panel.unwrap_or(560.0);
    ui.set_width(w);
    // in the panel the description goes under the ID and type
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label("Curve ID");
            ui.add(egui::TextEdit::singleline(&mut e.id).desired_width(110.0));
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.label("Curve Type");
            egui::ComboBox::from_id_salt("curve_kind")
                .selected_text(curve_type_label(&e.kind))
                .width(100.0)
                .show_ui(ui, |ui| {
                    for k in ["PUMP", "EFFICIENCY", "VOLUME", "VALVE", "HEADLOSS", ""] {
                        ui.selectable_value(&mut e.kind, k.to_string(), curve_type_label(k));
                    }
                });
        });
        if panel.is_none() {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.label("Description");
                ui.add(egui::TextEdit::singleline(&mut e.description).desired_width(280.0));
            });
        }
    });
    if panel.is_some() {
        ui.label("Description");
        ui.add(egui::TextEdit::singleline(&mut e.description).desired_width(w));
    }
    ui.add_space(8.0);
    if e.points
        .last()
        .is_none_or(|p| !(p[0].trim().is_empty() && p[1].trim().is_empty()))
    {
        e.points.push([String::new(), String::new()]);
    }
    let (xt, yt) = curve_axes(&e.kind);
    let pts: Vec<[f64; 2]> = e
        .points
        .iter()
        .filter_map(|[x, y]| Some([num::parse(x)?, num::parse(y)?]))
        .collect();
    let line: PlotPoints = curve_line(&e.kind, &pts).into_iter().collect();
    let chart = |ui: &mut Ui, size: egui::Vec2| {
        Plot::new("curve_editor_chart")
            .show_crosshair(false)
            .width(size.x)
            .height(size.y)
            .x_axis_label(xt)
            .y_axis_label(yt)
            .allow_scroll(false)
            .allow_drag(false)
            .allow_zoom(false)
            .show(ui, |p| {
                p.line(
                    Line::new("curve", line)
                        .stroke(egui::Stroke::new(3.0, Color32::from_rgb(0x9f, 0xc5, 0xe8))),
                );
            });
    };
    let mut cells = |ui: &mut Ui, height: f32| {
        egui::ScrollArea::vertical()
            .id_salt("curve_cells")
            .max_height(height)
            .min_scrolled_height(height)
            .show(ui, |ui| {
                egui::Grid::new("curve_cells_grid")
                    .striped(true)
                    .spacing([2.0, 2.0])
                    .show(ui, |ui| {
                        ui.add_sized([86.0, 20.0], egui::Label::new(xt));
                        ui.add_sized([86.0, 20.0], egui::Label::new(yt));
                        ui.end_row();
                        for p in &mut e.points {
                            ui.add(egui::TextEdit::singleline(&mut p[0]).desired_width(86.0));
                            ui.add(egui::TextEdit::singleline(&mut p[1]).desired_width(86.0));
                            ui.end_row();
                        }
                    });
            });
    };
    if let Some(w) = panel {
        cells(ui, 150.0);
        ui.add_space(8.0);
        chart(ui, vec2(w, 200.0));
    } else {
        ui.horizontal_top(|ui| {
            cells(ui, 220.0);
            ui.add_space(10.0);
            chart(ui, vec2(320.0, 220.0));
        });
    }
    while e.points.len() > 1
        && e.points[e.points.len() - 1]
            .iter()
            .all(|v| v.trim().is_empty())
        && e.points[e.points.len() - 2]
            .iter()
            .all(|v| v.trim().is_empty())
    {
        e.points.pop();
    }
    if let Some(err) = &e.error {
        ui.label(RichText::new(err).color(RED));
    }
    ui.add_space(8.0);
    let clicked = form_buttons(
        ui,
        panel.is_some(),
        &["Help", "Clear"],
        &[("OK", true), ("Cancel", true)],
    );
    match clicked.as_deref() {
        Some("Help") => crate::help::open(ui.ctx(), "Data Curves"),
        Some("Clear") => e.points = vec![[String::new(), String::new()]],
        Some("OK") => match accept_curve(app) {
            Ok(()) => {
                app.forms.curve = None;
                app.dialog = Dialog::CurveSelector;
            }
            Err(err) => {
                if let Some(e) = &mut app.forms.curve {
                    e.error = Some(err);
                }
            }
        },
        Some("Cancel") => {
            app.forms.curve = None;
            app.dialog = Dialog::CurveSelector;
        }
        _ => {}
    }
    false
}

fn accept_curve(app: &mut EpanetApp) -> Result<(), String> {
    let e = app.forms.curve.as_ref().ok_or("no curve")?;
    let id = e.id.trim().to_string();
    if id.is_empty() || id.contains(char::is_whitespace) || id.contains(';') {
        return Err("Enter a curve ID without spaces".into());
    }
    let mut points = Vec::new();
    for [x, y] in &e.points {
        let (x, y) = (x.trim(), y.trim());
        if x.is_empty() && y.is_empty() {
            continue;
        }
        let number = |v: &str| num::parse(v).ok_or_else(|| format!("\"{v}\" is not a number"));
        points.push([number(x)?, number(y)?]);
    }
    if points.is_empty() {
        return Err("Enter at least one point".into());
    }
    if points.windows(2).any(|w| w[1][0] <= w[0][0]) {
        return Err("The X values must increase".into());
    }
    let mut curves = app.net.data_curves();
    if curves.iter().any(|c| c.id == id) && e.original.as_deref() != Some(&id) {
        return Err(format!("A curve named {id} already exists"));
    }
    let c = DataCurve {
        id: id.clone(),
        kind: e.kind.clone(),
        description: e.description.trim().to_string(),
        points,
    };
    let original = e.original.clone();
    app.checkpoint();
    if let Some(old) = &original
        && *old != id
    {
        app.net.rename_curve(old, &id);
        curves = app.net.data_curves();
    }
    match curves.iter_mut().find(|d| d.id == id) {
        Some(d) => *d = c,
        None => curves.push(c),
    }
    app.net.set_data_curves(&curves);
    Ok(())
}

// ----- single species quality -------------------------------------------------

struct QualityEdit {
    model: String,
    chem: String,
    units: String,
    trace: String,
    /// Values of `props::quality_specs()`.
    values: Vec<String>,
    error: Option<String>,
}

/// Opens the Single Species Water Quality Editor.
pub fn open_quality(app: &mut EpanetApp) {
    let (model, chem, units, trace) = props::quality_parts(&app.net);
    let values = props::quality_specs()
        .iter()
        .map(|s| {
            let v = app
                .net
                .option(s.section, s.key)
                .unwrap_or_else(|| s.default.to_string());
            if s.kind == PropKind::Number {
                style::show_num(&v)
            } else {
                v
            }
        })
        .collect();
    app.forms.quality = Some(QualityEdit {
        model,
        chem,
        units: if units.is_empty() {
            "mg/L".into()
        } else {
            units
        },
        trace,
        values,
        error: None,
    });
    app.dialog = Dialog::QualityEditor;
}

/// The Single Species Water Quality Editor.
pub fn quality_editor(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let msx = app.msx_active();
    let Some(e) = &mut app.forms.quality else {
        return true;
    };
    let specs = props::quality_specs();
    ui.set_width(300.0);
    if msx {
        ui.label(
            RichText::new("EPANET-MSX models the water quality: these settings are not used.")
                .weak(),
        );
        ui.add_space(4.0);
    }
    egui::Grid::new("quality_editor")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label("Type of Quality Analysis");
            egui::ComboBox::from_id_salt("q_model")
                .selected_text(&e.model)
                .width(110.0)
                .show_ui(ui, |ui| {
                    for m in ["None", "Chemical", "Age", "Trace"] {
                        ui.selectable_value(&mut e.model, m.to_string(), m);
                    }
                });
            ui.end_row();
            if e.model == "Trace" {
                ui.label("Trace Node");
                ui.add(egui::TextEdit::singleline(&mut e.trace).desired_width(110.0));
                ui.end_row();
            }
            ui.label("Constituent Name");
            ui.add_enabled(
                e.model == "Chemical",
                egui::TextEdit::singleline(&mut e.chem).desired_width(110.0),
            );
            ui.end_row();
            for (k, s) in specs.iter().enumerate() {
                if s.key == "TOLERANCE" {
                    ui.label(s.name);
                    ui.add(egui::TextEdit::singleline(&mut e.values[k]).desired_width(110.0));
                    ui.end_row();
                    ui.label("Concentration Units");
                    ui.add_enabled_ui(e.model == "Chemical", |ui| {
                        egui::ComboBox::from_id_salt("q_units")
                            .selected_text(&e.units)
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for u in ["mg/L", "ug/L"] {
                                    ui.selectable_value(&mut e.units, u.to_string(), u);
                                }
                            });
                    });
                    ui.end_row();
                    continue;
                }
                ui.label(s.name);
                match s.kind {
                    PropKind::Choice(_) => {
                        // the wall reaction order: zero or first
                        let shown = if e.values[k].trim().starts_with('0') {
                            "Zero-order"
                        } else {
                            "1st-order"
                        };
                        egui::ComboBox::from_id_salt(("q_choice", k))
                            .selected_text(shown)
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut e.values[k],
                                    "0".to_string(),
                                    "Zero-order",
                                );
                                ui.selectable_value(&mut e.values[k], "1".to_string(), "1st-order");
                            });
                    }
                    _ => {
                        ui.add(egui::TextEdit::singleline(&mut e.values[k]).desired_width(110.0));
                    }
                }
                ui.end_row();
            }
        });
    if let Some(err) = &e.error {
        ui.label(RichText::new(err).color(RED));
    }
    ui.add_space(10.0);
    match button_row(ui, &["Help"], &[("OK", true), ("Cancel", true)]).as_deref() {
        Some("Help") => crate::help::open(ui.ctx(), "Single Species Quality"),
        Some("OK") => match accept_quality(app) {
            Ok(()) => {
                app.forms.quality = None;
                return true;
            }
            Err(err) => {
                if let Some(e) = &mut app.forms.quality {
                    e.error = Some(err);
                }
            }
        },
        Some("Cancel") => {
            app.forms.quality = None;
            return true;
        }
        _ => {}
    }
    false
}

fn accept_quality(app: &mut EpanetApp) -> Result<(), String> {
    let e = app.forms.quality.as_ref().ok_or("no edit")?;
    if e.model == "Trace" && app.net.node_index(e.trace.trim()).is_none() {
        return Err(format!("There is no node \"{}\" to trace", e.trace.trim()));
    }
    let specs = props::quality_specs();
    for (s, v) in specs.iter().zip(&e.values) {
        if s.kind == PropKind::Number && !v.trim().is_empty() && !num::is_number(v) {
            return Err(format!("{}: \"{}\" is not a number", s.name, v.trim()));
        }
    }
    let (model, chem, units, trace) = (
        e.model.clone(),
        e.chem.trim().to_string(),
        e.units.clone(),
        e.trace.trim().to_string(),
    );
    let values = e.values.clone();
    let mut net = app.net.clone();
    props::set_quality_parts(&mut net, &model, &chem, &units, &trace);
    for (s, v) in specs.iter().zip(&values) {
        let current = net
            .option(s.section, s.key)
            .unwrap_or_else(|| s.default.into());
        let v = if s.kind == PropKind::Number {
            num::normalize(v)
        } else {
            v.trim().to_string()
        };
        if v != current {
            net.set_option(s.section, s.key, &v);
        }
    }
    if net != app.net {
        app.checkpoint();
        app.net = net;
    }
    Ok(())
}

// ----- simple controls ----------------------------------------------------------

/// A simple control being built in the Simple Controls Editor's lower panel.
#[derive(Clone, Debug, PartialEq)]
struct ControlForm {
    link: String,
    /// OPEN, CLOSED or a setting.
    action: String,
    /// IF NODE (else AT).
    on_node: bool,
    node: String,
    above: bool,
    level: String,
    /// AT CLOCKTIME (else AT TIME).
    clock: bool,
    time: String,
}

impl Default for ControlForm {
    fn default() -> Self {
        ControlForm {
            link: String::new(),
            action: "OPEN".into(),
            on_node: true,
            node: String::new(),
            above: false,
            level: String::new(),
            clock: false,
            time: String::new(),
        }
    }
}

impl ControlForm {
    /// Reads `LINK id action IF NODE id ABOVE|BELOW value` or `LINK id
    /// action AT TIME|CLOCKTIME value [AM|PM]`.
    fn parse(statement: &str) -> Option<ControlForm> {
        let t: Vec<&str> = statement.split_whitespace().collect();
        let up = |k: usize| t.get(k).map(|s| s.to_ascii_uppercase()).unwrap_or_default();
        if up(0) != "LINK" || t.len() < 5 {
            return None;
        }
        let mut f = ControlForm {
            link: t[1].to_string(),
            action: t[2].to_string(),
            ..Default::default()
        };
        match (up(3).as_str(), up(4).as_str()) {
            ("IF", "NODE") if t.len() >= 8 => {
                f.node = t[5].to_string();
                f.above = up(6) == "ABOVE";
                f.level = style::show_num(t[7]);
            }
            ("AT", "TIME") | ("AT", "CLOCKTIME") if t.len() >= 6 => {
                f.on_node = false;
                f.clock = up(4) == "CLOCKTIME";
                f.time = t[5..].join(" ");
            }
            _ => return None,
        }
        Some(f)
    }

    fn statement(&self) -> String {
        let head = format!(
            "LINK {} {}",
            self.link.trim(),
            self.action.trim().to_ascii_uppercase()
        );
        if self.on_node {
            format!(
                "{head} IF NODE {} {} {}",
                self.node.trim(),
                if self.above { "ABOVE" } else { "BELOW" },
                num::normalize(&self.level)
            )
        } else {
            format!(
                "{head} AT {} {}",
                if self.clock { "CLOCKTIME" } else { "TIME" },
                self.time.trim()
            )
        }
    }
}

struct ControlsEdit {
    /// (enabled, statement)
    rows: Vec<(bool, String)>,
    selected: Option<usize>,
    /// The control being inserted (None) or edited (Some row), and its form.
    form: Option<(Option<usize>, ControlForm)>,
    original: Vec<(bool, String)>,
    error: Option<String>,
}

/// The controls of the network: enabled ones, and those written with
/// EPANET 2.3's trailing `DISABLED`.
fn read_controls(app: &EpanetApp) -> Vec<(bool, String)> {
    app.net
        .raw_lines("CONTROLS")
        .map(|t| {
            let off = t.last().is_some_and(|w| w.eq_ignore_ascii_case("DISABLED"));
            let words = if off { &t[..t.len() - 1] } else { &t[..] };
            (!off, words.join(" "))
        })
        .collect()
}

/// Starts editing the network's controls.
fn start_controls(app: &mut EpanetApp) {
    let rows = read_controls(app);
    app.forms.controls = Some(ControlsEdit {
        original: rows.clone(),
        selected: (!rows.is_empty()).then_some(0),
        rows,
        form: None,
        error: None,
    });
}

/// The Simple Controls Editor inside the Property Editor panel. An editor
/// with no pending edits follows the network (undo, a new project).
pub fn controls_panel(app: &mut EpanetApp, ui: &mut Ui) {
    let stale = app.forms.controls.as_ref().is_none_or(|e| {
        e.rows == e.original && e.form.is_none() && e.original != read_controls(app)
    });
    if stale {
        start_controls(app);
    }
    let w = ui.available_width() - 20.0;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| controls_body(app, ui, w));
}

/// The ID of the node or link selected on the map, for the + buttons.
fn selected_id(app: &EpanetApp, node: bool) -> Option<String> {
    match (app.selection?, node) {
        (Element::Node(i), true) => app.net.nodes.get(i).map(|n| n.id.clone()),
        (Element::Link(i), false) => app.net.links.get(i).map(|l| l.id.clone()),
        _ => None,
    }
}

/// The Simple Controls Editor, `w` wide.
fn controls_body(app: &mut EpanetApp, ui: &mut Ui, w: f32) -> bool {
    let sel_node = selected_id(app, true);
    let sel_link = selected_id(app, false);
    let mono = egui::FontId::monospace(13.0);
    let Some(e) = &mut app.forms.controls else {
        return true;
    };
    ui.set_width(w);
    egui::Frame::NONE
        .fill(if ui.visuals().dark_mode {
            ui.visuals().extreme_bg_color
        } else {
            Color32::WHITE
        })
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(160)))
        .show(ui, |ui| {
            ui.set_min_size(vec2(w, 200.0));
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("controls_list")
                        .num_columns(2)
                        .spacing([6.0, 2.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Use").font(mono.clone()));
                            ui.label(RichText::new("Control Statement").font(mono.clone()));
                            ui.end_row();
                            for (k, (on, s)) in e.rows.iter_mut().enumerate() {
                                ui.checkbox(on, "");
                                if cell(
                                    ui,
                                    w - 50.0,
                                    e.selected == Some(k),
                                    RichText::new(s.as_str()).font(mono.clone()),
                                )
                                .clicked()
                                {
                                    e.selected = Some(k);
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    let editing = e.form.is_some();
    let has = e.selected.is_some_and(|k| k < e.rows.len());
    ui.horizontal_wrapped(|ui| {
        let b = |ui: &mut Ui, on: bool, t: &str| {
            let min = if t.chars().count() == 1 { 32.0 } else { 76.0 };
            ui.add_enabled(
                on,
                egui::Button::new(crate::i18n::tr(t)).min_size(vec2(min, 26.0)),
            )
            .clicked()
        };
        if b(ui, !editing, "Insert") {
            e.form = Some((None, ControlForm::default()));
        }
        if b(ui, !editing && has, "Edit")
            && let Some(k) = e.selected
        {
            let f = ControlForm::parse(&e.rows[k].1).unwrap_or_default();
            e.form = Some((Some(k), f));
        }
        if b(ui, !editing && has, "Delete")
            && let Some(k) = e.selected
        {
            e.rows.remove(k);
            e.selected = (!e.rows.is_empty()).then(|| k.min(e.rows.len() - 1));
        }
        if b(ui, !editing && has && e.selected > Some(0), "⬆")
            && let Some(k) = e.selected
        {
            e.rows.swap(k, k - 1);
            e.selected = Some(k - 1);
        }
        if b(
            ui,
            !editing && has && e.selected.is_some_and(|k| k + 1 < e.rows.len()),
            "⬇",
        ) && let Some(k) = e.selected
        {
            e.rows.swap(k, k + 1);
            e.selected = Some(k + 1);
        }
    });
    // the lower panel: the control being inserted or edited
    // a narrow panel stacks the form: LINK / action / IF-AT / NODE / level
    let narrow = w < 400.0;
    let (fw, cw) = if narrow { (64.0, 80.0) } else { (80.0, 90.0) };
    let mut done: Option<bool> = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(vec2(w - 10.0, 110.0));
        let Some((_, f)) = &mut e.form else {
            let (rect, _) = ui.allocate_exact_size(vec2(w - 10.0, 110.0), egui::Sense::hover());
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Control Editor",
                egui::FontId::proportional(18.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        egui::Grid::new("control_form")
            .num_columns(4)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                ui.label("LINK");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut f.link).desired_width(fw));
                    if ui
                        .add_enabled(sel_link.is_some(), egui::Button::new("+"))
                        .on_hover_text("The link selected on the map")
                        .clicked()
                        && let Some(id) = &sel_link
                    {
                        f.link = id.clone();
                    }
                });
                if narrow {
                    ui.end_row();
                    ui.label("");
                }
                let shown = f.action.to_ascii_uppercase();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("ctl_action")
                        .selected_text(if shown == "OPEN" || shown == "CLOSED" {
                            shown.clone()
                        } else {
                            "SETTING".into()
                        })
                        .width(cw)
                        .show_ui(ui, |ui| {
                            for a in ["OPEN", "CLOSED"] {
                                if ui.selectable_label(shown == a, a).clicked() {
                                    f.action = a.into();
                                }
                            }
                            if ui
                                .selectable_label(shown != "OPEN" && shown != "CLOSED", "SETTING")
                                .clicked()
                                && (shown == "OPEN" || shown == "CLOSED")
                            {
                                f.action = "0".into();
                            }
                        });
                    if shown != "OPEN" && shown != "CLOSED" {
                        ui.add(egui::TextEdit::singleline(&mut f.action).desired_width(fw * 0.75));
                    }
                });
                if narrow {
                    ui.end_row();
                    ui.label("");
                }
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.on_node, true, "IF");
                    ui.radio_value(&mut f.on_node, false, "AT");
                });
                ui.end_row();
                if f.on_node {
                    ui.label("NODE");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut f.node).desired_width(fw));
                        if ui
                            .add_enabled(sel_node.is_some(), egui::Button::new("+"))
                            .on_hover_text("The node selected on the map")
                            .clicked()
                            && let Some(id) = &sel_node
                        {
                            f.node = id.clone();
                        }
                    });
                    if narrow {
                        ui.end_row();
                        ui.label("");
                    }
                    egui::ComboBox::from_id_salt("ctl_above")
                        .selected_text(if f.above { "ABOVE" } else { "BELOW" })
                        .width(cw)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.above, true, "ABOVE");
                            ui.selectable_value(&mut f.above, false, "BELOW");
                        });
                    ui.add(egui::TextEdit::singleline(&mut f.level).desired_width(fw));
                } else {
                    egui::ComboBox::from_id_salt("ctl_time")
                        .selected_text(if f.clock { "CLOCKTIME" } else { "TIME" })
                        .width(cw)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.clock, false, "TIME");
                            ui.selectable_value(&mut f.clock, true, "CLOCKTIME");
                        });
                    ui.add(egui::TextEdit::singleline(&mut f.time).desired_width(fw))
                        .on_hover_text(if f.clock {
                            "Time of day, e.g. 6:00 or 6:00 PM"
                        } else {
                            "Hours into the simulation, e.g. 12 or 12:30"
                        });
                }
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(((w - 180.0) / 2.0).clamp(0.0, 110.0));
            if ui
                .add_sized([80.0, 24.0], egui::Button::new("Accept"))
                .clicked()
            {
                done = Some(true);
            }
            if ui
                .add_sized([80.0, 24.0], egui::Button::new("Cancel"))
                .clicked()
            {
                done = Some(false);
            }
        });
    });
    match done {
        Some(true) => {
            if let Some((at, f)) = e.form.clone() {
                let err =
                    if f.link.trim().is_empty() || !app.net.link_index(f.link.trim()).is_some() {
                        Some(format!("There is no link \"{}\"", f.link.trim()))
                    } else if f.on_node && app.net.node_index(f.node.trim()).is_none() {
                        Some(format!("There is no node \"{}\"", f.node.trim()))
                    } else if f.on_node && !num::is_number(&f.level) {
                        Some("Enter a number for the node's level".into())
                    } else if !f.on_node
                        && eui_core::analysis::parse_time(
                            f.time
                                .trim()
                                .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                                .trim(),
                        )
                        .is_none()
                    {
                        Some("Enter a time, e.g. 6:00".into())
                    } else {
                        None
                    };
                let e = app.forms.controls.as_mut().expect("the controls edit");
                e.error = err;
                if e.error.is_none() {
                    let s = f.statement();
                    match at {
                        Some(k) => e.rows[k].1 = s,
                        None => {
                            let k = e.selected.map_or(e.rows.len(), |k| k + 1);
                            e.rows.insert(k, (true, s));
                            e.selected = Some(k);
                        }
                    }
                    e.form = None;
                }
            }
        }
        Some(false) => {
            e.form = None;
            e.error = None;
        }
        None => {}
    }
    let Some(e) = &mut app.forms.controls else {
        return true;
    };
    if let Some(err) = &e.error {
        ui.label(RichText::new(err).color(RED));
    }
    ui.add_space(8.0);
    let changed = e.rows != e.original && e.form.is_none();
    match button_row(ui, &["Help"], &[("OK", changed), ("Cancel", true)]).as_deref() {
        Some("Help") => crate::help::open(ui.ctx(), "Simple Controls Editor"),
        Some("OK") => {
            let lines: Vec<String> = e
                .rows
                .iter()
                .map(|(on, s)| format!(" {s}{}", if *on { "" } else { "  DISABLED" }))
                .collect();
            app.checkpoint();
            app.net.replace_section("CONTROLS", lines);
            app.forms.controls = None;
            return true;
        }
        Some("Cancel") => {
            app.forms.controls = None;
            return true;
        }
        _ => {}
    }
    false
}

// ----- rule-based controls --------------------------------------------------------

struct RulesEdit {
    /// (enabled, rule ID, text of the rule without DISABLED)
    rows: Vec<(bool, String, String)>,
    selected: Option<usize>,
    /// The text being edited, and the row it replaces (None: a new rule).
    edit: Option<(Option<usize>, String)>,
    original: Vec<(bool, String, String)>,
    error: Option<String>,
}

/// The rules of the network, split at each RULE line.
fn read_rules(app: &EpanetApp) -> Vec<(bool, String, String)> {
    let text = app.net.section_text("RULES");
    let mut out: Vec<(bool, String, String)> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        let first = t
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if first == "RULE" {
            let id = t.split_whitespace().nth(1).unwrap_or("").to_string();
            out.push((true, id, format!("RULE {}", t[4..].trim())));
        } else if let Some(r) = out.last_mut() {
            if first == "DISABLED" {
                r.0 = false;
            } else if !t.is_empty() {
                r.2.push('\n');
                r.2.push_str(t);
            }
        }
    }
    out
}

/// Starts editing the network's rules.
fn start_rules(app: &mut EpanetApp) {
    let rows = read_rules(app);
    app.forms.rules = Some(RulesEdit {
        original: rows.clone(),
        selected: (!rows.is_empty()).then_some(0),
        rows,
        edit: None,
        error: None,
    });
}

/// The Rule-Based Controls Editor inside the Property Editor panel. An
/// editor with no pending edits follows the network (undo, a new project).
pub fn rules_panel(app: &mut EpanetApp, ui: &mut Ui) {
    let stale =
        app.forms.rules.as_ref().is_none_or(|e| {
            e.rows == e.original && e.edit.is_none() && e.original != read_rules(app)
        });
    if stale {
        start_rules(app);
    }
    let w = ui.available_width() - 20.0;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| rules_body(app, ui, w));
}

/// Checks a rule's text: a RULE line, then IF and THEN clauses.
fn check_rule(text: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let first: Vec<&str> = lines
        .first()
        .map(|l| l.split_whitespace().collect())
        .unwrap_or_default();
    if first.len() < 2 || !first[0].eq_ignore_ascii_case("RULE") {
        return Err("A rule starts with RULE and its ID".into());
    }
    let has = |kw: &str| {
        lines.iter().any(|l| {
            l.split_whitespace()
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case(kw))
        })
    };
    if !has("IF") || !has("THEN") {
        return Err("A rule needs an IF and a THEN clause".into());
    }
    Ok(first[1].to_string())
}

/// The Rule-Based Controls Editor, `w` wide.
fn rules_body(app: &mut EpanetApp, ui: &mut Ui, w: f32) -> bool {
    let mono = egui::FontId::monospace(13.0);
    let Some(e) = &mut app.forms.rules else {
        return true;
    };
    ui.set_width(w);
    egui::Frame::NONE
        .fill(if ui.visuals().dark_mode {
            ui.visuals().extreme_bg_color
        } else {
            Color32::WHITE
        })
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(160)))
        .show(ui, |ui| {
            ui.set_min_size(vec2(w, 220.0));
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("rules_list")
                        .num_columns(2)
                        .spacing([6.0, 2.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Use").font(mono.clone()));
                            ui.label(RichText::new("Rule ID").font(mono.clone()));
                            ui.end_row();
                            for (k, (on, id, _)) in e.rows.iter_mut().enumerate() {
                                ui.checkbox(on, "");
                                if cell(
                                    ui,
                                    w - 50.0,
                                    e.selected == Some(k),
                                    RichText::new(id.as_str()).font(mono.clone()),
                                )
                                .clicked()
                                    && e.edit.is_none()
                                {
                                    e.selected = Some(k);
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    let editing = e.edit.is_some();
    let has = e.selected.is_some_and(|k| k < e.rows.len());
    ui.horizontal_wrapped(|ui| {
        let b = |ui: &mut Ui, on: bool, t: &str| {
            let min = if t.chars().count() == 1 { 32.0 } else { 76.0 };
            ui.add_enabled(
                on,
                egui::Button::new(crate::i18n::tr(t)).min_size(vec2(min, 26.0)),
            )
            .clicked()
        };
        if b(ui, !editing, "Insert") {
            e.edit = Some((None, "RULE \nIF   \nTHEN ".into()));
        }
        if b(ui, !editing && has, "Edit")
            && let Some(k) = e.selected
        {
            e.edit = Some((Some(k), e.rows[k].2.clone()));
        }
        if b(ui, !editing && has, "Delete")
            && let Some(k) = e.selected
        {
            e.rows.remove(k);
            e.selected = (!e.rows.is_empty()).then(|| k.min(e.rows.len() - 1));
        }
        if b(ui, !editing && has && e.selected > Some(0), "⬆")
            && let Some(k) = e.selected
        {
            e.rows.swap(k, k - 1);
            e.selected = Some(k - 1);
        }
        if b(
            ui,
            !editing && has && e.selected.is_some_and(|k| k + 1 < e.rows.len()),
            "⬇",
        ) && let Some(k) = e.selected
        {
            e.rows.swap(k, k + 1);
            e.selected = Some(k + 1);
        }
    });
    // the rule's text: shown, or edited after Insert / Edit
    let shown = e
        .selected
        .and_then(|k| e.rows.get(k))
        .map(|r| r.2.clone())
        .unwrap_or_default();
    let bg = if ui.visuals().dark_mode {
        Color32::from_rgb(0x3a, 0x38, 0x2a)
    } else {
        crate::style::HINT_BG
    };
    egui::Frame::NONE
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(160)))
        .inner_margin(4)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("rule_text")
                .max_height(130.0)
                .min_scrolled_height(130.0)
                .show(ui, |ui| match &mut e.edit {
                    Some((_, text)) => {
                        ui.add(
                            egui::TextEdit::multiline(text)
                                .font(mono.clone())
                                .desired_rows(7)
                                .desired_width(f32::INFINITY),
                        );
                    }
                    None => {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new(shown).font(mono.clone()));
                    }
                });
        });
    let mut accept = false;
    let mut cancel = false;
    ui.horizontal_wrapped(|ui| {
        ui.add_space(((w - 340.0) / 2.0).clamp(0.0, 40.0));
        if ui
            .add_sized([100.0, 26.0], egui::Button::new("? Rule Format"))
            .clicked()
        {
            crate::help::open(ui.ctx(), "Rule-Based Controls");
        }
        accept = ui
            .add_enabled(
                editing,
                egui::Button::new("Accept Edits").min_size(vec2(100.0, 26.0)),
            )
            .clicked();
        cancel = ui
            .add_enabled(
                editing,
                egui::Button::new("Cancel Edits").min_size(vec2(100.0, 26.0)),
            )
            .clicked();
    });
    if accept && let Some((at, text)) = e.edit.clone() {
        match check_rule(&text) {
            Ok(id) => {
                let clean: Vec<&str> = text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect();
                let row = (true, id, clean.join("\n"));
                match at {
                    Some(k) => {
                        let on = e.rows[k].0;
                        e.rows[k] = (on, row.1, row.2);
                    }
                    None => {
                        let k = e.selected.map_or(e.rows.len(), |k| k + 1);
                        e.rows.insert(k, row);
                        e.selected = Some(k);
                    }
                }
                e.edit = None;
                e.error = None;
            }
            Err(err) => e.error = Some(err),
        }
    } else if cancel {
        e.edit = None;
        e.error = None;
    }
    if let Some(err) = &e.error {
        ui.label(RichText::new(err).color(RED));
    }
    ui.add_space(8.0);
    let changed = e.rows != e.original && e.edit.is_none();
    match button_row(ui, &["Help"], &[("OK", changed), ("Cancel", true)]).as_deref() {
        Some("Help") => crate::help::open(ui.ctx(), "Rule-Based Controls Editor"),
        Some("OK") => {
            let mut lines = Vec::new();
            for (on, _, text) in &e.rows {
                lines.extend(text.lines().map(str::to_string));
                if !on {
                    lines.push("DISABLED".into());
                }
                lines.push(String::new());
            }
            app.checkpoint();
            app.net.replace_section("RULES", lines);
            app.forms.rules = None;
            return true;
        }
        Some("Cancel") => {
            app.forms.rules = None;
            return true;
        }
        _ => {}
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> EpanetApp {
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        app.load_example("Net1", eui_core::examples::NET1);
        app
    }

    #[test]
    fn control_statements_round_trip() {
        for s in [
            "LINK 9 OPEN IF NODE 2 BELOW 110",
            "LINK 10 CLOSED AT CLOCKTIME 20:00",
            "LINK 12 1.5 AT TIME 6",
        ] {
            assert_eq!(ControlForm::parse(s).unwrap().statement(), s);
        }
        assert!(ControlForm::parse("NODE 1 OPEN").is_none());
    }

    #[test]
    fn disabled_controls_and_rules_are_kept() {
        let mut app = app();
        start_controls(&mut app);
        let e = app.forms.controls.as_mut().unwrap();
        assert_eq!(e.rows.len(), 2);
        assert!(e.rows[0].1.starts_with("LINK 9 OPEN IF NODE 2 BELOW"));
        e.rows[1].0 = false;
        let lines: Vec<String> = e
            .rows
            .iter()
            .map(|(on, s)| format!(" {s}{}", if *on { "" } else { "  DISABLED" }))
            .collect();
        app.net.replace_section("CONTROLS", lines);
        let rows = read_controls(&app);
        assert!(rows[0].0 && !rows[1].0);
        // EPANET 2.3 reads it
        let out = eui_engine::run_blocking(&app.net.to_inp()).unwrap();
        assert!(!out.is_error(), "{}", out.report);

        let rule = "RULE R1\nIF TANK 2 LEVEL ABOVE 140\nTHEN PUMP 9 STATUS IS CLOSED";
        assert_eq!(check_rule(rule).unwrap(), "R1");
        assert!(check_rule("RULE R2\nTHEN PUMP 9 STATUS IS OPEN").is_err());
        app.net.replace_section(
            "RULES",
            rule.lines()
                .map(str::to_string)
                .chain(["DISABLED".to_string()])
                .collect(),
        );
        let rules = read_rules(&app);
        assert_eq!(rules.len(), 1);
        assert!(!rules[0].0);
        assert_eq!(rules[0].1, "R1");
        let out = eui_engine::run_blocking(&app.net.to_inp()).unwrap();
        assert!(!out.is_error(), "{}", out.report);
    }

    #[test]
    fn patterns_are_picked_edited_and_kept_when_in_use() {
        let mut app = app();
        let j = app.net.node_index("10").unwrap();
        // pick a pattern for a junction
        open_patterns(&mut app, Pick::Prop(Element::Node(j), PropKey::Field(2)));
        assert_eq!(app.dialog, Dialog::PatternSelector);
        app.forms.selected = 1; // pattern 1 (row 0 is <blank>)
        assign(&mut app, "1");
        assert_eq!(app.net.get_prop(Element::Node(j), PropKey::Field(2)), "1");
        // a new pattern from the editor
        app.forms.pattern = Some(PatternEdit::new(
            None,
            "P2".into(),
            "Night".into(),
            vec![0.5, 1.5],
        ));
        accept_pattern(&mut app).unwrap();
        let p = app.net.time_patterns();
        assert_eq!(p[1].id, "P2");
        assert_eq!(p[1].description, "Night");
        // renaming follows its users
        app.forms.pattern = Some(PatternEdit::new(
            Some("1".into()),
            "DAY".into(),
            "Day".into(),
            vec![1.0],
        ));
        accept_pattern(&mut app).unwrap();
        assert_eq!(app.net.get_prop(Element::Node(j), PropKey::Field(2)), "DAY");
        assert!(!app.net.pattern_users("DAY").is_empty());
        // a duplicate ID is refused
        app.forms.pattern = Some(PatternEdit::new(
            None,
            "P2".into(),
            String::new(),
            vec![1.0],
        ));
        assert!(accept_pattern(&mut app).is_err());
    }

    #[test]
    fn pump_curves_are_fitted() {
        let line = curve_line("PUMP", &[[1500.0, 250.0]]);
        assert_eq!(line[0], [0.0, 1.33334 * 250.0]);
        let q = line.iter().find(|p| (p[0] - 1500.0).abs() < 31.0).unwrap();
        assert!((q[1] - 250.0).abs() < 10.0, "{q:?}");
        assert!(line.last().unwrap()[1].abs() < 1e-6);
        assert_eq!(curve_line("VOLUME", &[[1.0, 2.0]]), [[1.0, 2.0]]);
    }

    #[test]
    fn single_species_quality_editor_sets_the_options() {
        let mut app = app();
        open_quality(&mut app);
        let e = app.forms.quality.as_mut().unwrap();
        assert_eq!(e.model, "Chemical");
        assert_eq!(e.chem, "Chlorine");
        e.model = "Age".into();
        accept_quality(&mut app).unwrap();
        assert_eq!(props::quality_parts(&app.net).0, "Age");
        open_quality(&mut app);
        let e = app.forms.quality.as_mut().unwrap();
        e.model = "Trace".into();
        e.trace = "nope".into();
        assert!(accept_quality(&mut app).is_err());
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;

    /// The forms are about EPANET-UI's size.
    #[test]
    fn forms_have_epanet_ui_sizes() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.load_example("Net1", eui_core::examples::NET1);
        let mut sizes = Vec::new();
        open_patterns(&mut app, Pick::Browse);
        sizes.push(("patterns", crate::msx::tests::dialog_size(&mut app, &ctx)));
        app.forms.pattern = Some(PatternEdit::new(
            Some("1".into()),
            "1".into(),
            String::new(),
            DIURNAL.to_vec(),
        ));
        app.dialog = Dialog::PatternEditor;
        sizes.push(("pattern", crate::msx::tests::dialog_size(&mut app, &ctx)));
        open_curves(&mut app, Pick::Browse);
        sizes.push(("curves", crate::msx::tests::dialog_size(&mut app, &ctx)));
        app.forms.curve = Some(CurveEdit::new(Some("1".into()), &app.net.data_curves()[0]));
        app.dialog = Dialog::CurveEditor;
        sizes.push(("curve", crate::msx::tests::dialog_size(&mut app, &ctx)));
        open_quality(&mut app);
        sizes.push(("quality", crate::msx::tests::dialog_size(&mut app, &ctx)));
        for (name, s) in &sizes {
            assert!(s.x < 700.0 && s.y < 640.0, "{name} {s:?} — {sizes:?}");
        }
        println!("{sizes:?}");
    }

    /// The controls editors fit the Property Editor panel, at its narrowest
    /// too, with no editor button left.
    #[test]
    fn the_controls_editors_fit_the_panel() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.load_example("Net1", eui_core::examples::NET1);
        app.net.replace_section(
            "RULES",
            [
                "RULE R1",
                "IF TANK 2 LEVEL ABOVE 140",
                "THEN PUMP 9 STATUS IS CLOSED",
            ]
            .map(str::to_string)
            .to_vec(),
        );
        for width in [260.0, 500.0] {
            for (rules, editing) in [(false, false), (false, true), (true, false), (true, true)] {
                let input = || egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(width + 600.0, 1200.0),
                    )),
                    ..Default::default()
                };
                let mut right = 0.0f32;
                let mut texts = Vec::new();
                for _ in 0..3 {
                    let mut used = 0.0f32;
                    let mut out = ctx.run_ui(input(), |ui| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| {
                                used = ui
                                    .allocate_ui(vec2(width, 1200.0), |ui| {
                                        if rules {
                                            rules_panel(&mut app, ui)
                                        } else {
                                            controls_panel(&mut app, ui)
                                        }
                                    })
                                    .response
                                    .rect
                                    .right();
                            });
                    });
                    out.textures_delta.clear();
                    right = used;
                    texts.clear();
                    for c in &out.shapes {
                        if let egui::Shape::Text(t) = &c.shape {
                            texts.push(t.galley.text().to_string());
                        }
                    }
                    if editing {
                        if rules {
                            let e = app.forms.rules.as_mut().unwrap();
                            e.edit.get_or_insert((Some(0), e.rows[0].2.clone()));
                        } else {
                            let e = app.forms.controls.as_mut().unwrap();
                            let f = ControlForm::parse(&e.rows[0].1).unwrap();
                            e.form.get_or_insert((Some(0), f));
                        }
                    }
                }
                let what = format!("width {width} rules {rules} editing {editing}");
                assert!(right <= width + 0.5, "{what}: {right}");
                assert!(texts.iter().any(|t| t == "Insert"), "{what}: {texts:?}");
                assert!(!texts.iter().any(|t| t.contains("Editor…")), "{what}");
                if editing && !rules {
                    assert!(texts.iter().any(|t| t == "NODE"), "{what}: {texts:?}");
                }
                app.forms.controls = None;
                app.forms.rules = None;
            }
        }
    }

    /// The pattern and curve selectors and editors fit the panel, which has
    /// its own state: a selector window is left as it was.
    #[test]
    fn the_pattern_and_curve_forms_fit_the_panel() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.load_example("Net1", eui_core::examples::NET1);
        app.net.set_data_curves(&[DataCurve {
            id: "1".into(),
            kind: "PUMP".into(),
            points: vec![[1500.0, 250.0]],
            ..Default::default()
        }]);
        for width in [260.0, 500.0] {
            for (curves, editing) in [(false, false), (false, true), (true, false), (true, true)] {
                let input = || egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(width + 600.0, 1200.0),
                    )),
                    ..Default::default()
                };
                let mut right = 0.0f32;
                let mut texts = Vec::new();
                for frame in 0..3 {
                    if frame == 1 && editing {
                        // Edit, as the panel's button does
                        let p = &mut app.forms.panel;
                        if curves {
                            p.curve =
                                Some(CurveEdit::new(Some("1".into()), &app.net.data_curves()[0]));
                        } else {
                            let pat = &app.net.time_patterns()[0];
                            p.pattern = Some(PatternEdit::new(
                                Some(pat.id.clone()),
                                pat.id.clone(),
                                String::new(),
                                pat.values.clone(),
                            ));
                        }
                    }
                    let mut used = 0.0f32;
                    let mut out = ctx.run_ui(input(), |ui| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| {
                                used = ui
                                    .allocate_ui(vec2(width, 1200.0), |ui| {
                                        if curves {
                                            curves_panel(&mut app, ui)
                                        } else {
                                            patterns_panel(&mut app, ui)
                                        }
                                    })
                                    .response
                                    .rect
                                    .right();
                            });
                    });
                    out.textures_delta.clear();
                    right = used;
                    texts.clear();
                    for c in &out.shapes {
                        if let egui::Shape::Text(t) = &c.shape {
                            texts.push(t.galley.text().to_string());
                        }
                    }
                }
                let what = format!("width {width} curves {curves} editing {editing}");
                assert!(right <= width + 0.5, "{what}: {right}");
                let wanted = if editing { "Cancel" } else { "Add" };
                assert!(texts.iter().any(|t| t == wanted), "{what}: {texts:?}");
                assert!(!texts.iter().any(|t| t.contains("Selector…")), "{what}");
                assert_eq!(app.dialog, Dialog::None, "{what}");
                assert!(app.forms.pattern.is_none() && app.forms.curve.is_none());
                app.forms.panel = PanelForms::default();
            }
        }
    }
}
