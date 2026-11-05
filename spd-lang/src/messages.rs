//! Translations shown in place of their keys, as the VS Code extension does
//! (after i18n-ally): `{t.panier.titre}` says nothing of what will be displayed,
//! so the text of the default locale is shown instead.
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
/// The choices of `t.group[index]`: all of them, as far as they fit…
const MAX_CHOICES_CHARS: usize = 120;
/// …each message cut shorter, and the expression too.
const MAX_CHOICE_CHARS: usize = 24;
const MAX_INDEX_CHARS: usize = 24;

/// The hints of `text`, the content of the component at `path`, as the IDE's
/// JSON `[{"line","col","from","text","target"}]`: each one in place of its
/// key `from..col`, as i18n-ally shows them (IDEs that don't hide the key put
/// the hint after it, older ones at the end of the line), and `target` the
/// line of the key in the catalogue, where Ctrl+click goes.
pub fn inline_hints(path: &str, text: &str) -> String {
    let found = reads(text);
    let catalogue = if found.is_empty() {
        None
    } else {
        catalogue(Path::new(path))
    };
    let Some(catalogue) = catalogue else {
        return "[]".to_string();
    };
    let target_path = catalogue.path.to_string_lossy();
    // The types of the component, read only when a key is chosen at run time.
    let scope = found
        .iter()
        .any(|read| read.index.is_some())
        .then(|| crate::types::Scope::of_component(Path::new(path), text));
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let hints: Vec<serde_json::Value> = found
        .into_iter()
        .map(|read| {
            let hint = match (&read.index, catalogue.messages.get(&read.key)) {
                (Some(index), _) => {
                    let allowed = scope.as_ref().and_then(|scope| {
                        let start = line_starts.get(read.line).copied().unwrap_or(0);
                        let at = text[start..]
                            .char_indices()
                            .nth(read.start)
                            .map_or(text.len(), |(i, _)| start + i);
                        scope.strings_of(text, at, index)
                    });
                    catalogue.choices(&read.key, index, allowed.as_deref())
                }
                (None, Some(value)) => short(value),
                (None, None) => "— clé absente".to_string(),
            };
            serde_json::json!({
                "line": read.line,
                "col": read.end,
                "from": read.start,
                "text": hint,
                "target": { "path": target_path, "line": catalogue.line_of(&read.key) },
            })
        })
        .collect();
    serde_json::Value::Array(hints).to_string()
}

/// A project's translation file, read.
struct Catalogue {
    path: PathBuf,
    /// Messages keyed in dotted notation.
    messages: HashMap<String, String>,
    /// The 0-based line of every key (of messages and of their groups).
    lines: HashMap<String, usize>,
}

impl Catalogue {
    /// What `t.group[index]` can show: each message of `group` with its key,
    /// in the order of the file, after the expression choosing among them —
    /// `selon statut : brouillon → « Brouillon » · payee → « Payée »`.
    /// `allowed`: the strings the expression's type allows, when known; only
    /// their messages are shown, those missing from the group last.
    fn choices(&self, group: &str, index: &str, allowed: Option<&[String]>) -> String {
        let prefix = format!("{group}.");
        let mut options: Vec<(&str, Option<&String>)> = match allowed {
            Some(allowed) => allowed
                .iter()
                .map(|name| (name.as_str(), self.messages.get(&format!("{prefix}{name}"))))
                .collect(),
            None => self
                .messages
                .iter()
                .filter_map(|(key, value)| {
                    let name = key.strip_prefix(&prefix)?;
                    (!name.contains('.')).then_some((name, Some(value)))
                })
                .collect(),
        };
        if options.is_empty() {
            return "— aucune valeur pour cette clé".to_string();
        }
        options.sort_by_key(|(name, value)| {
            let line = value.map(|_| self.line_of(&format!("{prefix}{name}")));
            (line.is_none(), line, *name)
        });
        let mut text = format!("selon {} :", short_to(index, MAX_INDEX_CHARS));
        for (shown, (name, value)) in options.iter().enumerate() {
            let option = format!(
                " {}{name} → {}",
                if shown == 0 { "" } else { "· " },
                match value {
                    Some(value) => format!("« {} »", short_to(value, MAX_CHOICE_CHARS)),
                    None => "— absente".to_string(),
                }
            );
            if text.chars().count() + option.chars().count() > MAX_CHOICES_CHARS {
                text.push_str(&format!(" · … (+{})", options.len() - shown));
                break;
            }
            text.push_str(&option);
        }
        text
    }

    /// The line of `key`, else of its closest group (where a missing key is
    /// to be added), else the first.
    fn line_of(&self, key: &str) -> usize {
        let mut key = key;
        loop {
            if let Some(&line) = self.lines.get(key) {
                return line;
            }
            match key.rfind('.') {
                Some(dot) => key = &key[..dot],
                None => return 0,
            }
        }
    }
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

/// The catalogue of the project holding `component`. Read on every call: the
/// file is small, and a cache would have to know when it changes. A JSON being
/// edited reads as no catalogue — nothing to show, not an error.
fn catalogue(component: &Path) -> Option<Catalogue> {
    let path = catalogue_file(component)?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let tree: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let mut messages = HashMap::new();
    flatten(&tree, "", &mut messages);
    Some(Catalogue {
        path,
        messages,
        lines: key_lines(&raw),
    })
}

/// The 0-based line of each key of a JSON document (valid: parsed before),
/// in dotted notation. Keys of objects inside arrays are no messages and are
/// left out, as [`flatten`] does.
fn key_lines(raw: &str) -> HashMap<String, usize> {
    enum Frame {
        Object(Option<String>),
        Array,
    }
    let mut lines = HashMap::new();
    let mut stack: Vec<Frame> = Vec::new();
    // The last key read: the name of the object it opens, if it opens one.
    let mut last_key: Option<String> = None;
    let mut line = 0;
    let mut chars = raw.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '\n' => line += 1,
            '{' => stack.push(Frame::Object(last_key.take())),
            '[' => {
                last_key = None;
                stack.push(Frame::Array);
            }
            '}' | ']' => {
                stack.pop();
            }
            '"' => {
                let mut end = raw.len();
                let mut escaped = false;
                for (i, c) in chars.by_ref() {
                    match c {
                        _ if escaped => escaped = false,
                        '\\' => escaped = true,
                        '"' => {
                            end = i;
                            break;
                        }
                        _ => {}
                    }
                }
                while chars
                    .next_if(|(_, c)| c.is_whitespace() && *c != '\n')
                    .is_some()
                {}
                if chars.peek().is_none_or(|(_, c)| *c != ':') {
                    last_key = None;
                    continue;
                }
                let quoted = raw.get(at..=end).unwrap_or_default();
                let key: String = serde_json::from_str(quoted)
                    .unwrap_or_else(|_| quoted.trim_matches('"').to_string());
                if !stack.iter().any(|f| matches!(f, Frame::Array)) {
                    let mut full = String::new();
                    for frame in &stack {
                        if let Frame::Object(Some(name)) = frame {
                            full.push_str(name);
                            full.push('.');
                        }
                    }
                    full.push_str(&key);
                    lines.insert(full, line);
                }
                last_key = Some(key);
            }
            ',' => last_key = None,
            _ => {}
        }
    }
    lines
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

/// A read of the catalogue: `t.panier.titre` on `line`, from column `start`
/// (its `t`) to `end` (just after it), 0-based, columns in characters.
/// `t.statuts[commande.statut]` reads the group `statuts`, `index` being the
/// expression between the brackets.
#[derive(Debug, PartialEq)]
pub struct Read {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub key: String,
    pub index: Option<String>,
}

/// The `]` closing the `[` at `open`, on the same line.
fn closing_bracket(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every read of `t` in `text`.
/// What precedes the `t` is neither a word character nor a dot, so neither
/// `objet.t.court` nor `format.t` reads the catalogue.
pub fn reads(text: &str) -> Vec<Read> {
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
                let key = chars[i + 2..end].iter().collect();
                // `[expression]`: one message of the group, chosen at run time.
                let index = (chars.get(end) == Some(&'['))
                    .then(|| closing_bracket(&chars, end))
                    .flatten();
                let (end, index) = match index {
                    Some(close) => (
                        close + 1,
                        Some(chars[end + 1..close].iter().collect::<String>().trim().to_string()),
                    ),
                    None => (end, None),
                };
                found.push(Read {
                    line: line_index,
                    start: i,
                    end,
                    key,
                    index,
                });
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
    short_to(value, MAX_CHARS)
}

/// `value` on one line, cut past `max` characters.
fn short_to(value: &str, max: usize) -> String {
    let single = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if single.chars().count() > max {
        let cut: String = single.chars().take(max - 1).collect();
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
        let read = |line, start, end, key: &str| Read {
            line,
            start,
            end,
            key: key.to_string(),
            index: None,
        };
        assert_eq!(
            reads(text),
            vec![read(0, 5, 16, "app.titre"), read(1, 4, 15, "app.clics")]
        );
    }

    #[test]
    fn reads_take_the_index_of_a_group() {
        let text = "<p>{t.statuts[ commande[0].statut ]}</p> {t.a[}";
        let found = reads(text);
        assert_eq!(
            found[0],
            Read {
                line: 0,
                start: 4,
                end: 35,
                key: "statuts".to_string(),
                index: Some("commande[0].statut".to_string()),
            }
        );
        // Unclosed: the key alone.
        assert_eq!((found[1].end, found[1].index.as_deref()), (45, None));
    }

    #[test]
    fn choices_list_the_group_in_file_order() {
        let raw = r#"{"statuts": {"payee": "Payée", "brouillon": "Brouillon", "sous": {"x": "y"}}}"#;
        let tree: serde_json::Value = serde_json::from_str(raw).unwrap();
        let mut messages = HashMap::new();
        flatten(&tree, "", &mut messages);
        let catalogue = Catalogue {
            path: PathBuf::from("fr.json"),
            messages,
            lines: HashMap::from([
                ("statuts.payee".to_string(), 1),
                ("statuts.brouillon".to_string(), 2),
            ]),
        };
        assert_eq!(
            catalogue.choices("statuts", "c.statut", None),
            "selon c.statut : payee → « Payée » · brouillon → « Brouillon »"
        );
        assert_eq!(
            catalogue.choices("rien", "x", None),
            "— aucune valeur pour cette clé"
        );
        // Only what the type allows, the missing ones last.
        let allowed = ["oubli".to_string(), "brouillon".to_string()];
        assert_eq!(
            catalogue.choices("statuts", "c.statut", Some(&allowed)),
            "selon c.statut : brouillon → « Brouillon » · oubli → — absente"
        );
        let many: HashMap<String, String> = (0..20)
            .map(|i| (format!("g.k{i:02}"), "Une valeur".to_string()))
            .collect();
        let catalogue = Catalogue {
            messages: many,
            ..catalogue
        };
        let text = catalogue.choices("g", "i", None);
        assert!(text.chars().count() <= MAX_CHOICES_CHARS + 12, "{text}");
        assert!(text.ends_with(" · … (+16)"), "{text}");
    }

    #[test]
    fn key_lines_follow_nesting() {
        let raw = r#"{
  "a": {
    "b": "x, y: z",
    "c": { "d": "z" }
  },
  "l": [{ "e": "1" }],
  "f\"g": "2"
}"#;
        let lines = key_lines(raw);
        assert_eq!(lines.get("a"), Some(&1));
        assert_eq!(lines.get("a.b"), Some(&2));
        assert_eq!(lines.get("a.c.d"), Some(&3));
        assert_eq!(lines.get("l"), Some(&5));
        assert_eq!(lines.get("e"), None, "keys in arrays are no messages");
        assert_eq!(lines.get("f\"g"), Some(&6));
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
        let en = lang.join("en.json");
        std::fs::write(&en, "{\n  \"app\": {\n    \"titre\": \"Home\"\n  }\n}").unwrap();
        let page = routes.join("+page.spd");
        let text = "<h1>{t.app.titre}</h1>\n\n<p>{t.app.absent}</p>";

        // Without the default locale, the first one.
        let hints: serde_json::Value =
            serde_json::from_str(&inline_hints(page.to_str().unwrap(), text)).unwrap();
        let en = en.to_string_lossy();
        assert_eq!(
            hints,
            serde_json::json!([
                { "line": 0, "col": 16, "from": 5, "text": "Home",
                  "target": { "path": en, "line": 2 } },
                // A missing key goes to its group.
                { "line": 2, "col": 16, "from": 4, "text": "— clé absente",
                  "target": { "path": en, "line": 1 } },
            ])
        );
        std::fs::write(lang.join("fr.json"), r#"{"app":{"titre":"Accueil"}}"#).unwrap();
        assert!(inline_hints(page.to_str().unwrap(), text).contains(r#""text":"Accueil""#));
        // No catalogue, nothing to show.
        assert_eq!(inline_hints("/nulle/part/x.spd", text), "[]");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
