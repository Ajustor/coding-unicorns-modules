//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p powershell-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document: Some(powershell_lang::tokenize_document_ffi),
        tokenize_line: Some(powershell_lang::tokenize_line_ffi),
        reset_tokenizer: Some(powershell_lang::reset_tokenizer),
        ..Ffi::new(powershell_lang::file_extensions, powershell_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
