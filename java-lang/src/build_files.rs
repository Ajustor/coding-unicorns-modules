//! The files around Java code: Gradle scripts in Groovy (`*.gradle`,
//! `.groovy`) or Kotlin (`*.gradle.kts`, `.kts`, `.kt`), and `.properties`
//! files (`gradle.properties`, `application.properties`, `MANIFEST.MF`).
//! Highlighted with tree-sitter; they have no language server.

use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

#[cfg(test)]
/// Language keys handled here (file extensions, or the `file_names` target).
pub const LANGUAGES: &[&str] = &["gradle", "groovy", "kts", "kt", "properties"];

fn groovy() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let query = format!(
            "{}\n{}",
            dekobon_tree_sitter_groovy::HIGHLIGHTS_QUERY,
            include_str!("../queries/groovy-extra.scm")
        );
        unicorns_highlight::configure(
            dekobon_tree_sitter_groovy::LANGUAGE.into(),
            "groovy",
            &query,
            "",
        )
    })
}

fn kotlin() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        unicorns_highlight::configure(
            tree_sitter_kotlin_ng::LANGUAGE.into(),
            "kotlin",
            include_str!("../queries/kotlin.scm"),
            "",
        )
    })
}

fn properties() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        unicorns_highlight::configure(
            tree_sitter_properties::LANGUAGE.into(),
            "properties",
            include_str!("../queries/properties.scm"),
            "",
        )
    })
}

/// Token lines (the IDE's JSON) of `source` in language `lang`, or `None`
/// when `lang` isn't one of [`LANGUAGES`].
pub fn document_to_json(lang: &str, source: &str) -> Option<String> {
    let config = match lang {
        "gradle" | "groovy" => groovy(),
        "kts" | "kt" => kotlin(),
        "properties" => properties(),
        _ => return None,
    };
    Some(unicorns_highlight::document(config, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_kinds(lang: &str, source: &str, expected: &[(&str, &str)]) {
        let json = document_to_json(lang, source).unwrap();
        for (text, kind) in expected {
            let needle = format!(
                r#"{{"text":"{}","kind":"{kind}"}}"#,
                text.replace('"', "\\\"")
            );
            assert!(json.contains(&needle), "{lang}: {text} as {kind} in {json}");
        }
    }

    #[test]
    fn gradle_groovy_script() {
        let src = "// build\nplugins {\n    id 'java'\n}\n\
                   group = 'org.example'\nversion = \"1.${minor}\"\n\
                   dependencies {\n    implementation 'com.google.guava:guava:33.0.0-jre'\n    \
                   testImplementation group: 'junit', name: 'junit', version: '4.13'\n}\n\
                   def answer(int x) { return x * 42 }\n";
        assert_kinds(
            "gradle",
            src,
            &[
                ("// build", "comment"),
                ("id", "function"),
                ("'java'", "string"),
                ("implementation", "function"),
                ("testImplementation", "function"),
                ("group", "property"),
                ("def", "keyword"),
                ("answer", "function"),
                ("int", "type"),
                ("return", "keyword"),
                ("42", "number"),
            ],
        );
    }

    #[test]
    fn gradle_kotlin_script() {
        let src = "// build\nplugins {\n    kotlin(\"jvm\") version \"2.0.0\"\n    application\n}\n\
                   val ktorVersion: String by project\n\
                   dependencies {\n    implementation(\"io.ktor:ktor-server-core:$ktorVersion\")\n}\n\
                   tasks.register(\"hello\") { println(\"Hi\") }\n\
                   fun answer(x: Int): Int = x * 42\n";
        assert_kinds(
            "kts",
            src,
            &[
                ("// build", "comment"),
                ("plugins", "function"),
                ("\"jvm\"", "string"),
                ("val", "keyword"),
                ("String", "type"),
                ("by", "keyword"),
                ("implementation", "function"),
                ("register", "function"),
                ("fun", "keyword"),
                ("answer", "function"),
                ("Int", "type"),
                ("42", "number"),
            ],
        );
    }

    #[test]
    fn properties_file() {
        let src = "# Gradle\norg.gradle.jvmargs=-Xmx2g\nkotlin.code.style = official\n";
        assert_kinds(
            "properties",
            src,
            &[
                ("# Gradle", "comment"),
                ("org.gradle.jvmargs", "property"),
                ("-Xmx2g", "string"),
                ("official", "string"),
            ],
        );
    }

    #[test]
    fn other_languages_are_not_handled_here() {
        assert!(document_to_json("java", "class A {}").is_none());
        for lang in LANGUAGES {
            assert!(document_to_json(lang, "").is_some(), "{lang}");
        }
    }
}
