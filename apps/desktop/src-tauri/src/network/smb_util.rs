//! SMB utility functions.
//!
//! Contains error classification, share filtering, and NDR string parsing utilities.

use crate::network::smb_types::{ShareInfo, ShareListError};
use smb_rpc::interface::ShareInfo1;

/// Checks if an error is an authentication error (including signing requirement).
pub fn is_auth_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("logon failure")
        || lower.contains("access denied")
        || lower.contains("auth")
        || lower.contains("0xc000006d") // STATUS_LOGON_FAILURE
        || lower.contains("signing is required") // SMB signing required
}

/// Classifies an error string into a ShareListError.
pub fn classify_error(err: &str) -> ShareListError {
    let lower = err.to_lowercase();

    if lower.contains("timeout") {
        ShareListError::Timeout(err.to_string())
    } else if lower.contains("no route") || lower.contains("unreachable") || lower.contains("connection refused") {
        ShareListError::HostUnreachable(err.to_string())
    } else if lower.contains("signing is required") || lower.contains("not signed or encrypted") {
        // Server requires SMB signing - guest/anonymous access won't work
        ShareListError::SigningRequired(err.to_string())
    } else if lower.contains("logon failure") || lower.contains("0xc000006d") {
        ShareListError::AuthFailed(err.to_string())
    } else if lower.contains("access denied") || lower.contains("auth") {
        ShareListError::AuthRequired(err.to_string())
    } else {
        ShareListError::ProtocolError(err.to_string())
    }
}

/// Filters raw SMB share info to show only disk shares.
pub fn filter_disk_shares(shares: Vec<ShareInfo1>) -> Vec<ShareInfo> {
    shares
        .into_iter()
        .filter_map(|share| {
            // Get the share name
            let name = extract_share_name(&share);

            // Skip hidden/admin shares (ending with $)
            if name.ends_with('$') {
                return None;
            }

            // Check if it's a disk share (type 0 in SMB)
            let share_type_str = format!("{:?}", share.share_type);
            let is_disk = share_type_str.contains("Disk") || share_type_str.contains("DiskTree");

            if !is_disk {
                return None;
            }

            // Extract comment
            let comment = extract_share_comment(&share);

            Some(ShareInfo {
                name,
                is_disk: true,
                comment,
            })
        })
        .collect()
}

/// Extracts the share name from SMB share info.
pub fn extract_share_name(share: &ShareInfo1) -> String {
    // The netname is an NdrPtr<NdrString<u16>>
    // Use Debug format and clean up
    let debug_str = format!("{:?}", share.netname);
    clean_ndr_string(&debug_str)
}

/// Extracts the comment from SMB share info.
pub fn extract_share_comment(share: &ShareInfo1) -> Option<String> {
    let debug_str = format!("{:?}", share.remark);
    let cleaned = clean_ndr_string(&debug_str);
    if cleaned.is_empty() || cleaned == "None" {
        None
    } else {
        Some(cleaned)
    }
}

/// Cleans up an NDR string from Debug format.
pub fn clean_ndr_string(debug_str: &str) -> String {
    // NDR strings come out as things like:
    // Some(NdrAlign { inner: NdrString("Documents") })
    // We extract just the string content
    if let Some(start) = debug_str.find('"')
        && let Some(end) = debug_str.rfind('"')
        && start < end
    {
        return debug_str[start + 1..end].to_string();
    }
    debug_str.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_ndr_string() {
        assert_eq!(
            clean_ndr_string(r#"Some(NdrAlign { inner: NdrString("Documents") })"#),
            "Documents"
        );
        assert_eq!(clean_ndr_string(r#""Media""#), "Media");
        assert_eq!(clean_ndr_string("None"), "None");
    }

    #[test]
    fn test_is_auth_error() {
        assert!(is_auth_error("Logon Failure (0xc000006d)"));
        assert!(is_auth_error("access denied"));
        assert!(is_auth_error("Authentication failed"));
        assert!(!is_auth_error("Connection refused"));
        assert!(!is_auth_error("Timeout"));
    }

    #[test]
    fn test_classify_error() {
        match classify_error("Timeout after 15s") {
            ShareListError::Timeout(_) => {}
            e => panic!("Expected Timeout, got {:?}", e),
        }

        match classify_error("no route to host") {
            ShareListError::HostUnreachable(_) => {}
            e => panic!("Expected HostUnreachable, got {:?}", e),
        }

        match classify_error("Logon Failure (0xc000006d)") {
            ShareListError::AuthFailed(_) => {}
            e => panic!("Expected AuthFailed, got {:?}", e),
        }
    }
}
