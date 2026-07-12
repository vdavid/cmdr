//! Folder name suggestions powered by AI (local LLM or Cloud AI provider).
//!
//! Builds a prompt from the current directory listing, calls the configured AI backend,
//! and parses the response into validated folder name suggestions.

use std::collections::HashSet;

use futures_util::StreamExt;
use genai::chat::ChatOptions;
use serde::Serialize;
use tauri::ipc::Channel;

use crate::ai::llm_log::LlmLogContext;
use crate::file_system::get_file_at;

/// Maximum number of file names to include in the prompt context.
const MAX_CONTEXT_ENTRIES: usize = 100;
/// Maximum number of suggestions to return.
const MAX_SUGGESTIONS: usize = 5;

/// Shared system prompt for both streaming and non-streaming suggestion paths.
const SUGGESTION_SYSTEM_PROMPT: &str = "You are a pattern-matching assistant. Carefully observe the style, language, and formatting of existing items, then generate new items that match exactly. Output only what is requested, no formatting or explanation.";

/// Generates folder name suggestions for the given directory.
///
/// Suggestions are a nice-to-have enhancement: every "no backend" case (provider off,
/// missing key, local server not running) silently returns `Ok(Vec::new())`. UI hides
/// the feature instead of surfacing an error.
#[tauri::command]
#[specta::specta]
pub async fn get_folder_suggestions(
    listing_id: String,
    current_path: String,
    include_hidden: bool,
) -> Result<Vec<String>, String> {
    log::debug!(
        "AI suggestions: get_folder_suggestions called for listing={}, path={}",
        listing_id,
        current_path
    );

    let Some(backend) = super::manager::resolve_backend().ready_or_log("AI suggestions") else {
        return Ok(Vec::new());
    };

    get_suggestions_from_backend(&listing_id, &current_path, include_hidden, backend).await
}

/// Gets file names from the listing cache (up to MAX_CONTEXT_ENTRIES).
fn get_file_names(listing_id: &str, include_hidden: bool) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..MAX_CONTEXT_ENTRIES {
        match get_file_at(listing_id, i, include_hidden) {
            Ok(Some(entry)) => names.push(entry.name),
            _ => break,
        }
    }
    names
}

/// Builds the prompt for folder name suggestions.
fn build_prompt(current_path: &str, file_names: &[String]) -> String {
    let contents = file_names.join("\n");

    format!(
        "Suggest {MAX_SUGGESTIONS} new folder names that fit naturally with the existing items. \
         IMPORTANT: Match the naming style exactly - same language, same letter case, same word structure. \
         If existing names are lowercase single words, suggest lowercase single words. \
         If existing names are in a specific language, suggest names in that same language. \
         Output ONLY the folder names, one per line. No numbers, bullets, dashes, markdown, or explanation.\n\
         \n\
         Directory: {current_path}\n\
         Existing items:\n\
         {contents}\n\
         \n\
         {MAX_SUGGESTIONS} folder names:"
    )
}

/// Cleans up a single line from an LLM response into a candidate folder name.
///
/// Strips bullets / numbering / markdown, length-bounds, rejects forbidden chars.
/// Returns `None` for invalid lines (empty, contains `/` or `\0`, > 255 bytes).
/// Does NOT dedupe: that's the caller's job (existing-names check + emit-history).
pub(super) fn sanitize_one_line(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    // Strip leading bullets like "- docs" / "* docs"
    let no_bullet = trimmed.trim_start_matches(['-', '*']).trim_start();
    // Strip leading numbering like "1. docs", "2) docs"
    let no_number = if let Some(rest) = no_bullet.strip_prefix(|c: char| c.is_ascii_digit()) {
        rest.trim_start_matches(['.', ')', ' '])
    } else {
        no_bullet
    };
    // Strip surrounding markdown emphasis (`**bold**`, `*italic*`, `_underline_`, `` `code` ``)
    let cleaned = no_number
        .trim_start_matches(['*', '_', '`'])
        .trim_end_matches(['*', '_', '`']);

    if cleaned.is_empty() {
        return None;
    }
    if cleaned.contains('/') || cleaned.contains('\0') {
        return None;
    }
    if cleaned.len() > 255 {
        return None;
    }
    Some(cleaned.to_owned())
}

/// Parses the LLM response into validated folder name suggestions.
fn parse_suggestions(response: &str, existing_names: &[String]) -> Vec<String> {
    response
        .lines()
        .filter_map(sanitize_one_line)
        .filter(|name| !existing_names.iter().any(|e| e.eq_ignore_ascii_case(name)))
        .take(MAX_SUGGESTIONS)
        .collect()
}

/// Calls the AI backend and returns parsed suggestions.
async fn get_suggestions_from_backend(
    listing_id: &str,
    current_path: &str,
    include_hidden: bool,
    backend: super::client::AiBackend,
) -> Result<Vec<String>, String> {
    let file_names = get_file_names(listing_id, include_hidden);
    let prompt = build_prompt(current_path, &file_names);

    log::debug!("AI suggestions: calling AI with {} files in context", file_names.len());
    log::trace!("AI suggestions: prompt:\n{prompt}");

    let options = ChatOptions::default()
        .with_temperature(0.6)
        .with_max_tokens(150)
        .with_top_p(0.95);

    let backend = backend.with_log_context(LlmLogContext::folder_suggestions());
    match super::client::chat_completion(&backend, SUGGESTION_SYSTEM_PROMPT, &prompt, &options).await {
        Ok(response) => {
            log::trace!("AI suggestions: raw response:\n{response}");
            let suggestions = parse_suggestions(&response, &file_names);
            log::debug!(
                "AI suggestions: got {} suggestions: {:?}",
                suggestions.len(),
                suggestions
            );
            Ok(suggestions)
        }
        Err(e) => {
            log::warn!("AI suggestions: AI call failed: {e}");
            Ok(Vec::new()) // Graceful degradation: return empty on any error
        }
    }
}

// region: --- Streaming variant ----------------------------------------------------

/// Wire-format event for streaming folder suggestions.
///
/// Frontend renders `Suggestion` immediately; treats `Done`/`Cancelled`/`Failed` as
/// stream-end markers. `Failed` carries no message: the error is logged on the Rust
/// side, not surfaced to the user (suggestions are nice-to-have; graceful degrade).
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SuggestionStreamEvent {
    Suggestion { name: String },
    Done,
    Cancelled,
    Failed,
}

/// Stateful sanitizer for streamed chunks.
///
/// `push_chunk` accumulates bytes until it sees `\n`, runs each completed line
/// through [`sanitize_one_line`], dedupes case-insensitively against `existing_names`
/// + previously-emitted, and calls `emit` per accepted suggestion.
///
/// `emit` returns `bool`: `false` halts further processing (caller wants to stop:
/// cancellation, channel send-error, etc.).
pub(super) struct StreamingSanitizer<'a> {
    existing_names: &'a [String],
    emitted_lower: HashSet<String>,
    line_buffer: String,
    suggestions_emitted: usize,
    halted: bool,
}

impl<'a> StreamingSanitizer<'a> {
    pub(super) fn new(existing_names: &'a [String]) -> Self {
        Self {
            existing_names,
            emitted_lower: HashSet::new(),
            line_buffer: String::new(),
            suggestions_emitted: 0,
            halted: false,
        }
    }

    /// Pushes a chunk of streamed text. Calls `emit` for each accepted suggestion.
    /// `emit` returning `false` halts further processing in this and subsequent calls.
    pub(super) fn push_chunk(&mut self, chunk: &str, mut emit: impl FnMut(String) -> bool) {
        if self.halted || self.suggestions_emitted >= MAX_SUGGESTIONS {
            return;
        }
        self.line_buffer.push_str(chunk);
        // Split off completed lines (each `\n` terminates a line). Rust's `split` keeps
        // the last empty segment for trailing `\n` and the in-progress line otherwise.
        while let Some(idx) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..idx].to_owned();
            self.line_buffer.drain(..=idx);
            if !self.try_emit(&line, &mut emit) {
                return;
            }
        }
    }

    /// Flushes the trailing in-progress line (LLMs often skip the final `\n`).
    pub(super) fn finish(&mut self, mut emit: impl FnMut(String) -> bool) {
        if self.halted || self.suggestions_emitted >= MAX_SUGGESTIONS {
            return;
        }
        let line = std::mem::take(&mut self.line_buffer);
        if !line.is_empty() {
            let _ = self.try_emit(&line, &mut emit);
        }
    }

    /// Returns `false` if the caller wants to stop (sets `halted`).
    fn try_emit(&mut self, raw: &str, emit: &mut impl FnMut(String) -> bool) -> bool {
        let Some(name) = sanitize_one_line(raw) else {
            return true;
        };
        let lower = name.to_lowercase();
        if self.emitted_lower.contains(&lower) {
            return true;
        }
        if self.existing_names.iter().any(|e| e.eq_ignore_ascii_case(&name)) {
            return true;
        }
        self.emitted_lower.insert(lower);
        self.suggestions_emitted += 1;
        let want_more = emit(name);
        if !want_more {
            self.halted = true;
            return false;
        }
        if self.suggestions_emitted >= MAX_SUGGESTIONS {
            self.halted = true;
            return false;
        }
        true
    }
}

/// Streams folder name suggestions to the frontend via `Channel`.
///
/// Always returns `Ok(())`. All signaling (suggestions, completion, errors) goes
/// through `on_event`. The IPC `Result` exists only because `#[tauri::command]`
/// requires it. See `ai/CLAUDE.md` § Decisions.
#[tauri::command]
pub async fn stream_folder_suggestions(
    request_id: String,
    listing_id: String,
    current_path: String,
    include_hidden: bool,
    on_event: Channel<SuggestionStreamEvent>,
) -> Result<(), String> {
    log::debug!("AI suggestions stream: start request_id={request_id} listing={listing_id} path={current_path}");

    // Register the cancellation token synchronously BEFORE any await. This closes the
    // race window where `cancel_folder_suggestions` could arrive before registration.
    let token = super::manager::register_stream(&request_id);
    // RAII guard: unregisters even on panic-unwind.
    struct UnregisterGuard<'a>(&'a str);
    impl Drop for UnregisterGuard<'_> {
        fn drop(&mut self) {
            super::manager::unregister_stream(self.0);
        }
    }
    let _guard = UnregisterGuard(&request_id);

    let Some(backend) = super::manager::resolve_backend().ready_or_log("AI suggestions stream") else {
        let _ = on_event.send(SuggestionStreamEvent::Done);
        return Ok(());
    };

    let file_names = get_file_names(&listing_id, include_hidden);
    let prompt = build_prompt(&current_path, &file_names);
    log::debug!(
        "AI suggestions stream: opening stream with {} files in context",
        file_names.len()
    );

    let options = ChatOptions::default()
        .with_temperature(0.6)
        .with_max_tokens(150)
        .with_top_p(0.95);

    let backend = backend.with_log_context(LlmLogContext::folder_suggestions());
    let mut stream =
        match super::client::chat_completion_stream(&backend, SUGGESTION_SYSTEM_PROMPT, &prompt, &options).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!(target: "ai_suggestions", "stream open failed: {e}");
                let _ = on_event.send(SuggestionStreamEvent::Failed);
                return Ok(());
            }
        };

    let mut sanitizer = StreamingSanitizer::new(&file_names);

    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => {
                log::debug!("AI suggestions stream: cancelled (request_id={request_id})");
                let _ = on_event.send(SuggestionStreamEvent::Cancelled);
                return Ok(());
            }
            item = stream.next() => match item {
                None => break,
                Some(Ok(chunk)) => sanitizer.push_chunk(&chunk, |name| {
                    // `Channel::send` returning Err means the webview is gone. Trigger
                    // implicit cancel so the next loop iter unwinds via the cancel arm.
                    match on_event.send(SuggestionStreamEvent::Suggestion { name }) {
                        Ok(()) => true,
                        Err(_) => {
                            token.cancel();
                            false
                        }
                    }
                }),
                Some(Err(e)) => {
                    log::warn!(target: "ai_suggestions", "stream error: {e}");
                    let _ = on_event.send(SuggestionStreamEvent::Failed);
                    return Ok(());
                }
            }
        }
    }

    sanitizer.finish(|name| on_event.send(SuggestionStreamEvent::Suggestion { name }).is_ok());
    let _ = on_event.send(SuggestionStreamEvent::Done);
    log::debug!(
        "AI suggestions stream: done (request_id={request_id}, emitted={})",
        sanitizer.suggestions_emitted
    );
    Ok(())
}

/// Cancels an in-flight `stream_folder_suggestions` call. Idempotent: missing id is
/// a no-op (the stream may have already finished and unregistered itself).
#[tauri::command]
pub fn cancel_folder_suggestions(request_id: String) {
    super::manager::cancel_stream(&request_id);
}

// endregion: --- Streaming variant -------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_includes_path_and_files() {
        let names = vec![String::from("src"), String::from("README.md")];
        let prompt = build_prompt("/home/user/project", &names);

        assert!(prompt.contains("/home/user/project"));
        assert!(prompt.contains("src\nREADME.md"));
        assert!(prompt.contains("Match the naming style exactly"));
        assert!(prompt.contains("same language"));
        assert!(!prompt.contains("- src")); // No bullet points
    }

    #[test]
    fn test_build_prompt_empty_dir() {
        let names: Vec<String> = Vec::new();
        let prompt = build_prompt("/empty", &names);
        assert!(prompt.contains("/empty"));
        assert!(prompt.contains("Existing items:"));
    }

    #[test]
    fn test_parse_suggestions_basic() {
        let response = "docs\ntests\nscripts\nconfig\nassets\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["docs", "tests", "scripts", "config", "assets"]);
    }

    #[test]
    fn test_parse_suggestions_with_numbering() {
        let response = "1. docs\n2. tests\n3. scripts\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["docs", "tests", "scripts"]);
    }

    #[test]
    fn test_parse_suggestions_strips_bullets() {
        let response = "- docs\n- tests\n* scripts\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["docs", "tests", "scripts"]);
    }

    #[test]
    fn test_parse_suggestions_strips_markdown() {
        let response = "**bold-folder**\n*italic-folder*\n`code-folder`\n__underline__\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["bold-folder", "italic-folder", "code-folder", "underline"]);
    }

    #[test]
    fn test_parse_suggestions_numbered_with_markdown() {
        let response = "1. **HighDensityStorage**\n2. **CompressedArchive**\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["HighDensityStorage", "CompressedArchive"]);
    }

    #[test]
    fn test_parse_suggestions_filters_existing() {
        let response = "docs\ntests\nscripts\n";
        let existing = vec![String::from("docs"), String::from("Tests")];
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["scripts"]);
    }

    #[test]
    fn test_parse_suggestions_filters_invalid_chars() {
        let response = "good-name\nbad/name\nalso\0bad\nvalid\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["good-name", "valid"]);
    }

    #[test]
    fn test_parse_suggestions_trims_to_max() {
        let response = "a\nb\nc\nd\ne\nf\ng\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result.len(), MAX_SUGGESTIONS);
    }

    #[test]
    fn test_parse_suggestions_skips_empty_lines() {
        let response = "\n\ndocs\n\ntests\n\n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["docs", "tests"]);
    }

    #[test]
    fn test_parse_suggestions_trims_whitespace() {
        let response = "  docs  \n  tests  \n";
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(response, &existing);
        assert_eq!(result, vec!["docs", "tests"]);
    }

    #[test]
    fn test_parse_suggestions_too_long_name() {
        let long_name = "a".repeat(256);
        let response = format!("{long_name}\nvalid\n");
        let existing: Vec<String> = Vec::new();
        let result = parse_suggestions(&response, &existing);
        assert_eq!(result, vec!["valid"]);
    }

    // --- StreamingSanitizer ---

    fn collect(sanitizer: &mut StreamingSanitizer<'_>, chunks: &[&str]) -> Vec<String> {
        let mut out = Vec::new();
        for c in chunks {
            sanitizer.push_chunk(c, |name| {
                out.push(name);
                true
            });
        }
        sanitizer.finish(|name| {
            out.push(name);
            true
        });
        out
    }

    #[test]
    fn streaming_emits_complete_lines() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["docs\ntests\nscripts\n"]);
        assert_eq!(out, vec!["docs", "tests", "scripts"]);
    }

    #[test]
    fn streaming_handles_chunks_split_mid_line() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["doc", "s\nte", "sts\nscripts\n"]);
        assert_eq!(out, vec!["docs", "tests", "scripts"]);
    }

    #[test]
    fn streaming_handles_empty_chunks() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["", "docs\n", "", "tests\n"]);
        assert_eq!(out, vec!["docs", "tests"]);
    }

    #[test]
    fn streaming_finish_flushes_trailing_line_without_newline() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        // No trailing `\n`; flush should still emit "scripts".
        let out = collect(&mut s, &["docs\ntests\nscripts"]);
        assert_eq!(out, vec!["docs", "tests", "scripts"]);
    }

    #[test]
    fn streaming_skips_existing_names_case_insensitive() {
        let existing = vec![String::from("Docs")];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["docs\ntests\n"]);
        assert_eq!(out, vec!["tests"]);
    }

    #[test]
    fn streaming_dedupes_already_emitted_case_insensitive() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["docs\nDOCS\ntests\n"]);
        assert_eq!(out, vec!["docs", "tests"]);
    }

    #[test]
    fn streaming_strips_per_line_formatting() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["1. **docs**\n- tests\n* `scripts`\n"]);
        assert_eq!(out, vec!["docs", "tests", "scripts"]);
    }

    #[test]
    fn streaming_caps_at_max_suggestions() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["a\nb\nc\nd\ne\nf\ng\n"]);
        assert_eq!(out.len(), MAX_SUGGESTIONS);
    }

    #[test]
    fn streaming_emit_returning_false_halts() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let mut out = Vec::new();
        s.push_chunk("docs\ntests\nscripts\n", |name| {
            out.push(name);
            // Halt after the first emit.
            false
        });
        // After halt, further chunks are no-ops.
        s.push_chunk("more\n", |name| {
            out.push(name);
            true
        });
        assert_eq!(out, vec!["docs"]);
    }

    #[test]
    fn streaming_finish_no_op_on_empty_buffer() {
        let existing: Vec<String> = vec![];
        let mut s = StreamingSanitizer::new(&existing);
        let out = collect(&mut s, &["docs\n"]);
        assert_eq!(out, vec!["docs"]);
    }
}
