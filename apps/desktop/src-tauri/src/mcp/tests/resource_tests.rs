use crate::mcp::archive_password::{ArchivePasswordPrompt, ArchivePromptMode};
use crate::mcp::dialog_state::KnownDialog;
use crate::mcp::resources::{format_archive_password_dialog, format_available_dialogs_yaml, get_all_resources};

#[test]
fn dialogs_available_carries_registered_descriptions() {
    // Every FE-registered soft dialog renders its `dialog-registry.ts` description in
    // cmdr://dialogs/available; a dialog without one renders just its type line. This
    // pins the description round-trip the dogfooding flagged as inconsistent.
    let known = vec![
        KnownDialog {
            id: "whats-new".to_string(),
            description: Some("Post-update changelog summary popup".to_string()),
            blocks_operations: true,
        },
        KnownDialog {
            id: "about".to_string(),
            description: None,
            blocks_operations: true,
        },
    ];
    let yaml = format_available_dialogs_yaml(&known);

    // Window-based types are always present.
    assert!(yaml.contains("- type: settings"));
    assert!(yaml.contains("- type: file-viewer"));
    // A described soft dialog carries its description line.
    assert!(yaml.contains("- type: whats-new\n  description: Post-update changelog summary popup\n"));
    // A description-less one renders the type line with no description.
    assert!(yaml.contains("- type: about\n"));
    assert!(!yaml.contains("- type: about\n  description:"));
}

/// A prompt as the archive-password flow would mirror it.
fn prompt(mode: ArchivePromptMode, wrong_attempt: bool) -> ArchivePasswordPrompt {
    ArchivePasswordPrompt {
        archive_name: "photos.zip".to_string(),
        archive_path: "/tmp/left/photos.zip/holiday.raw".to_string(),
        parent_volume_id: "drive-1".to_string(),
        mode,
        wrong_attempt,
        operation_id: Some("op-7".to_string()),
    }
}

#[test]
fn the_archive_password_entry_carries_everything_an_answer_needs() {
    // The whole point of the block: a bare `- type: archive-password` left an
    // agent unable to say which archive it was answering, or with what.
    let yaml = format_archive_password_dialog(&prompt(ArchivePromptMode::Transfer, false));

    assert!(yaml.contains("- type: archive-password"), "{yaml}");
    assert!(yaml.contains("archive: \"photos.zip\""), "{yaml}");
    assert!(
        yaml.contains("archivePath: \"/tmp/left/photos.zip/holiday.raw\""),
        "{yaml}"
    );
    assert!(yaml.contains("mode: transfer"), "{yaml}");
    assert!(yaml.contains("wrongAttempt: false"), "{yaml}");
    // The operation is already settled, so the name says so rather than
    // inviting a `queue` call that would find nothing.
    assert!(yaml.contains("settledOperationId: op-7"), "{yaml}");
    assert!(yaml.contains("answerWith: unlock_archive"), "{yaml}");
}

#[test]
fn a_rejected_password_is_visible_as_a_typed_flag() {
    // Without this an agent can't tell its answer was tried and refused from a
    // first ask, and loops on the same wrong password.
    let yaml = format_archive_password_dialog(&prompt(ArchivePromptMode::Browse, true));
    assert!(yaml.contains("mode: browse"), "{yaml}");
    assert!(yaml.contains("wrongAttempt: true"), "{yaml}");
}

#[test]
fn a_browse_prompt_names_no_operation() {
    // Nothing was started, so a `settledOperationId` line would be a fiction.
    let mut browse = prompt(ArchivePromptMode::Browse, false);
    browse.operation_id = None;
    assert!(!format_archive_password_dialog(&browse).contains("settledOperationId"));
}

#[test]
fn the_entry_never_renders_the_password_or_the_volume_id() {
    // The mirror holds the QUESTION. The password never reaches the store at
    // all, and the volume id is the backend's business: an answer names the
    // archive, and the backend supplies the rest.
    let yaml = format_archive_password_dialog(&prompt(ArchivePromptMode::Transfer, true));
    assert!(!yaml.contains("password:"), "{yaml}");
    assert!(!yaml.contains("drive-1"), "{yaml}");
}

#[test]
fn test_resource_count() {
    let resources = get_all_resources();
    assert_eq!(
        resources.len(),
        6,
        "Expected 6 resources (cmdr://state, cmdr://dialogs/available, cmdr://indexing, cmdr://importance, \
         cmdr://settings, cmdr://logs)"
    );
}

#[test]
fn test_all_resource_uris_are_valid() {
    let resources = get_all_resources();
    for resource in resources {
        assert!(
            resource.uri.starts_with("cmdr://"),
            "Resource URI should start with cmdr://: {}",
            resource.uri
        );
        assert!(!resource.name.is_empty(), "Resource name should not be empty");
        assert!(
            !resource.description.is_empty(),
            "Resource description should not be empty"
        );
    }
}

#[test]
fn test_no_duplicate_resource_uris() {
    let resources = get_all_resources();
    let mut uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
    uris.sort();
    let original_len = uris.len();
    uris.dedup();
    assert_eq!(uris.len(), original_len, "Duplicate resource URIs detected");
}

#[test]
fn test_resources_exist() {
    let resources = get_all_resources();
    let expected_uris = [
        "cmdr://state",
        "cmdr://dialogs/available",
        "cmdr://indexing",
        "cmdr://importance",
        "cmdr://settings",
        "cmdr://logs",
    ];
    for uri in expected_uris {
        assert!(resources.iter().any(|r| r.uri == uri), "Missing resource: {}", uri);
    }
}

#[test]
fn test_all_resources_have_valid_mime_type() {
    let resources = get_all_resources();
    for resource in resources {
        assert!(
            resource.mime_type == "text/yaml" || resource.mime_type == "text/plain",
            "Resource {} has unexpected mime type: {}",
            resource.uri,
            resource.mime_type
        );
    }
}
