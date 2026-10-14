//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p java-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document_lang: Some(java_lang::tokenize_document_lang_ffi),
        tokenize_document: Some(java_lang::tokenize_document_ffi),
        tokenize_line_lang: Some(java_lang::tokenize_line_lang_ffi),
        tokenize_line: Some(java_lang::tokenize_line_ffi),
        reset_tokenizer: Some(java_lang::reset_tokenizer),
        ..Ffi::new(java_lang::file_extensions, java_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
