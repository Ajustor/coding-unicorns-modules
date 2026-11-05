//! Translations shown at the end of lines, as the VS Code extension does (after
//! i18n-ally): `{t.panier.titre}` says nothing of what will be displayed, so the
//! text of the default locale is shown next to it.
//!
//! This is a reading aid, not a check: the language server types `t` and the
//! checker reports unknown keys. A key missing from the catalogue is still said,
//! softly, since showing nothing would pass for an empty translation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Where the translations live, relative to the project (as the compiler and
/// the VS Code extension's `speedster.langDirectory` default).
const LANG_DIRECTORY: &str = "src/lang";
/// The locale shown (the VS Code extension's `speedster.defaultLocale` default);
/// a project without it shows its first locale.
const DEFAULT_LOCALE: &str = "fr";
/// A hint is no preview: a long message would bury the line it explains.
const MAX_CHARS: usize = 60;

/// The hints of `text`, the content of the component at `path`, as the IDE's
/// JSON `[{"line","col","text"}]`: each one right after its key, as i18n-ally
/// shows them (IDEs without inlays put them at the end of the line).
pub fn inline_hints(path: &str, text: &str) -> String {
    let found = reads(text);
    let messages = if found.is_empty() {
        None
    } else {
        catalogue(Path::new(path))
    };
    let Some(messages) = messages else {
        return "[]".to_string();
    };
    let hints: Vec<serde_json::Value> = found
        .into_iter()
        .map(|(line, col, key)| {
            let text = match messages.get(&key) {
                Some(value) => short(value),
                None => "— clé absente".to_string(),
            };
            serde_json::json!({ "line": line, "col": col, "text": text })
        })
        .collect();
    serde_json::Value::Array(hints).to_string()
}

/// The translation file of the project holding `component`: `src/lang/fr.json`
/// in the closest ancestor having a `src/lang`, else its first `.json`.
fn catalogue_file(component: &Path) -> Option<PathBuf> {
    let directory = component
        .ancestors()
        .skip(1)
        .map(|dir| dir.join(LANG_DIRECTORY))
        .find(|dir| dir.is_dir())?;
    let preferred = directory.join(format!("{DEFAULT_LOCALE}.json"));
    if preferred.is_file() {
        return Some(preferred);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files.into_iter().next()
}

/// The messages of the catalogue, keyed in dotted notation. Read on every call:
/// the file is small, and a cache would have to know when it changes. A JSON
/// being edited reads as no catalogue — nothing to show, not an error.
fn catalogue(component: &Path) -> Option<HashMap<String, String>> {
    let raw = std::fs::read_to_string(catalogue_file(component)?).ok()?;
    let tree: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let mut messages = HashMap::new();
    flatten(&tree, "", &mut messages);
    Some(messages)
}

/// `{"panier": {"titre": "…"}}` → `panier.titre`, as the compiler names keys.
/// An array is no tree of messages: no component can write `liste.0`.
fn flatten(tree: &serde_json::Value, path: &str, into: &mut HashMap<String, String>) {
    let Some(object) = tree.as_object() else {
        return;
    };
    for (key, value) in object {
        let full = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        match value {
            serde_json::Value::String(text) => {
                into.insert(full, text.clone());
            }
            _ => flatten(value, &full, into),
        }
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Every read of `t` — `t.panier.titre` — with its line, the column just after
/// it (both 0-based, the column in characters) and its key.
/// What precedes the `t` is neither a word character nor a dot, so neither
/// `objet.t.court` nor `format.t` reads the catalogue.
pub fn reads(text: &str) -> Vec<(usize, usize, String)> {
    let mut found = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let starts = chars[i] == 't'
                && (i == 0 || !(is_ident(chars[i - 1]) || chars[i - 1] == '.'));
            if !starts {
                i += 1;
                continue;
            }
            // `.segment` repeated, each starting with a letter, `_` or `$`.
            let mut j = i + 1;
            let mut end = j;
            while j + 1 < chars.len()
                && chars[j] == '.'
                && (chars[j + 1].is_alphabetic() || chars[j + 1] == '_' || chars[j + 1] == '$')
            {
                j += 1;
                while j < chars.len() && is_ident(chars[j]) {
                    j += 1;
                }
                end = j;
            }
            if end > i + 1 {
                found.push((line_index, end, chars[i + 2..end].iter().collect()));
                i = end;
            } else {
                i += 1;
            }
        }
    }
    found
}

/// The message on one line, cut past [`MAX_CHARS`].
fn short(value: &str) -> String {
    let single = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if single.chars().count() > MAX_CHARS {
        let cut: String = single.chars().take(MAX_CHARS - 1).collect();
        format!("{cut}…")
    } else {
        single
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_find_keys_and_lines() {
        let text = "<h1>{t.app.titre}</h1>\n<p>{t.app.clics({ n })} {objet.t.court} {format.t}</p>\nconst t2 = t";
        assert_eq!(
            reads(text),
            vec![
                (0, 16, "app.titre".to_string()),
                (1, 15, "app.clics".to_string())
            ]
        );
    }

    #[test]
    fn flatten_uses_dotted_keys_and_skips_arrays() {
        let tree = serde_json::json!({ "app": { "titre": "Accueil", "liste": ["a"] }, "ok": "Oui" });
        let mut messages = HashMap::new();
        flatten(&tree, "", &mut messages);
        assert_eq!(messages.get("app.titre").map(String::as_str), Some("Accueil"));
        assert_eq!(messages.get("ok").map(String::as_str), Some("Oui"));
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn short_keeps_one_line() {
        assert_eq!(short("Bonjour\n  le   monde"), "Bonjour le monde");
        let long = "x".repeat(80);
        assert_eq!(short(&long).chars().count(), MAX_CHARS);
        assert!(short(&long).ends_with('…'));
    }

    #[test]
    fn hints_from_the_project_catalogue() {
        let root = std::env::temp_dir().join(format!("spd-hints-{}", std::process::id()));
        let lang = root.join("src/lang");
        let routes = root.join("src/routes");
        std::fs::create_dir_all(&lang).unwrap();
        std::fs::create_dir_all(&routes).unwrap();
        std::fs::write(lang.join("en.json"), r#"{"app":{"titre":"Home"}}"#).unwrap();
        let page = routes.join("+page.spd");
        let text = "<h1>{t.app.titre}</h1>\n\n<p>{t.app.absent}</p>";

        // Without the default locale, the first one.
        assert_eq!(
            inline_hints(page.to_str().unwrap(), text),
            r#"[{"col":16,"line":0,"text":"Home"},{"col":16,"line":2,"text":"— clé absente"}]"#
        );
        std::fs::write(lang.join("fr.json"), r#"{"app":{"titre":"Accueil"}}"#).unwrap();
        assert!(inline_hints(page.to_str().unwrap(), text).contains(r#""text":"Accueil""#));
        // No catalogue, nothing to show.
        assert_eq!(inline_hints("/nulle/part/x.spd", text), "[]");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
