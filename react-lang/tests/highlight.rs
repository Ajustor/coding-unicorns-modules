//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p react-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document: Some(react_lang::tokenize_document_ffi),
        tokenize_line: Some(react_lang::tokenize_line_ffi),
        reset_tokenizer: Some(react_lang::reset_tokenizer),
        ..Ffi::new(react_lang::file_extensions, react_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
