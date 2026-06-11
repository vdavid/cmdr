//! AI selection classification prompt.
//!
//! The prompt asks the cloud LLM to read a sample of the current folder's filenames
//! and translate a user's natural-language intent into the smallest glob or regex
//! that selects the matching files, plus optional size and date filters.
//!
//! Why key-value output and not JSON: same reason `crate::search::ai::prompt` does
//! it. Missing lines are individually skippable; malformed JSON would void the whole
//! call. Key-value is also cheaper to parse and read in logs.
//!
//! Why a folder sample at all: the user's intent often references filename
//! conventions the model can't infer cold ("all rymd files", "everything I named
//! `Final-*`"). The sample grounds the pattern in what's actually in the folder.
//!
//! ## Eval log
//!
//! The prompt was iterated against the configured cloud model in
//! `Settings > AI > Cloud > OpenAI` using the running app's `translate_selection_query`
//! IPC. Each sample was the focused folder's listing plus the user's prompt; the
//! agent recorded the model's response and the resulting parsed pattern.
//!
//! The full eval log lives in `src-tauri/src/selection/CLAUDE.md` § "Real-LLM eval
//! results" so we can re-run it when the prompt changes or when a new model lands.

/// Classification prompt for the selection LLM. `{TODAY}` and `{SAMPLE}` are replaced
/// at runtime.
///
/// Design notes:
/// - We ask for `pattern` + `kind` (`glob` or `regex`); the matcher runs on the
///   frontend so we don't need a structured filter type. `glob` accepts `*` / `?`
///   only (anchored full-name match); anything else should come back as `regex`.
/// - Size filters use byte ranges (`size_min`/`size_max`); the model is told to
///   spell bytes out so we don't have to parse `mb`/`gb` suffixes again.
/// - Date filters use ISO `YYYY-MM-DD`; the parser converts to unix seconds.
/// - `note` is a short caveat the UI surfaces in the AI transparency strip when the
///   intent has an unfilterable component (for example "files I never opened").
/// - `label` is a short, sentence-case title (≤40 chars) the dialog uses if it ever
///   wants to render a breadcrumb-style summary of the selection.
const CLASSIFICATION_PROMPT: &str = "\
You're helping a user select files inside one folder. Below is a sample of the folder's filenames.
Return one field per line. Omit fields that don't apply. Don't add explanation text.

pattern:        smallest glob or regex that selects the user's files; full-name match (no path)
kind:           glob | regex
type:           file | folder (OMIT unless the intent is clearly only files or only folders)
size_min:       minimum size in bytes (integer)
size_max:       maximum size in bytes (integer)
modified_after: ISO date YYYY-MM-DD (file modified on or after)
modified_before:ISO date YYYY-MM-DD (file modified strictly before)
note:           short caveat (max 200 chars) when intent has unfilterable parts
label:          short sentence-case title for this selection, max 40 chars, no trailing punctuation

Rules:
- \"glob\" supports only `*` (any chars) and `?` (one char). Anchored to the full filename.
- If the intent needs character classes, alternation, or anchors, use `regex` (JavaScript flavor).
- Patterns match the filename, not the full path. Don't include `/`.
- Don't escape glob metacharacters; the matcher anchors the pattern automatically.
- Prefer the broadest reasonable pattern. Don't list every filename individually.
- When the intent is purely a size, date, or type filter, set `pattern` to `*` so the matcher selects every name.
- When the intent has no time component, omit `modified_after` and `modified_before`. Never default to recent.
- When the intent has no size component, omit `size_min` and `size_max`.
- `type` is OPTIONAL. The user's current choice is shown below. OMIT `type` unless they clearly want only files \
(\"the pdf files\", \"just the documents\") or only folders (\"the subfolders\", \"empty directories\"). \
A bare \"all images\" is files; \"node_modules folders\" is folder. When in doubt, omit it.

Examples (sample lines elided for brevity):
\"all log files\" \u{2192} pattern: *.log / kind: glob / label: All log files
\"png and jpg images\" \u{2192} pattern: *.(png|jpg|jpeg) / kind: regex / label: PNG and JPG images
\"files bigger than 5 MB\" \u{2192} pattern: * / kind: glob / size_min: 5242880 / label: Files bigger than 5 MB
\"the subfolders\" \u{2192} pattern: * / kind: glob / type: folder / label: Subfolders
\"empty files\" \u{2192} pattern: * / kind: glob / type: file / size_min: 0 / size_max: 0 / label: Empty files
\"backups from last week\" \u{2192} pattern: *backup* / kind: glob / modified_after: {WEEK_AGO} / label: Recent backups
\"final drafts I haven't shared\" \u{2192} pattern: *final* / kind: glob / note: can't tell shared status / label: Final drafts
\"every rymd file\" \u{2192} pattern: *rymd* / kind: glob / label: Rymd files
\"files matching IMG_2024\" \u{2192} pattern: IMG_2024* / kind: glob / label: IMG_2024 files

Sample of the folder's filenames:
{SAMPLE}

Current type filter: {CURRENT_TYPE}.
Today: {TODAY}.";

/// Builds the full classification prompt with today's date, the supplied folder sample, and
/// the user's current type filter as context. The sample is rendered one filename per line,
/// prefixed with `- `, after a deduplicating pass; passing more than `MAX_SAMPLE` lines is
/// allowed but the prompt truncates to keep token cost predictable.
///
/// `current_type`: `Some(true)` = folders, `Some(false)` = files, `None` = both. The model is
/// told it may keep or change this; an omitted `type` in the response means "keep the user's
/// choice" downstream.
pub fn build_classification_prompt(sample_names: &[String], current_type: Option<bool>) -> String {
    let today = time::OffsetDateTime::now_utc().date();
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let today_str = today.format(&format).expect("date format always succeeds");

    let week_ago = today.saturating_sub(time::Duration::days(7));
    let week_ago_str = week_ago.format(&format).expect("date format always succeeds");

    let sample = format_sample_block(sample_names);

    CLASSIFICATION_PROMPT
        .replace("{TODAY}", &today_str)
        .replace("{WEEK_AGO}", &week_ago_str)
        .replace("{CURRENT_TYPE}", current_type_label(current_type))
        .replace("{SAMPLE}", &sample)
}

/// Renders the current type filter for the prompt context line.
fn current_type_label(current_type: Option<bool>) -> &'static str {
    match current_type {
        Some(true) => "folders only",
        Some(false) => "files only",
        None => "both files and folders",
    }
}

/// Cap on how many filenames we paste into the prompt. The frontend sampler usually
/// returns ≤240 names already; this is a defensive backstop that keeps the prompt
/// under ~10k tokens even if a buggy caller passes thousands.
const MAX_SAMPLE: usize = 240;

/// Renders the sample names as one-per-line bullet list. De-dupes while preserving
/// order (first occurrence wins) and truncates at `MAX_SAMPLE`.
pub fn format_sample_block(names: &[String]) -> String {
    if names.is_empty() {
        // Make it obvious to the model so it doesn't hallucinate a folder layout.
        return "(empty folder)".to_string();
    }

    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out = String::new();
    let mut count = 0;
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !seen.insert(trimmed) {
            continue;
        }
        out.push_str("- ");
        out.push_str(trimmed);
        out.push('\n');
        count += 1;
        if count >= MAX_SAMPLE {
            out.push_str("... (sample truncated)\n");
            break;
        }
    }
    // Strip the trailing newline so the prompt doesn't end with a blank line.
    if out.ends_with('\n') {
        out.pop();
    }
    // If every input was blank, `out` is still empty; surface the same signal as the
    // all-empty branch above so the model doesn't try to read a phantom listing.
    if out.is_empty() {
        return "(empty folder)".to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_today_date() {
        let prompt = build_classification_prompt(&["a.log".into(), "b.txt".into()], None);
        assert!(prompt.contains("Today:"));
        // Year always starts with "20" for the foreseeable future.
        assert!(prompt.contains("20"));
    }

    #[test]
    fn prompt_inlines_sample_block() {
        let prompt = build_classification_prompt(&["alpha.png".into(), "beta.jpg".into()], None);
        assert!(prompt.contains("- alpha.png"));
        assert!(prompt.contains("- beta.jpg"));
        assert!(prompt.contains("Sample of the folder's filenames:"));
    }

    #[test]
    fn prompt_substitutes_week_ago_placeholder() {
        let prompt = build_classification_prompt(&[], None);
        // The {WEEK_AGO} placeholder should be replaced so the example reads as a real date.
        assert!(!prompt.contains("{WEEK_AGO}"));
        assert!(!prompt.contains("{TODAY}"));
        assert!(!prompt.contains("{SAMPLE}"));
        assert!(!prompt.contains("{CURRENT_TYPE}"));
    }

    #[test]
    fn prompt_lists_required_fields() {
        let prompt = build_classification_prompt(&[], None);
        for field in [
            "pattern:",
            "kind:",
            "type:",
            "size_min:",
            "size_max:",
            "modified_after:",
            "modified_before:",
            "label:",
        ] {
            assert!(prompt.contains(field), "prompt missing field {field:?}");
        }
    }

    #[test]
    fn prompt_renders_current_type_context() {
        assert!(build_classification_prompt(&[], None).contains("Current type filter: both files and folders"));
        assert!(build_classification_prompt(&[], Some(true)).contains("Current type filter: folders only"));
        assert!(build_classification_prompt(&[], Some(false)).contains("Current type filter: files only"));
    }

    #[test]
    fn format_sample_block_dedupes_preserving_order() {
        let names = vec!["a.txt".into(), "b.txt".into(), "a.txt".into(), "c.txt".into()];
        let out = format_sample_block(&names);
        assert_eq!(out, "- a.txt\n- b.txt\n- c.txt");
    }

    #[test]
    fn format_sample_block_skips_blank_entries() {
        let names = vec!["a.txt".into(), "   ".into(), "".into(), "b.txt".into()];
        let out = format_sample_block(&names);
        assert_eq!(out, "- a.txt\n- b.txt");
    }

    #[test]
    fn format_sample_block_truncates_long_inputs() {
        let names: Vec<String> = (0..500).map(|i| format!("file-{i}.txt")).collect();
        let out = format_sample_block(&names);
        // Last line should mention truncation; line count = MAX_SAMPLE + 1 truncation note.
        assert!(out.contains("... (sample truncated)"));
        let line_count = out.lines().count();
        assert_eq!(line_count, MAX_SAMPLE + 1);
    }

    #[test]
    fn format_sample_block_signals_empty_folder() {
        assert_eq!(format_sample_block(&[]), "(empty folder)");
        // Whitespace-only entries are equivalent to empty.
        assert_eq!(format_sample_block(&["   ".into(), "".into()]), "(empty folder)");
    }
}
