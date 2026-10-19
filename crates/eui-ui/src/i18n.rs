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
const FILES: &[(&str, &str)] = &[
    ("el", include_str!("../lang/el.json")),
    ("es", include_str!("../lang/es.json")),
    ("zh", include_str!("../lang/zh.json")),
];

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

impl Language {
    /// This language's text for `english`, or `english` itself.
    pub fn text<'a>(&'a self, english: &'a str) -> &'a str {
        self.table.get(english).map_or(english, String::as_str)
    }
}

/// The current language's text for `english`, or `english` itself.
pub fn tr(english: &str) -> &str {
    current().text(english)
}

/// `tr` for a text with values: each `{}` of the (translated) text takes
/// the next value.
pub fn trf(english: &str, values: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut rest = tr(english);
    for v in values {
        let Some((head, tail)) = rest.split_once("{}") else {
            break;
        };
        out += head;
        out += &v.to_string();
        rest = tail;
    }
    out + rest
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
    fn values_fill_the_translated_text() {
        assert_eq!(
            trf("Imported {} nodes and {} pipes", &[&3, &2]),
            "Imported 3 nodes and 2 pipes"
        );
        let el = all().iter().find(|l| l.code == "el").unwrap();
        assert!(
            el.text("Imported {} nodes and {} pipes")
                .matches("{}")
                .count()
                == 2
        );
    }

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

    /// Every character a translation brings (one its English text does not
    /// have) can be drawn with the app's fonts; the Chinese font is a subset
    /// of Noto Sans SC (see `fonts.rs`).
    #[test]
    fn fonts_cover_the_translations() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        for lang in all() {
            for (english, text) in &lang.table {
                for family in [egui::FontId::proportional(12.5), crate::fonts::bold(12.5)] {
                    let missing: String = text
                        .chars()
                        .filter(|&c| !english.contains(c))
                        .filter(|&c| !ctx.fonts_mut(|f| f.has_glyph(&family, c)))
                        .collect();
                    assert!(
                        missing.is_empty(),
                        "{}: {text:?} lacks {missing:?}",
                        lang.code
                    );
                }
            }
        }
    }

    /// Each language translates and falls back to English. The tables are
    /// read directly: switching the current language would change the text
    /// of the other tests running at the same time.
    #[test]
    fn every_language_translates_and_falls_back_to_english() {
        let expected = [
            ("en", "Project Explorer"),
            ("el", "Εξερεύνηση Έργου"),
            ("es", "Explorador del Proyecto"),
            ("zh", "项目浏览器"),
        ];
        assert_eq!(all().len(), expected.len(), "a language without a test");
        for (code, text) in expected {
            let lang = all().iter().find(|l| l.code == code).unwrap();
            assert_eq!(lang.text("Project Explorer"), text, "{code}");
            assert_eq!(lang.text("no such text"), "no such text", "{code}");
        }
    }

    /// Every language file translates the same texts.
    #[test]
    fn language_files_have_the_same_keys() {
        let keys = |json: &str| -> std::collections::BTreeSet<String> {
            let t: HashMap<String, String> = serde_json::from_str(json).unwrap();
            t.into_keys().filter(|k| !k.starts_with('_')).collect()
        };
        let (first, json) = FILES[0];
        let reference = keys(json);
        for (code, json) in &FILES[1..] {
            let these = keys(json);
            let missing: Vec<_> = reference.difference(&these).collect();
            let extra: Vec<_> = these.difference(&reference).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "lang/{code}.json vs lang/{first}.json: missing {missing:?}, extra {extra:?}"
            );
        }
    }
}
