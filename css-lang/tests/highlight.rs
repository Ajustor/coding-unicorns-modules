//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p css-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document_lang: Some(css_lang::tokenize_document_lang_ffi),
        tokenize_document: Some(css_lang::tokenize_document_ffi),
        tokenize_line: Some(css_lang::tokenize_line_ffi),
        reset_tokenizer: Some(css_lang::reset_tokenizer),
        ..Ffi::new(css_lang::file_extensions, css_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
