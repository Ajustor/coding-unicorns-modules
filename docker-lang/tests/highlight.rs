//! Highlighting snapshots of `tests/samples/` (see `unicorns-highlight-test`).
//! After an intended change: `UPDATE_HIGHLIGHT=1 cargo test -p docker-lang`.

use unicorns_highlight_test::{check_samples, Ffi};

#[test]
fn samples() {
    let ffi = Ffi {
        tokenize_document_lang: Some(docker_lang::tokenize_document_lang_ffi),
        tokenize_document: Some(docker_lang::tokenize_document_ffi),
        tokenize_line_lang: Some(docker_lang::tokenize_line_lang_ffi),
        tokenize_line: Some(docker_lang::tokenize_line_ffi),
        reset_tokenizer: Some(docker_lang::reset_tokenizer),
        ..Ffi::new(docker_lang::file_extensions, docker_lang::free_string)
    };
    check_samples(env!("CARGO_MANIFEST_DIR"), &ffi);
}
