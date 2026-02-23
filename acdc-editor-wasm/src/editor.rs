//! DOM orchestration for the `AsciiDoc` live editor.
//!
//! Moves all event handling, debouncing, scroll sync, clipboard, and Tab key
//! logic from JavaScript into Rust/WASM via `web-sys`.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, Event, HtmlElement, HtmlTextAreaElement, KeyboardEvent, Window};

#[wasm_bindgen]
extern "C" {
    /// Re-typeset math in the preview pane via MathJax.
    ///
    /// Defined in `index.html`; silently no-ops if MathJax has not loaded yet.
    #[wasm_bindgen(js_name = typesetMathPreview)]
    fn typeset_math_preview();
}

const DEBOUNCE_MS: i32 = 25;

const DEFAULT_CONTENT: &str = include_str!("../assets/default-content.adoc");

/// Cached references to the DOM elements the editor interacts with.
struct EditorState {
    window: Window,
    editor: HtmlTextAreaElement,
    highlight: Element,
    backdrop: Element,
    preview: Element,
    parse_status: HtmlElement,
    /// Timer handle for the debounced parse. 0 means no pending timer.
    debounce_handle: Cell<i32>,
}

impl EditorState {
    /// Look up all required DOM elements and return an `EditorState`.
    fn from_document(doc: &Document, window: Window) -> Result<Self, JsValue> {
        let editor: HtmlTextAreaElement = doc
            .get_element_by_id("editor")
            .ok_or("missing #editor")?
            .dyn_into()?;
        let highlight = doc
            .get_element_by_id("highlight")
            .ok_or("missing #highlight")?;
        let backdrop = doc
            .get_element_by_id("editor-backdrop")
            .ok_or("missing #editor-backdrop")?;
        let preview = doc.get_element_by_id("preview").ok_or("missing #preview")?;
        let parse_status: HtmlElement = doc
            .get_element_by_id("parse-status")
            .ok_or("missing #parse-status")?
            .dyn_into()?;

        Ok(Self {
            window,
            editor,
            highlight,
            backdrop,
            preview,
            parse_status,
            debounce_handle: Cell::new(0),
        })
    }
}

// ---------------------------------------------------------------------------
// Parse status badge
// ---------------------------------------------------------------------------

fn set_parse_status(state: &EditorState, msg: &str, is_error: bool) {
    let cl = state.parse_status.class_list();
    if is_error {
        let _ = cl.remove_1("parse-ok");
        let _ = cl.add_1("parse-error");
        let _ = cl.remove_1("expanded");
        // Update the text span with the error message
        if let Some(text_el) = state
            .parse_status
            .query_selector(".parse-status-text")
            .ok()
            .flatten()
        {
            text_el.set_text_content(Some(msg));
        }
        // Update the icon to an exclamation triangle
        if let Some(icon_el) = state
            .parse_status
            .query_selector(".parse-status-icon")
            .ok()
            .flatten()
        {
            icon_el.set_inner_html(r#"<i class="fa-solid fa-triangle-exclamation"></i>"#);
        }
        state.parse_status.set_attribute("title", msg).unwrap_or(());
    } else {
        let _ = cl.remove_1("parse-error");
        let _ = cl.remove_1("expanded");
        let _ = cl.add_1("parse-ok");
        if let Some(text_el) = state
            .parse_status
            .query_selector(".parse-status-text")
            .ok()
            .flatten()
        {
            text_el.set_text_content(Some(""));
        }
        if let Some(icon_el) = state
            .parse_status
            .query_selector(".parse-status-icon")
            .ok()
            .flatten()
        {
            icon_el.set_inner_html(r#"<i class="fa-solid fa-check"></i>"#);
        }
        state
            .parse_status
            .set_attribute("title", "Parse OK")
            .unwrap_or(());
    }
}

// ---------------------------------------------------------------------------
// Unified parse + highlight + preview
// ---------------------------------------------------------------------------

/// Parse the editor content once and update both the highlight overlay and the
/// preview pane. On parse error, keeps the last-good highlight and shows an
/// error in the parse status badge.
fn parse_and_update_both(state: &EditorState) {
    let input = state.editor.value();

    match crate::parse_and_render(&input) {
        Ok(result) => {
            // Trailing newline keeps the pre height in sync with the textarea
            // when the last line is empty.
            state
                .highlight
                .set_inner_html(&(result.highlight_html.clone() + "\n"));
            state.preview.set_inner_html(&result.preview_html);
            if result.has_stem {
                typeset_math_preview();
            }
            set_parse_status(state, "OK", false);
        }
        Err(e) => {
            // Show plain escaped text so the input stays visible (the textarea
            // has color:transparent — without matching highlight content the
            // text disappears).
            let escaped = crate::ast_highlight::escape_html(&input);
            state.highlight.set_inner_html(&(escaped + "\n"));
            let msg = format!("Parse error: {e}");
            set_parse_status(state, &msg, true);
        }
    }
}

fn sync_scroll(state: &EditorState) {
    state.backdrop.set_scroll_top(state.editor.scroll_top());
    state.backdrop.set_scroll_left(state.editor.scroll_left());
}

/// Cancel any pending debounce timer and schedule a new parse + update.
fn schedule_parse(state: &EditorState, callback: &js_sys::Function) {
    let prev = state.debounce_handle.get();
    if prev != 0 {
        state.window.clear_timeout_with_handle(prev);
    }
    if let Ok(handle) = state
        .window
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback, DEBOUNCE_MS)
    {
        state.debounce_handle.set(handle);
    }
}

// ---------------------------------------------------------------------------
// Event listener setup (split out to keep `setup` under 100 lines)
// ---------------------------------------------------------------------------

/// Attach the input, scroll, and keydown listeners to the editor textarea.
fn attach_editor_listeners(
    state: &Rc<EditorState>,
    parse_cb: &Closure<dyn Fn()>,
) -> Result<(), JsValue> {
    // Input listener: scroll sync + debounce parse-and-update
    {
        let s = Rc::clone(state);
        let cb = parse_cb
            .as_ref()
            .unchecked_ref::<js_sys::Function>()
            .clone();
        let input_cb: Closure<dyn Fn()> = Closure::new(move || {
            sync_scroll(&s);
            schedule_parse(&s, &cb);
        });
        state
            .editor
            .add_event_listener_with_callback("input", input_cb.as_ref().unchecked_ref())?;
        input_cb.forget();
    }

    // Scroll listener
    {
        let s = Rc::clone(state);
        let scroll_cb: Closure<dyn Fn()> = Closure::new(move || {
            sync_scroll(&s);
        });
        state
            .editor
            .add_event_listener_with_callback("scroll", scroll_cb.as_ref().unchecked_ref())?;
        scroll_cb.forget();
    }

    // Keydown listener — Tab inserts 2 spaces
    {
        let s = Rc::clone(state);
        let parse_fn = parse_cb
            .as_ref()
            .unchecked_ref::<js_sys::Function>()
            .clone();
        let keydown_cb: Closure<dyn Fn(KeyboardEvent)> = Closure::new(move |e: KeyboardEvent| {
            if e.key() == "Tab" {
                e.prevent_default();
                insert_tab(&s);
                sync_scroll(&s);
                schedule_parse(&s, &parse_fn);
            }
        });
        state
            .editor
            .add_event_listener_with_callback("keydown", keydown_cb.as_ref().unchecked_ref())?;
        keydown_cb.forget();
    }

    Ok(())
}

/// Insert 2 spaces at the current cursor position in the textarea.
#[allow(clippy::cast_possible_truncation)] // WASM is 32-bit; textarea indices fit in u32
fn insert_tab(state: &EditorState) {
    let value = state.editor.value();
    let start = state.editor.selection_start().ok().flatten().unwrap_or(0) as usize;
    let end = state.editor.selection_end().ok().flatten().unwrap_or(0) as usize;

    // Use js_sys::JsString for correct UTF-16 slicing (matches JS semantics)
    let js_val: js_sys::JsString = value.into();
    let before = js_val.slice(0, start as u32);
    let after = js_val.slice(end as u32, js_val.length());

    let two_spaces = js_sys::JsString::from("  ");
    let new_val = before.concat(&two_spaces).concat(&after);

    state.editor.set_value(&String::from(&new_val));

    let new_pos = (start + 2) as u32;
    let _ = state.editor.set_selection_start(Some(new_pos));
    let _ = state.editor.set_selection_end(Some(new_pos));
}

/// Attach the click listener to the "File an issue" link.
///
/// Builds a GitHub issue URL pre-filled with the current `AsciiDoc` source and
/// browser info so maintainers can reproduce rendering problems.
fn attach_issue_listener(state: &Rc<EditorState>, doc: &Document) -> Result<(), JsValue> {
    let Some(link) = doc.get_element_by_id("link-issue") else {
        return Ok(());
    };
    let s = Rc::clone(state);
    let link_el: HtmlElement = link.dyn_into()?;

    let issue_cb: Closure<dyn Fn(Event)> = Closure::new(move |e: Event| {
        const MAX_CHARS: usize = 2000;

        e.prevent_default();

        let source = s.editor.value();

        // Truncate to keep the URL within browser limits (~8 kB).
        let truncated_source = if source.chars().count() > MAX_CHARS {
            let trimmed: String = source.chars().take(MAX_CHARS).collect();
            format!("{trimmed}\n\n... (truncated)")
        } else {
            source
        };

        let user_agent = s.window.navigator().user_agent().unwrap_or_default();

        let body = format!(
            "## Description\n\n\
             <!-- Please describe what looks wrong in the preview -->\n\n\
             ## AsciiDoc source\n\n\
             ```asciidoc\n{truncated_source}\n```\n\n\
             ## Environment\n\n\
             - User Agent: `{user_agent}`"
        );

        let title = js_sys::encode_uri_component("Editor: preview rendering issue");
        let encoded_body = js_sys::encode_uri_component(&body);

        let url = format!(
            "https://github.com/nlopes/acdc/issues/new?title={}&body={}&labels=editor",
            String::from(title),
            String::from(encoded_body),
        );

        let _ = s.window.open_with_url_and_target(&url, "_blank");
    });

    link_el.add_event_listener_with_callback("click", issue_cb.as_ref().unchecked_ref())?;
    issue_cb.forget();
    Ok(())
}

/// Attach the click listener to the "Copy HTML" button.
fn attach_copy_listener(state: &Rc<EditorState>, doc: &Document) -> Result<(), JsValue> {
    let Some(btn) = doc.get_element_by_id("btn-copy") else {
        return Ok(());
    };
    let s = Rc::clone(state);
    let copy_btn: HtmlElement = btn.dyn_into()?;
    let copy_el = copy_btn.clone();

    let copy_cb: Closure<dyn Fn(Event)> = Closure::new(move |_: Event| {
        let html = s.preview.inner_html();
        let nav = s.window.navigator();
        let clipboard = nav.clipboard();
        let btn_ref = copy_el.clone();
        let promise = clipboard.write_text(&html);

        let ok_cb = Closure::once(move |_: JsValue| {
            let orig = btn_ref.text_content().unwrap_or_default();
            btn_ref.set_text_content(Some("Copied!"));

            let restore = Closure::once(move || {
                btn_ref.set_text_content(Some(&orig));
            });

            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    restore.as_ref().unchecked_ref(),
                    1500,
                );
            }
            restore.forget();
        });

        let _ = promise.then(&ok_cb);
        ok_cb.forget();
    });

    copy_btn.add_event_listener_with_callback("click", copy_cb.as_ref().unchecked_ref())?;
    copy_cb.forget();
    Ok(())
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Wire up all DOM event listeners for the editor page.
///
/// Called once from `init()` after the panic hook is installed.
pub fn setup() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no global window")?;
    let doc = window.document().ok_or("no document")?;

    let state = Rc::new(EditorState::from_document(&doc, window)?);

    // Set default content + initial render
    state.editor.set_value(DEFAULT_CONTENT);
    parse_and_update_both(&state);

    // Long-lived closure for debounced parse + update
    let parse_state = Rc::clone(&state);
    let parse_cb: Closure<dyn Fn()> = Closure::new(move || {
        parse_and_update_both(&parse_state);
    });

    attach_editor_listeners(&state, &parse_cb)?;
    attach_copy_listener(&state, &doc)?;
    attach_issue_listener(&state, &doc)?;

    // Click-to-expand on error badge
    {
        let s = Rc::clone(&state);
        let badge_cb: Closure<dyn Fn()> = Closure::new(move || {
            let cl = s.parse_status.class_list();
            if cl.contains("parse-error") {
                let _ = cl.toggle("expanded");
            }
        });
        state
            .parse_status
            .add_event_listener_with_callback("click", badge_cb.as_ref().unchecked_ref())?;
        badge_cb.forget();
    }

    // Keep the debounce closure alive for the lifetime of the page
    parse_cb.forget();

    set_parse_status(&state, "OK", false);

    // Populate build info in footer (if the element and git info exist)
    if let (Some(build_info), Some(sha), Some(short_sha)) = (
        doc.get_element_by_id("build-info"),
        option_env!("GIT_SHA"),
        option_env!("GIT_SHORT_SHA"),
    ) {
        build_info.set_inner_html(&format!(
            r#"| Built from commit <a href="https://github.com/nlopes/acdc/commit/{sha}" target="_blank" rel="noopener noreferrer">{short_sha}</a>."#,
        ));
    }

    Ok(())
}
