//! CSS, SCSS and Less highlighting with tree-sitter.

use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

/// The query of a CSS superset (SCSS, Less). Theirs are written for Neovim
/// (`; inherits: css`): they only cover what the language adds, so the CSS
/// query, whose nodes these grammars share, comes first. Their second
/// `@spell` capture hid the comments, and variables (`$x`, `@x`) get the
/// color of variables elsewhere instead of none.
fn superset_query(own: &str) -> String {
    format!(
        "{}\n{}\n(variable) @property\n",
        tree_sitter_css::HIGHLIGHTS_QUERY,
        own.replace(" @spell", "")
    )
}

fn scss() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let query = superset_query(tree_sitter_scss::HIGHLIGHTS_QUERY);
        unicorns_highlight::configure(tree_sitter_scss::language(), "scss", &query, "")
    })
}

fn less() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let query = superset_query(tree_sitter_less::HIGHLIGHTS_QUERY);
        unicorns_highlight::configure(tree_sitter_less::language(), "less", &query, "")
    })
}

/// The document in language `lang` (`css`, `scss` or `less`) as the IDE's
/// token lines.
pub fn document_to_json(lang: &str, source: &str) -> String {
    let config = match lang {
        "scss" => scss(),
        "less" => less(),
        // The same CSS highlighting as in HTML, Vue and Svelte files.
        _ => unicorns_highlight::css(),
    };
    unicorns_highlight::document(config, source)
}

// ── FFI ───────────────────────────────────────────────────────────────────────

/// # Safety
/// `ptr` must be null or a valid, nul-terminated C string.
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
    c"css".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"css,scss,less".as_ptr()
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

/// The whole document as CSS, for IDEs without the language-aware export.
///
/// # Safety
/// `text_ptr` must be null or a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    into_c(document_to_json("css", unsafe { from_c(text_ptr) }))
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
