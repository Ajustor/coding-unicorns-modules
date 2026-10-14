//! Windows batch file highlighting (.bat, .cmd) with tree-sitter-batch.

use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

fn config() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        // The grammar's query paints variables and labels with captures we
        // leave plain (`@variable`, `@label`): `%ROOT%`, `!COUNT!` and `%%f`
        // get the color of variables in shell scripts, `:label` that of a
        // function (a `goto` / `call` target). Appended: the last pattern
        // matching a node wins.
        let query = format!(
            "{}\n[(variable_name) (variable_reference) (for_variable)] @property\n(label) @function\n",
            tree_sitter_batch::HIGHLIGHTS_QUERY
        );
        unicorns_highlight::configure(tree_sitter_batch::LANGUAGE.into(), "batch", &query, "")
    })
}

/// The document as the IDE's token lines.
pub fn document_to_json(source: &str) -> String {
    unicorns_highlight::document(config(), source)
}

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"bat".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"bat,cmd".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// Tokenize the whole document: `[[{text,kind},…],…]`, freed with
/// [`free_string`].
///
/// # Safety
/// `text_ptr` must be null or a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    if text_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let text = unsafe { CStr::from_ptr(text_ptr) }.to_str().unwrap_or("");
    CString::new(document_to_json(text))
        .unwrap_or_default()
        .into_raw()
}

/// Line tokenizing is not offered: the IDE uses [`tokenize_document_ffi`].
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
