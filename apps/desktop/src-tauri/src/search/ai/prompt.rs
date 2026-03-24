//! AI search classification prompt.
//!
//! The classification prompt instructs the LLM to extract structured search
//! parameters from natural language. Rust handles all structural/technical work.

/// Classification prompt for the LLM. The LLM classifies intent into predefined
/// enums and extracts filename keywords. Rust handles all structural/technical work.
/// `{TODAY}` is replaced at runtime.
const CLASSIFICATION_PROMPT: &str = "\
Extract search parameters from the user's file search query.
Return one field per line. Omit fields that don't apply.

keywords:  filename words, space-separated, in the user's language
type:      photos|screenshots|videos|documents|presentations|archives|music|\
code|rust|python|javascript|typescript|go|java|config|logs|fonts|\
databases|xcode|shell-scripts|ssh-keys|docker-compose|env-files|none
time:      today|yesterday|this_week|last_week|this_month|last_month|\
this_quarter|last_quarter|this_year|last_year|last_3_months|last_6_months|\
recent|old|YYYY|YYYY..YYYY
size:      empty|tiny|small|large|huge|>NUMBERmb|>NUMBERgb|<NUMBERmb
scope:     downloads|documents|desktop|dotfiles|PATH
exclude:   dirname1 dirname2
folders:   yes|no
note:      brief limitation caveat if query involves unfilterable concepts

Rules:
- \"keywords\" = words likely in FILENAMES. Not descriptions.
- Use singular forms for keywords (contract, not contracts).
- \"I name them X\" / \"I mark them as X\" → keywords: X (not the descriptive words)
- Only set `time` when the user explicitly mentions a time period (yesterday, last week, recent, 2024, etc.). Never default to recent/today.
- Prefer `type` over `keywords` for well-known file categories. Don't put the type name in keywords.
- Don't put the file format in keywords when using a type. \"PDF documents\" → type: documents. \"sqlite databases\" → type: databases.
- If the user wants ONLY a specific format (not all files of that category), use the format as keyword without type: \"HEIC photos I haven't converted\" → keywords: .heic / note: can't determine conversion status
- \"not in X\" / \"but not in X\" / \"excluding X\" / \"except in X\" → ALWAYS use exclude: X
- \"ssh keys\"/\"env files\"/\"docker compose\"/\"shell scripts\" → type handles this, no keywords needed
- For content/semantic queries (\"photos of my cat\"), set type + add a note

Examples:
\"recent invoices, I mark them rymd\" → keywords: rymd / type: documents / time: recent
\"\u{5927}\u{304d}\u{306a}\u{52d5}\u{753b}\u{3092}\u{524a}\u{9664}\u{3057}\u{305f}\u{3044}\" → type: videos / size: large / note: can't determine safe to delete
\"node_modules folders taking up space\" → keywords: node_modules / folders: yes / size: large
\"screenshots from this week\" → type: screenshots / time: this_week
\"package.json not in node_modules\" → keywords: package.json / exclude: node_modules
\"empty folders\" → folders: yes / size: empty
\"ssh keys\" → type: ssh-keys
\"foton fr\u{00e5}n f\u{00f6}rra veckan\" → type: photos / time: last_week
\"that rust file with the websocket server\" → keywords: websocket / type: rust
\"old xcode projects\" → type: xcode / time: old
\"contracts I signed in the last 6 months\" → keywords: contract / type: documents / time: last_6_months / note: \"signed\" is not filterable
\"shell scripts in my dotfiles\" → type: shell-scripts / scope: dotfiles
\"HEIC photos I haven't converted\" → keywords: .heic / note: can't determine conversion status

Today: {TODAY}.";

pub fn build_classification_prompt() -> String {
    let today = time::OffsetDateTime::now_utc().date();
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let today_str = today.format(&format).expect("date format always succeeds");
    CLASSIFICATION_PROMPT.replace("{TODAY}", &today_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classification_prompt_contains_date() {
        let prompt = build_classification_prompt();
        assert!(prompt.contains("Today:"));
        assert!(prompt.contains("Extract search parameters"));
        // Should contain a date in YYYY-MM-DD format
        assert!(prompt.contains("20")); // Year starts with 20
    }

    #[test]
    fn test_classification_prompt_contains_type_enums() {
        let prompt = build_classification_prompt();
        assert!(prompt.contains("photos|screenshots|videos"));
        assert!(prompt.contains("shell-scripts|ssh-keys|docker-compose|env-files"));
    }

    #[test]
    fn test_classification_prompt_contains_time_enums() {
        let prompt = build_classification_prompt();
        assert!(prompt.contains("last_3_months|last_6_months"));
    }
}
