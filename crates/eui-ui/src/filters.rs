//! Filters for the Node and Link Results tables (EPANET-UI's `networkrpt`
//! Filter page): the columns to show, and up to five conditions that the
//! rows shown must all meet.

use std::cmp::Ordering;

use egui::{Color32, Margin, Stroke, Ui, pos2, vec2};

use crate::i18n::{tr, trf};
use crate::reports::Rows;

/// The most conditions a table can have (as in EPANET-UI).
const MAX_CONDITIONS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Below,
    Equal,
    Above,
}

impl Relation {
    const ALL: [Relation; 3] = [Relation::Below, Relation::Equal, Relation::Above];

    fn label(self) -> &'static str {
        match self {
            Relation::Below => "Below",
            Relation::Equal => "Equal To",
            Relation::Above => "Above",
        }
    }
}

/// A row condition: a column (by name), a relation and the value as typed.
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub column: String,
    pub relation: Relation,
    pub value: String,
}

impl Condition {
    fn text(&self) -> String {
        format!(
            "{} {} {}",
            tr(&self.column),
            tr(self.relation.label()),
            self.value
        )
    }

    /// Does a row's cell in this condition's column meet it? Numbers are
    /// compared as EPANET-UI does (flows by size, equal within 0.001),
    /// anything else (Type, Status) as text, ignoring case.
    fn met_by(&self, cell: &str) -> bool {
        let ord = match (
            eui_core::num::parse(cell),
            eui_core::num::parse(&self.value),
        ) {
            (Some(x), Some(v)) => {
                let x = if self.column.starts_with("Flow") {
                    x.abs()
                } else {
                    x
                };
                if (x - v).abs() < 0.001 {
                    Ordering::Equal
                } else {
                    x.total_cmp(&v)
                }
            }
            _ => cell
                .trim()
                .to_lowercase()
                .cmp(&self.value.trim().to_lowercase()),
        };
        match self.relation {
            Relation::Below => ord.is_lt(),
            Relation::Equal => ord.is_eq(),
            Relation::Above => ord.is_gt(),
        }
    }
}

/// A column's name: the first line of its heading.
fn name(heading: &str) -> &str {
    heading
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// Columns compared as text: their values need not be numbers.
fn text_column(column: &str) -> bool {
    matches!(column, "Type" | "Status")
}

#[derive(Default)]
pub struct TableFilter {
    /// The columns not shown, by name.
    pub hidden: Vec<String>,
    pub conditions: Vec<Condition>,
    /// The Set Filters page, while it is open.
    page: Option<Page>,
}

/// The Set Filters page's choices, which Accept applies.
struct Page {
    hidden: Vec<String>,
    conditions: Vec<Condition>,
    /// The condition being entered: column (index among the page's
    /// columns), relation and value.
    column: usize,
    relation: Relation,
    value: String,
    picked: Option<usize>,
}

impl TableFilter {
    /// A filter with these columns hidden at first.
    pub fn hiding(columns: &[&str]) -> Self {
        TableFilter {
            hidden: columns.iter().map(|c| c.to_string()).collect(),
            ..Default::default()
        }
    }

    pub fn is_open(&self) -> bool {
        self.page.is_some()
    }

    /// Whether any conditions limit the rows.
    pub fn active(&self) -> bool {
        !self.conditions.is_empty()
    }

    /// Opens the Set Filters page with the current choices.
    pub fn open(&mut self) {
        self.page = Some(Page {
            hidden: self.hidden.clone(),
            conditions: self.conditions.clone(),
            column: 0,
            relation: Relation::Below,
            value: String::new(),
            picked: None,
        });
    }

    /// The rows that meet every condition, without the hidden columns (the
    /// first column, the ID, always shows).
    pub fn apply(&self, rows: &Rows) -> Rows {
        let Some(head) = rows.first() else {
            return Vec::new();
        };
        let names: Vec<&str> = head.iter().map(|h| name(h)).collect();
        let conditions: Vec<(usize, &Condition)> = self
            .conditions
            .iter()
            .filter_map(|c| names.iter().position(|n| *n == c.column).map(|k| (k, c)))
            .collect();
        let shown: Vec<usize> = (0..head.len())
            .filter(|&k| k == 0 || !self.hidden.iter().any(|h| h == names[k]))
            .collect();
        rows.iter()
            .enumerate()
            .filter(|(i, row)| {
                *i == 0
                    || conditions
                        .iter()
                        .all(|(k, c)| c.met_by(row.get(*k).map_or("", String::as_str)))
            })
            .map(|(_, row)| {
                shown
                    .iter()
                    .map(|&k| row.get(k).cloned().unwrap_or_default())
                    .collect()
            })
            .collect()
    }

    /// Draws the Set Filters page for a table with these headings, laid
    /// out as EPANET-UI's: the columns' check boxes on the left, the row
    /// conditions on the right, Accept and Cancel under them. True when
    /// Accept changed which columns show (the table's sort then resets).
    pub fn page(&mut self, ui: &mut Ui, head: &[String], status: &mut String) -> bool {
        let Some(p) = self.page.as_mut() else {
            return false;
        };
        let names: Vec<String> = head.iter().skip(1).map(|h| name(h).to_string()).collect();
        p.column = p.column.min(names.len().saturating_sub(1));
        let (mut accept, mut cancel) = (false, false);
        let dark = ui.visuals().dark_mode;
        let (back, page) = if dark {
            (ui.visuals().window_fill, ui.visuals().panel_fill)
        } else {
            (Color32::from_gray(0xf3), Color32::WHITE)
        };
        let button = |text: &str| egui::Button::new(tr(text)).min_size(vec2(BUTTON_W, BUTTON_H));
        ui.painter().rect_filled(ui.max_rect(), 0.0, back);
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Frame::NONE
                .fill(page)
                .stroke(Stroke::new(1.0, Color32::from_gray(0xb4)))
                .outer_margin(Margin::same(14))
                .inner_margin(Margin {
                    left: 28,
                    right: 28,
                    top: 24,
                    bottom: 28,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        caption_box(ui, "Select Columns", page, Margin::symmetric(8, 4), |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            for n in &names {
                                let mut shown = !p.hidden.contains(n);
                                if crate::widgets::checkbox(ui, &mut shown, n.as_str()).changed() {
                                    if shown {
                                        p.hidden.retain(|h| h != n);
                                    } else {
                                        p.hidden.push(n.clone());
                                    }
                                }
                            }
                        });
                        ui.add_space(58.0);
                        ui.vertical(|ui| {
                            let rows = caption_box(
                                ui,
                                "Select Rows With:",
                                page,
                                Margin {
                                    left: 30,
                                    right: 30,
                                    top: 18,
                                    bottom: 26,
                                },
                                |ui| {
                                    p.conditions_ui(ui, &names, status, button);
                                },
                            );
                            ui.add_space(26.0);
                            // Accept and Cancel, at the right under the conditions
                            ui.allocate_ui_with_layout(
                                vec2(rows.width(), BUTTON_H),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    cancel = ui.add(button("Cancel")).clicked();
                                    ui.add_space(30.0);
                                    accept = ui.add(button("Accept")).clicked();
                                },
                            );
                        });
                    });
                });
        });
        if cancel {
            self.page = None;
        }
        if accept && let Some(p) = self.page.take() {
            let changed = p.hidden != self.hidden;
            self.hidden = p.hidden;
            self.conditions = p.conditions;
            return changed;
        }
        false
    }
}

// Sizes on the Set Filters page, after EPANET-UI's: the conditions list is
// as wide as the column and relation boxes over it; Add and Delete line up
// under the value box.
const COLUMN_W: f32 = 210.0;
const RELATION_W: f32 = 124.0;
const VALUE_W: f32 = 124.0;
const GAP: f32 = 36.0;
const LIST_H: f32 = 122.0;
const BUTTON_W: f32 = 114.0;
const BUTTON_H: f32 = 34.0;
const FIELD_H: f32 = 28.0;

impl Page {
    /// The condition being entered (column, relation, value), the list of
    /// conditions, Add and Delete.
    fn conditions_ui(
        &mut self,
        ui: &mut Ui,
        names: &[String],
        status: &mut String,
        button: impl Fn(&str) -> egui::Button<'static>,
    ) {
        let p = self;
        // the boxes all the same height, centered on one line
        let row = vec2(COLUMN_W + RELATION_W + VALUE_W + 2.0 * GAP, FIELD_H);
        ui.allocate_ui_with_layout(
            row,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.spacing_mut().interact_size.y = FIELD_H;
                egui::ComboBox::from_id_salt("filter_column")
                    .selected_text(names.get(p.column).map_or("", |n| tr(n)))
                    .width(COLUMN_W)
                    .show_ui(ui, |ui| {
                        for (k, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut p.column, k, tr(n));
                        }
                    });
                ui.add_space(GAP);
                egui::ComboBox::from_id_salt("filter_relation")
                    .selected_text(tr(p.relation.label()))
                    .width(RELATION_W)
                    .show_ui(ui, |ui| {
                        for r in Relation::ALL {
                            ui.selectable_value(&mut p.relation, r, tr(r.label()));
                        }
                    });
                ui.add_space(GAP);
                ui.add_sized(
                    [VALUE_W, FIELD_H],
                    egui::TextEdit::singleline(&mut p.value).vertical_align(egui::Align::Center),
                );
            },
        );
        ui.add_space(26.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // the conditions, one per line; a click loads one into the
            // boxes above
            let (list, _) = ui.allocate_exact_size(
                vec2(COLUMN_W + GAP + RELATION_W, LIST_H),
                egui::Sense::hover(),
            );
            let v = ui.visuals();
            ui.painter().rect(
                list,
                0.0,
                v.extreme_bg_color,
                Stroke::new(1.0, Color32::from_gray(0x8a)),
                egui::StrokeKind::Inside,
            );
            let mut pick = None;
            // a child of its own, which leaves the row's place after the list
            let mut inside = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(list.shrink(4.0))
                    .layout(egui::Layout::top_down_justified(egui::Align::Min)),
            );
            for (k, c) in p.conditions.iter().enumerate() {
                if inside
                    .add(egui::Button::selectable(p.picked == Some(k), c.text()))
                    .clicked()
                {
                    pick = Some(k);
                }
            }
            if let Some(k) = pick {
                let c = &p.conditions[k];
                p.picked = Some(k);
                p.column = names.iter().position(|n| *n == c.column).unwrap_or(0);
                p.relation = c.relation;
                p.value = c.value.clone();
            }
            ui.add_space(GAP);
            // a column as wide as the buttons: their text is centered
            let column = egui::Layout::top_down(egui::Align::Center);
            ui.allocate_ui_with_layout(vec2(BUTTON_W, LIST_H), column, |ui| {
                if ui
                    .add_enabled(p.conditions.len() < MAX_CONDITIONS, button("Add"))
                    .clicked()
                    && let Some(column) = names.get(p.column)
                {
                    let value = p.value.trim().to_string();
                    if value.is_empty() {
                        *status = tr("Enter a value for the filter").into();
                    } else if !text_column(column) && eui_core::num::parse(&value).is_none() {
                        *status = trf("{} is not a valid number", &[&value]);
                    } else {
                        p.conditions.push(Condition {
                            column: column.clone(),
                            relation: p.relation,
                            value,
                        });
                        p.picked = Some(p.conditions.len() - 1);
                    }
                }
                ui.add_space(10.0);
                if ui
                    .add_enabled(p.picked.is_some(), button("Delete"))
                    .clicked()
                    && let Some(k) = p.picked
                {
                    p.conditions.remove(k);
                    p.picked = (!p.conditions.is_empty()).then(|| k.min(p.conditions.len() - 1));
                }
            });
        });
    }
}

/// A Windows group box: a thin light frame with its title over the top
/// line (on `bg`, the color behind the box). Returns the box's rectangle.
fn caption_box(
    ui: &mut Ui,
    title: &str,
    bg: Color32,
    margin: Margin,
    add: impl FnOnce(&mut Ui),
) -> egui::Rect {
    ui.vertical(|ui| {
        let color = ui.visuals().text_color();
        let galley = ui.painter().layout_no_wrap(
            tr(title).to_string(),
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
        let half = (galley.size().y / 2.0).round();
        ui.add_space(half);
        let frame = egui::Frame::NONE
            .stroke(Stroke::new(1.0, Color32::from_gray(0xdc)))
            .inner_margin(Margin {
                top: margin.top + half as i8,
                ..margin
            })
            .show(ui, add);
        let r = frame.response.rect;
        let pos = pos2(r.left() + 8.0, r.top() - half);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(pos - vec2(3.0, 0.0), galley.size() + vec2(6.0, 0.0)),
            0.0,
            bg,
        );
        ui.painter().galley(pos, galley, color);
        r
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Rows {
        let row = |cells: &[&str]| cells.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        vec![
            row(&["Link", "Type", "Flow Rate\ngpm", "Velocity\nfps", "Status"]),
            row(&["10", "Pipe", "-150.00", "1.20", "Open"]),
            row(&["11", "Pipe", "40.00", "0.30", "Open"]),
            row(&["9", "Pump", "1800.00", "0.00", "Closed"]),
        ]
    }

    fn condition(column: &str, relation: Relation, value: &str) -> Condition {
        Condition {
            column: column.into(),
            relation,
            value: value.into(),
        }
    }

    /// Every condition must hold; flows compare by size, Type and Status as
    /// text; hidden columns are left out but the ID column stays.
    #[test]
    fn rows_meet_every_condition_and_hidden_columns_go() {
        let mut f = TableFilter::default();
        assert_eq!(f.apply(&rows()), rows());
        f.conditions = vec![condition("Flow Rate", Relation::Above, "100")];
        let ids = |r: Rows| r[1..].iter().map(|r| r[0].clone()).collect::<Vec<_>>();
        assert_eq!(ids(f.apply(&rows())), ["10", "9"]);
        f.conditions
            .push(condition("Type", Relation::Equal, "pipe"));
        assert_eq!(ids(f.apply(&rows())), ["10"]);
        f.conditions = vec![condition("Status", Relation::Equal, "Closed")];
        assert_eq!(ids(f.apply(&rows())), ["9"]);
        f.conditions = vec![condition("Velocity", Relation::Equal, "0.3")];
        assert_eq!(ids(f.apply(&rows())), ["11"]);
        f.conditions.clear();
        f.hidden = vec!["Velocity".into(), "Link".into()];
        let r = f.apply(&rows());
        assert_eq!(r[0], ["Link", "Type", "Flow Rate\ngpm", "Status"]);
        assert_eq!(r[3], ["9", "Pump", "1800.00", "Closed"]);
    }

    /// The Set Filters page is laid out as EPANET-UI's: the column check
    /// boxes one under the other, the row conditions to their right (their
    /// title on one line), Accept and Cancel under the conditions.
    #[test]
    fn set_filters_page_layout() {
        let ctx = egui::Context::default();
        let mut f = TableFilter::default();
        f.open();
        let head = rows()[0].clone();
        let mut status = String::new();
        let mut texts = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1100.0, 500.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    f.page(ui, &head, &mut status);
                },
            );
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => {
                        Some((t.galley.text().to_string(), t.visual_bounding_rect()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
        }
        let at = |s: &str| {
            texts
                .iter()
                .find(|(t, _)| t == s)
                .map(|(_, r)| *r)
                .unwrap_or_else(|| panic!("{s}: {texts:?}"))
        };
        let (cols, rows_title) = (at("Select Columns"), at("Select Rows With:"));
        let (ty, flow, status_box) = (at("Type"), at("Flow Rate"), at("Status"));
        assert!(
            ty.top() > cols.bottom() && flow.top() > ty.bottom(),
            "{texts:?}"
        );
        assert!((ty.left() - status_box.left()).abs() < 1.0, "{texts:?}");
        assert!(rows_title.left() > ty.right(), "{texts:?}");
        assert!(rows_title.height() < 1.5 * cols.height(), "on one line");
        assert!(
            at("Add").left() > at("Below").right(),
            "Add beside the list"
        );
        assert!(at("Accept").top() > at("Delete").bottom(), "{texts:?}");
        assert!(at("Cancel").left() > at("Accept").right(), "{texts:?}");
    }
}
