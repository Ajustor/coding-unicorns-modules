//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p vue-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document: Some(vue_lang::tokenize_document_ffi),
        tokenize_line: Some(vue_lang::tokenize_line_ffi),
        reset_tokenizer: Some(vue_lang::reset_tokenizer),
        ..Ffi::new(vue_lang::file_extensions, vue_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
