// ── Type mapping ─────────────────────────────────────────────────────

/// A file type filter: regex pattern to match filenames, plus optional flags.
pub struct TypeFilter {
    pub pattern: &'static str,
    pub include_system_dirs: bool,
}

fn filter(pattern: &'static str) -> TypeFilter {
    TypeFilter {
        pattern,
        include_system_dirs: false,
    }
}

fn with_system_dirs(pattern: &'static str) -> TypeFilter {
    TypeFilter {
        pattern,
        include_system_dirs: true,
    }
}

/// Map a `type` enum value to its filename regex pattern and flags.
pub fn type_to_filter(t: &str) -> Option<TypeFilter> {
    Some(match t {
        "photos" => filter(r"\.(jpg|jpeg|png|heic|webp|gif)$"),
        "screenshots" => filter(r"^Screenshot.*\.(png|jpg|heic)$"),
        "videos" => filter(r"\.(mp4|mov|avi|mkv|webm)$"),
        "documents" => filter(r"\.(pdf|doc|docx|txt|odt|xls|xlsx)$"),
        "presentations" => filter(r"\.(ppt|pptx|odp)$"),
        "archives" => filter(r"\.(zip|tar|gz|tgz|bz2|xz|7z|rar)$"),
        "music" => filter(r"\.(mp3|m4a|flac|wav|ogg|aac)$"),
        "code" => filter(r"\.(rs|py|js|ts|go|java|c|cpp|h|rb|swift|svelte|vue)$"),
        "rust" => filter(r"\.rs$"),
        "python" => filter(r"\.py$"),
        "javascript" => filter(r"\.(js|jsx|mjs|cjs)$"),
        "typescript" => filter(r"\.(ts|tsx|mts|cts)$"),
        "go" => filter(r"\.go$"),
        "java" => filter(r"\.java$"),
        "config" => filter(r"\.(json|ya?ml|toml|ini|conf|cfg)$"),
        "logs" => with_system_dirs(r"\.(log|out|err)$"),
        "fonts" => filter(r"\.(ttf|otf|ttc|woff|woff2)$"),
        "databases" => filter(r"\.(sqlite|sqlite3|db)$"),
        "xcode" => filter(r"\.(xcodeproj|xcworkspace|pbxproj)$"),
        "ssh-keys" => filter(r"^(id_(rsa|dsa|ecdsa|ed25519)|authorized_keys|known_hosts)(\.pub)?$"),
        "shell-scripts" => filter(r"\.(sh|bash|zsh)$"),
        "docker-compose" => filter(r"^(docker-compose|compose)\.(yml|yaml)$"),
        "env-files" => filter(r"^\.env(\..+)?$"),
        "none" => return None,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_to_filter_all_enum_values() {
        let types = [
            "photos",
            "screenshots",
            "videos",
            "documents",
            "presentations",
            "archives",
            "music",
            "code",
            "rust",
            "python",
            "javascript",
            "typescript",
            "go",
            "java",
            "config",
            "logs",
            "fonts",
            "databases",
            "xcode",
            "shell-scripts",
            "ssh-keys",
            "docker-compose",
            "env-files",
        ];
        for t in types {
            let f = type_to_filter(t);
            assert!(f.is_some(), "type '{t}' should produce a filter");
            // Verify the pattern compiles as regex
            let f = f.unwrap();
            let re = regex::RegexBuilder::new(f.pattern).case_insensitive(true).build();
            assert!(re.is_ok(), "type '{t}' pattern should compile: {}", f.pattern);
        }
    }

    #[test]
    fn type_logs_includes_system_dirs() {
        let f = type_to_filter("logs").unwrap();
        assert!(f.include_system_dirs);
    }

    #[test]
    fn type_photos_no_system_dirs() {
        let f = type_to_filter("photos").unwrap();
        assert!(!f.include_system_dirs);
    }

    #[test]
    fn type_unknown_returns_none() {
        assert!(type_to_filter("bananas").is_none());
    }

    #[test]
    fn type_none_returns_none() {
        assert!(type_to_filter("none").is_none());
    }

    #[test]
    fn type_screenshots_anchored() {
        let f = type_to_filter("screenshots").unwrap();
        assert!(f.pattern.starts_with('^'));
    }

    #[test]
    fn type_documents_matches_expected_files() {
        let f = type_to_filter("documents").unwrap();
        let re = regex::RegexBuilder::new(f.pattern)
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(re.is_match("report.pdf"));
        assert!(re.is_match("notes.txt"));
        assert!(re.is_match("budget.xlsx"));
        assert!(!re.is_match("photo.jpg"));
        assert!(!re.is_match("code.rs"));
    }

    #[test]
    fn type_shell_scripts_matches() {
        let f = type_to_filter("shell-scripts").unwrap();
        let re = regex::RegexBuilder::new(f.pattern)
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(re.is_match("deploy.sh"));
        assert!(re.is_match("init.bash"));
        assert!(re.is_match("setup.zsh"));
        assert!(!re.is_match("readme.md"));
    }

    #[test]
    fn type_presentations_no_key_extension() {
        let f = type_to_filter("presentations").unwrap();
        assert!(!f.pattern.contains("key"), "presentations should not match .key files");
    }
}
