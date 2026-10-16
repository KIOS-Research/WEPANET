//! Interface languages.
//!
//! The English text in the code is the key: `tr("Project Explorer")` looks it
//! up in the current language's table in `lang/<code>.json` and falls back to
//! the English text when there is no translation. The UI helpers (toolbar
//! buttons, menu items, check boxes, dialog titles, panel headers, ...) call
//! `tr` themselves, so most strings only need an entry in the language file.
//!
//! To add a language, copy `lang/el.json` to `lang/<code>.json`, translate the
//! values (and `_language`, the name shown in Preferences), and list the file
//! in `FILES` below.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The translation files: (language code, JSON contents).
const FILES: &[(&str, &str)] = &[("el", include_str!("../lang/el.json"))];

pub struct Language {
    pub code: &'static str,
    /// The language's own name, as listed in Preferences.
    pub name: String,
    table: HashMap<String, String>,
}

/// English first, then the translations in `FILES` order.
pub fn all() -> &'static [Language] {
    static LANGUAGES: OnceLock<Vec<Language>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        let mut all = vec![Language {
            code: "en",
            name: "English".into(),
            table: HashMap::new(),
        }];
        for (code, json) in FILES {
            let mut table: HashMap<String, String> = match serde_json::from_str(json) {
                Ok(t) => t,
                Err(e) => {
                    log::error!("lang/{code}.json: {e}");
                    continue;
                }
            };
            let name = table
                .remove("_language")
                .unwrap_or_else(|| code.to_string());
            table.retain(|k, v| !k.starts_with('_') && !v.is_empty());
            all.push(Language { code, name, table });
        }
        all
    })
}

static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Switches the interface language (unknown codes mean English).
pub fn set(code: &str) {
    let k = all().iter().position(|l| l.code == code).unwrap_or(0);
    CURRENT.store(k, Ordering::Relaxed);
}

pub fn current() -> &'static Language {
    &all()[CURRENT.load(Ordering::Relaxed)]
}

/// The current language's text for `english`, or `english` itself.
pub fn tr(english: &str) -> &str {
    current().table.get(english).map_or(english, String::as_str)
}

/// `tr` for widget text: translates plain text, leaves styled text alone.
pub fn tr_widget(text: egui::WidgetText) -> egui::WidgetText {
    match text {
        egui::WidgetText::Text(s) => tr(&s).to_owned().into(),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_files_parse() {
        for (code, json) in FILES {
            let table: HashMap<String, String> =
                serde_json::from_str(json).unwrap_or_else(|e| panic!("lang/{code}.json: {e}"));
            assert!(
                table.contains_key("_language"),
                "lang/{code}.json: no _language"
            );
        }
        assert_eq!(all().len(), FILES.len() + 1);
    }

    #[test]
    fn falls_back_to_english() {
        set("el");
        assert_eq!(tr("Project Explorer"), "Εξερεύνηση Έργου");
        assert_eq!(tr("no such text"), "no such text");
        set("en");
        assert_eq!(tr("Project Explorer"), "Project Explorer");
    }
}
