//! Folder name suggestions powered by the local LLM.
//!
//! Builds a prompt from the current directory listing, calls the LLM,
//! and parses the response into validated folder name suggestions.

use crate::file_system::get_file_at;

/// Maximum number of file names to include in the prompt context.
const MAX_CONTEXT_ENTRIES: usize = 100;
/// Maximum number of suggestions to return.
const MAX_SUGGESTIONS: usize = 5;

/// Mock suggestions returned in dev mode.
#[cfg(debug_assertions)]
const MOCK_SUGGESTIONS: &[&str] = &["docs", "tests", "scripts", "config", "assets"];

/// Generates folder name suggestions for the given directory.
///
/// In dev mode, returns hardcoded mock suggestions.
/// In release mode, calls the local llama-server.
#[tauri::command]
pub async fn get_folder_suggestions(
    listing_id: String,
    current_path: String,
    include_hidden: bool,
) -> Result<Vec<String>, String> {
    #[cfg(debug_assertions)]
    {
        let _ = &current_path; // Only used in release mode (for the LLM prompt)
        // In dev mode, return mock suggestions filtered against existing names
        let existing = get_file_names(&listing_id, include_hidden);
        let suggestions: Vec<String> = MOCK_SUGGESTIONS
            .iter()
            .filter(|name| !existing.contains(&name.to_string()))
            .take(MAX_SUGGESTIONS)
            .map(|s| s.to_string())
            .collect();
        Ok(suggestions)
    }

    #[cfg(not(debug_assertions))]
    {
        get_suggestions_from_llm(&listing_id, &current_path, include_hidden).await
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
#[cfg(any(not(debug_assertions), test))]
fn build_prompt(current_path: &str, file_names: &[String]) -> String {
    let mut contents = String::new();
    for name in file_names {
        contents.push_str("- ");
        contents.push_str(name);
        contents.push('\n');
    }

    format!(
        "You are a file organization assistant. Given the contents of a directory, suggest {MAX_SUGGESTIONS} new folder \
         names that would make sense to create here. Consider the existing structure, naming conventions, and common \
         project patterns.\n\
         \n\
         Current directory: {current_path}\n\
         Contents:\n\
         {contents}\n\
         Respond with exactly {MAX_SUGGESTIONS} folder names, one per line, no numbering, no explanation."
    )
}

/// Parses the LLM response into validated folder name suggestions.
#[cfg(any(not(debug_assertions), test))]
fn parse_suggestions(response: &str, existing_names: &[String]) -> Vec<String> {
    response
        .lines()
        .map(|line| line.trim().to_string())
        // Remove any accidental numbering (e.g., "1. docs" → "docs")
        .map(|line| {
            if let Some(rest) = line.strip_prefix(|c: char| c.is_ascii_digit()) {
                rest.trim_start_matches(['.', ')', ' ']).to_string()
            } else {
                line
            }
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

/// Calls the LLM and returns parsed suggestions.
#[cfg(not(debug_assertions))]
async fn get_suggestions_from_llm(
    listing_id: &str,
    current_path: &str,
    include_hidden: bool,
) -> Result<Vec<String>, String> {
    let port = super::manager::get_port().ok_or("AI server not running")?;
    let file_names = get_file_names(listing_id, include_hidden);
    let prompt = build_prompt(current_path, &file_names);

    match super::client::chat_completion(port, &prompt).await {
        Ok(response) => Ok(parse_suggestions(&response, &file_names)),
        Err(_) => Ok(Vec::new()), // Graceful degradation: return empty on any error
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
        assert!(prompt.contains("- src"));
        assert!(prompt.contains("- README.md"));
        assert!(prompt.contains("5 folder names"));
    }

    #[test]
    fn test_build_prompt_empty_dir() {
        let names: Vec<String> = Vec::new();
        let prompt = build_prompt("/empty", &names);
        assert!(prompt.contains("/empty"));
        assert!(prompt.contains("Contents:"));
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
