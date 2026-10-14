//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p go-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document: Some(go_lang::tokenize_document_ffi),
        tokenize_line: Some(go_lang::tokenize_line_ffi),
        reset_tokenizer: Some(go_lang::reset_tokenizer),
        ..Ffi::new(go_lang::file_extensions, go_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
