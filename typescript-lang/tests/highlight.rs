//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p typescript-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document: Some(typescript_lang::tokenize_document_ffi),
        tokenize_document_tsx: Some(typescript_lang::tokenize_document_tsx_ffi),
        tokenize_line: Some(typescript_lang::tokenize_line_ffi),
        reset_tokenizer: Some(typescript_lang::reset_tokenizer),
        ..Ffi::new(typescript_lang::file_extensions, typescript_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
