//! Reading what `smbutil view` printed: its stderr into an outcome, its share table into
//! shares. Pure text work, so it's compiled for the tests on every OS as well as for the macOS
//! arm that runs the command.

use cmdr_smb::ShareInfo;

/// Outcome of classifying smbutil's stderr output.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SmbutilOutcome {
    /// Authentication error: smbutil explicitly reported credentials were rejected.
    AuthRequired,
    /// Any other failure: include the stderr message as context.
    ProtocolError(String),
}

/// Classifies smbutil's stderr text into a typed outcome.
///
/// smbutil has no structured error output; string matching is unavoidable here.
/// LC_ALL=C / LANG=C is forced in `run_smbutil_view` so these strings are always
/// in English regardless of the user's locale.
pub(crate) fn classify_smbutil_stderr(stderr: &str) -> SmbutilOutcome {
    // allowed-error-string-match: smbutil CLI has no structured error output; LC_ALL=C pins English
    if stderr.contains("Authentication error") || stderr.contains("rejected the authentication") {
        SmbutilOutcome::AuthRequired
    } else {
        SmbutilOutcome::ProtocolError(stderr.trim().to_string())
    }
}

/// Parses smbutil view output to extract share information.
/// Example output:
/// ```text
/// Share                                           Type    Comments
/// -------------------------------
/// public                                          Disk
/// Documents                                       Disk    My documents
/// ```
pub(crate) fn parse_smbutil_output(output: &str) -> Vec<ShareInfo> {
    // Known smbutil share type keywords, used to locate the type column in each line.
    const TYPE_KEYWORDS: &[&str] = &["Disk", "Pipe", "Printer"];

    let mut shares = Vec::new();
    let mut in_shares_section = false;

    for line in output.lines() {
        // Detect header line
        if line.starts_with("Share") && line.contains("Type") {
            in_shares_section = true;
            continue;
        }
        if line.starts_with("---") {
            continue;
        }
        if line.contains("shares listed") {
            break;
        }

        if !in_shares_section {
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        // Find the type keyword in the line. smbutil pads the name with spaces before the type,
        // so we look for "  Disk", "  Pipe", or "  Printer" (preceded by at least two spaces).
        // This handles share names with spaces and multi-byte unicode characters correctly.
        let mut found_type_pos = None;
        let mut found_type_keyword = "";
        for keyword in TYPE_KEYWORDS {
            // Look for the keyword preceded by whitespace (at least 2 spaces before it)
            let search = format!("  {}", keyword);
            if let Some(pos) = line.rfind(&search) {
                let type_start = pos + 2; // skip the two leading spaces
                found_type_pos = Some(type_start);
                found_type_keyword = keyword;
                break;
            }
        }

        let (name, share_type, comment) = if let Some(type_start) = found_type_pos {
            let name = line[..type_start].trim_end().to_string();
            let after_type = &line[type_start + found_type_keyword.len()..];
            let comment_text = after_type.trim();
            let cmt = if comment_text.is_empty() {
                None
            } else {
                Some(comment_text.to_string())
            };
            (name, found_type_keyword.to_lowercase(), cmt)
        } else {
            // No known type keyword found; skip this line
            continue;
        };

        if name.is_empty() {
            continue;
        }

        // Skip hidden shares (ending with $) and non-disk shares
        if name.ends_with('$') {
            continue;
        }
        if share_type != "disk" {
            continue;
        }

        shares.push(ShareInfo {
            name,
            is_disk: true,
            comment,
        });
    }

    shares
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn parse_multiple_shares_filters_hidden_and_non_disk() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Public                                          Disk    System default share
Web                                             Disk
Multimedia                                      Disk    System default share
IPC$                                            Pipe    IPC Service (NAS Server)
home                                            Disk    Home
ADMIN$                                          Disk    Admin share

6 shares listed
";
        let shares = parse_smbutil_output(output);

        assert_eq!(shares.len(), 4);
        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Public"));
        assert!(names.contains(&"Web"));
        assert!(names.contains(&"Multimedia"));
        assert!(names.contains(&"home"));
        assert!(!names.contains(&"IPC$"));
        assert!(!names.contains(&"ADMIN$"));
        assert!(shares.iter().all(|s| s.is_disk));

        let public = shares.iter().find(|s| s.name == "Public").unwrap();
        assert_eq!(public.comment.as_deref(), Some("System default share"));

        let web = shares.iter().find(|s| s.name == "Web").unwrap();
        assert!(web.comment.is_none());
    }

    #[test]
    fn parse_empty_output_returns_no_shares() {
        // Header + separator but no share lines
        let output = "\
Share                                           Type    Comments
-------------------------------

0 shares listed
";
        let shares = parse_smbutil_output(output);
        assert!(shares.is_empty());
    }

    #[test]
    fn parse_completely_empty_string() {
        assert!(parse_smbutil_output("").is_empty());
    }

    #[test]
    fn parse_single_share() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Backups                                         Disk    Time Machine backups

1 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "Backups");
        assert_eq!(shares[0].comment.as_deref(), Some("Time Machine backups"));
        assert!(shares[0].is_disk);
    }

    #[test]
    fn parse_share_names_with_spaces() {
        let output = "\
Share                                           Type    Comments
-------------------------------
My Documents                                    Disk    Personal files
Shared Media Files                              Disk
Time Machine                                    Disk    TM backups

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 3);

        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"My Documents"));
        assert!(names.contains(&"Shared Media Files"));
        assert!(names.contains(&"Time Machine"));

        let docs = shares.iter().find(|s| s.name == "My Documents").unwrap();
        assert_eq!(docs.comment.as_deref(), Some("Personal files"));
    }

    #[test]
    fn parse_unicode_share_names() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Données                                         Disk    French data
共有フォルダ                                     Disk    Japanese folder
Müsik                                           Disk

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 3);

        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Données"));
        assert!(names.contains(&"共有フォルダ"));
        assert!(names.contains(&"Müsik"));
    }

    #[test]
    fn parse_special_characters_in_names() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Music & Videos                                  Disk    Media collection
Work (2024)                                     Disk
Tom's Files                                     Disk    Personal

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 3);

        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Music & Videos"));
        assert!(names.contains(&"Work (2024)"));
        assert!(names.contains(&"Tom's Files"));
    }

    #[test]
    fn parse_output_without_trailing_summary() {
        // Some macOS versions may omit the "N shares listed" line
        let output = "\
Share                                           Type    Comments
-------------------------------
Photos                                          Disk    Family photos
Videos                                          Disk
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].name, "Photos");
        assert_eq!(shares[1].name, "Videos");
    }

    #[test]
    fn parse_only_ipc_and_printer_shares_returns_empty() {
        let output = "\
Share                                           Type    Comments
-------------------------------
IPC$                                            Pipe    IPC Service
print$                                          Printer Printer Drivers
Canon MF640                                     Printer Office printer

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert!(shares.is_empty());
    }

    #[test]
    fn parse_preamble_text_before_header_is_ignored() {
        // smbutil may print server info before the share table
        let output = "\
OS (PC Network Program 1.0)
Lanman (Windows for Workgroups 3.1a)
Share                                           Type    Comments
-------------------------------
Documents                                       Disk

1 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "Documents");
    }

    // ── classify_smbutil_stderr tests ──────────────────────────────────

    #[test]
    fn classify_authentication_error_phrase() {
        assert_eq!(
            classify_smbutil_stderr("Authentication error\n"),
            SmbutilOutcome::AuthRequired
        );
    }

    #[test]
    fn classify_rejected_the_authentication_phrase() {
        assert_eq!(
            classify_smbutil_stderr("server rejected the authentication\n"),
            SmbutilOutcome::AuthRequired
        );
    }

    #[test]
    fn classify_protocol_error_for_unknown_stderr() {
        assert_eq!(
            classify_smbutil_stderr("  connection refused  "),
            SmbutilOutcome::ProtocolError("connection refused".to_string())
        );
    }
}
