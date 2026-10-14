/// The page as the IDE's token lines: markup, `<script>` and `<style>`.
fn document_to_json(source: &str) -> String {
    unicorns_highlight::page(source)
}

// ── HTML tag hover docs ───────────────────────────────────────────────────────

const HTML_TAG_DOCS: &[(&str, &str)] = &[
    ("html", "Root element of an HTML document"),
    ("head", "Container for metadata (title, scripts, styles)"),
    ("body", "Container for the visible page content"),
    ("div", "Generic block-level container"),
    ("span", "Generic inline container"),
    ("p", "Paragraph"),
    ("a", "Hyperlink — use href attribute for the URL"),
    ("img", "Image — use src and alt attributes"),
    ("input", "Input control (text, checkbox, radio, etc.)"),
    ("button", "Clickable button"),
    ("form", "Form container for user input"),
    ("table", "Table element"),
    ("tr", "Table row"),
    ("td", "Table data cell"),
    ("th", "Table header cell"),
    ("ul", "Unordered list"),
    ("ol", "Ordered list"),
    ("li", "List item"),
    ("h1", "Heading level 1 (largest)"),
    ("h2", "Heading level 2"),
    ("h3", "Heading level 3"),
    ("h4", "Heading level 4"),
    ("h5", "Heading level 5"),
    ("h6", "Heading level 6 (smallest)"),
    ("header", "Header section of a page or section"),
    ("footer", "Footer section"),
    ("nav", "Navigation links container"),
    ("main", "Main content of the document"),
    ("section", "Thematic section"),
    ("article", "Self-contained content (blog post, news article)"),
    ("aside", "Sidebar or tangential content"),
    ("script", "Embedded JavaScript"),
    ("style", "Embedded CSS styles"),
    ("link", "External resource link (stylesheets, icons)"),
    ("meta", "Document metadata (charset, viewport, etc.)"),
    ("title", "Document title (shown in browser tab)"),
    ("br", "Line break (void element)"),
    ("hr", "Horizontal rule / thematic break"),
    ("label", "Label for a form control"),
    ("select", "Drop-down select list"),
    ("option", "Option in a select list"),
    ("textarea", "Multi-line text input"),
    ("video", "Video player"),
    ("audio", "Audio player"),
    ("canvas", "Drawing surface for JavaScript graphics"),
    ("iframe", "Inline frame for embedding another document"),
];

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const std::ffi::c_char {
    c"html".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char {
    c"html,htm".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(
    text_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let text = unsafe { std::ffi::CStr::from_ptr(text_ptr).to_str().unwrap_or("") };
    let json = document_to_json(text);
    std::ffi::CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn tokenize_line_ffi(
    _line_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut std::ffi::c_char) {
    if !ptr.is_null() {
        unsafe { drop(std::ffi::CString::from_raw(ptr)) }
    }
}

pub fn hover_info(word: &str, _file_content: &str) -> Option<String> {
    HTML_TAG_DOCS
        .iter()
        .find(|(tag, _)| *tag == word)
        .map(|(tag, desc)| format!("```html\n<{}>\n```\n{}", tag, desc))
}

#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(
    word_ptr: *const std::ffi::c_char,
    file_content_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let word = unsafe { std::ffi::CStr::from_ptr(word_ptr).to_str().unwrap_or("") };
    let content = unsafe { std::ffi::CStr::from_ptr(file_content_ptr).to_str().unwrap_or("") };
    match hover_info(word, content) {
        Some(s) => std::ffi::CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}
