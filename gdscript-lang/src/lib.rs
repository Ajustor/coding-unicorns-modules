//! Godot support: GDScript (`.gd`), scene and resource files (`.tscn`,
//! `.tres`, `project.godot`, `.import`, `.gdextension`) and shaders
//! (`.gdshader`, `.gdshaderinc`), highlighted with tree-sitter.
//!
//! Completion, diagnostics and debugging come from the Godot editor, which
//! runs a language server and a debug adapter the IDE connects to (`connect`
//! in `manifest.toml`).

use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;
use unicorns_highlight::{Kind, Paint};

fn gdscript() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        unicorns_highlight::configure(
            tree_sitter_gdscript::LANGUAGE.into(),
            "gdscript",
            include_str!("../queries/gdscript.scm"),
            "",
        )
    })
}

fn resource() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        unicorns_highlight::configure(
            tree_sitter_godot_resource::LANGUAGE.into(),
            "godot_resource",
            include_str!("../queries/godot-resource.scm"),
            "",
        )
    })
}

fn shader() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        // A later pattern of the crate's query captures the same `shader_type`
        // node as `render_mode`, and tree-sitter-highlight then drops the whole
        // earlier match: render modes (`blend_mix`…) stay uncolored without this.
        let query = format!(
            "{}\n(render_mode) @attribute\n",
            tree_sitter_gdshader::HIGHLIGHTS_QUERY
        );
        unicorns_highlight::configure(
            tree_sitter_gdshader::LANGUAGE.into(),
            "gdshader",
            &query,
            "",
        )
    })
}

/// The document in language `lang` as the IDE's token lines.
pub fn document_to_json(lang: &str, source: &str) -> String {
    match lang {
        "tscn" | "tres" | "godot" | "import" | "gdextension" => {
            let mut paint = Paint::new(source);
            paint.highlight(resource(), 0..source.len());
            dotted_keys(source, &mut paint);
            paint.to_json()
        }
        "gdshader" | "gdshaderinc" => unicorns_highlight::document(shader(), source),
        _ => unicorns_highlight::document(gdscript(), source),
    }
}

/// Keys with a dot (`macos.debug`, `path.s3tc`, `rendering_method.mobile`):
/// tree-sitter-godot-resource 0.7 cannot parse them and leaves them half
/// coloured. Paint the key of each `key = value` line not inside a string.
fn dotted_keys(source: &str, paint: &mut Paint) {
    let mut start = 0;
    for line in source.split('\n') {
        let key_start = start + (line.len() - line.trim_start().len());
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim();
            if key.contains('.')
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_/:.".contains(&b))
                && paint.kind_at(key_start) != Kind::String
            {
                paint.fill(key_start..key_start + key.len(), Kind::Property);
            }
        }
        start += line.len() + 1;
    }
}

// ── FFI ───────────────────────────────────────────────────────────────────────

unsafe fn from_c<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"gdscript".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"gd,tscn,tres,godot,import,gdextension,gdshader,gdshaderinc".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// The whole document in language `lang`: `[[{text,kind},…],…]`, freed with
/// [`free_string`].
///
/// # Safety
/// Both pointers must be null or valid, nul-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_lang_ffi(
    lang_ptr: *const c_char,
    text_ptr: *const c_char,
) -> *mut c_char {
    let (lang, text) = unsafe { (from_c(lang_ptr), from_c(text_ptr)) };
    into_c(document_to_json(lang, text))
}

/// The whole document as GDScript, for IDEs without the language-aware export.
///
/// # Safety
/// `text_ptr` must be null or a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    into_c(document_to_json("gd", unsafe { from_c(text_ptr) }))
}

/// Line tokenizing is not offered: the IDE uses the document tokenizers.
#[no_mangle]
pub extern "C" fn tokenize_line_ffi(_line_ptr: *const c_char) -> *mut c_char {
    std::ptr::null_mut()
}

/// # Safety
/// `ptr` must come from this module and not be freed yet.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kind of the first token whose text is `text`.
    fn kind_of(json: &str, text: &str) -> Option<String> {
        let needle = format!(r#"{{"text":"{}","kind":""#, text.replace('"', "\\\""));
        let start = json.find(&needle)? + needle.len();
        let end = json[start..].find('"')? + start;
        Some(json[start..end].to_string())
    }

    #[test]
    fn each_language_gets_its_grammar() {
        let gd = document_to_json("gd", "var speed := 1\n");
        assert_eq!(kind_of(&gd, "var").as_deref(), Some("keyword"));
        let scene = document_to_json("tscn", "[node name=\"A\"]\nspeed = 1\n");
        assert_eq!(kind_of(&scene, "node").as_deref(), Some("keyword"));
        assert_eq!(kind_of(&scene, "speed").as_deref(), Some("property"));
        for lang in ["tres", "godot", "import", "gdextension"] {
            let json = document_to_json(lang, "[remap]\n");
            assert_eq!(
                kind_of(&json, "remap").as_deref(),
                Some("keyword"),
                "{lang}"
            );
        }
        for lang in ["gdshader", "gdshaderinc"] {
            let json = document_to_json(lang, "uniform float a;\n");
            assert_eq!(
                kind_of(&json, "uniform").as_deref(),
                Some("keyword"),
                "{lang}"
            );
        }
    }

    #[test]
    fn dotted_keys_are_properties() {
        let json = document_to_json(
            "gdextension",
            "[libraries]\nwindows.debug.x86_64 = \"res://a.dll\"\n",
        );
        assert_eq!(
            kind_of(&json, "windows.debug.x86_64").as_deref(),
            Some("property")
        );
        assert_eq!(kind_of(&json, "libraries").as_deref(), Some("keyword"));
    }

    #[test]
    fn language_less_export_reads_gdscript() {
        let text = CString::new("func _ready():\n\tpass\n").unwrap();
        let out = unsafe { tokenize_document_ffi(text.as_ptr()) };
        let json = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
        unsafe { free_string(out) };
        assert_eq!(kind_of(&json, "func").as_deref(), Some("keyword"));
        assert_eq!(kind_of(&json, "_ready").as_deref(), Some("function"));
    }
}
