//! Folder name suggestions powered by AI (local LLM or OpenAI-compatible API).
//!
//! Builds a prompt from the current directory listing, calls the configured AI backend,
//! and parses the response into validated folder name suggestions.

use super::client::AiBackend;
use crate::file_system::get_file_at;

/// Maximum number of file names to include in the prompt context.
const MAX_CONTEXT_ENTRIES: usize = 100;
/// Maximum number of suggestions to return.
const MAX_SUGGESTIONS: usize = 5;

/// Generates folder name suggestions for the given directory.
///
/// Routes based on the configured AI provider:
/// - `off`: returns empty
/// - `local`: uses local llama-server (if running)
/// - `openai-compatible`: uses remote OpenAI-compatible API
#[tauri::command]
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

    let provider = super::manager::get_provider();
    match provider.as_str() {
        "off" => {
            log::debug!("AI suggestions: provider is off, returning empty");
            Ok(Vec::new())
        }
        "local" => get_suggestions_from_backend(&listing_id, &current_path, include_hidden, None).await,
        "openai-compatible" => {
            let (api_key, base_url, model) = super::manager::get_openai_config();
            if api_key.is_empty() {
                log::debug!("AI suggestions: OpenAI API key not configured, returning empty");
                return Ok(Vec::new());
            }
            let backend = AiBackend::OpenAi {
                api_key,
                base_url,
                model,
            };
            get_suggestions_from_backend(&listing_id, &current_path, include_hidden, Some(backend)).await
        }
        _ => {
            log::debug!("AI suggestions: unknown provider '{provider}', returning empty");
            Ok(Vec::new())
        }
    }
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

/// Parses the LLM response into validated folder name suggestions.
fn parse_suggestions(response: &str, existing_names: &[String]) -> Vec<String> {
    response
        .lines()
        .map(|line| line.trim().to_string())
        // Remove any accidental bullet points (like "- docs" → "docs")
        .map(|line| line.trim_start_matches(['-', '*']).trim_start().to_string())
        // Remove any accidental numbering (like "1. docs" → "docs")
        .map(|line| {
            if let Some(rest) = line.strip_prefix(|c: char| c.is_ascii_digit()) {
                rest.trim_start_matches(['.', ')', ' ']).to_string()
            } else {
                line
            }
        })
        // Remove markdown formatting (bold, italic, backticks)
        .map(|line| {
            line.trim_start_matches(['*', '_', '`'])
                .trim_end_matches(['*', '_', '`'])
                .to_string()
        })
        .filter(|name| !name.is_empty())
        .filter(|name| !name.contains('/') && !name.contains('\0'))
        .filter(|name| name.len() <= 255)
        .filter(|name| {
            !existing_names
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
        })
        .take(MAX_SUGGESTIONS)
        .collect()
}

/// Calls the AI backend and returns parsed suggestions.
/// If `backend` is None, uses the local llama-server (looks up port from manager state).
async fn get_suggestions_from_backend(
    listing_id: &str,
    current_path: &str,
    include_hidden: bool,
    backend: Option<AiBackend>,
) -> Result<Vec<String>, String> {
    let backend = match backend {
        Some(b) => b,
        None => {
            let port = match super::manager::get_port() {
                Some(p) => p,
                None => {
                    log::debug!("AI suggestions: local server not running (no port)");
                    return Ok(Vec::new());
                }
            };
            AiBackend::Local { port }
        }
    };

    let file_names = get_file_names(listing_id, include_hidden);
    let prompt = build_prompt(current_path, &file_names);

    log::debug!("AI suggestions: calling AI with {} files in context", file_names.len());
    log::trace!("AI suggestions: prompt:\n{prompt}");

    match super::client::chat_completion(&backend, &prompt).await {
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
}
