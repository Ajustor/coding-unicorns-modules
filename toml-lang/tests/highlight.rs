//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p toml-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document: Some(toml_lang::tokenize_document_ffi),
        tokenize_line: Some(toml_lang::tokenize_line_ffi),
        reset_tokenizer: Some(toml_lang::reset_tokenizer),
        ..Ffi::new(toml_lang::file_extensions, toml_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
