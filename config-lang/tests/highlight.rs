//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p config-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document_lang: Some(config_lang::tokenize_document_lang_ffi),
        tokenize_document: Some(config_lang::tokenize_document_ffi),
        tokenize_line: Some(config_lang::tokenize_line_ffi),
        reset_tokenizer: Some(config_lang::reset_tokenizer),
        ..Ffi::new(config_lang::file_extensions, config_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
