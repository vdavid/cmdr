use crate::mcp::dialog_state::KnownDialog;
use crate::mcp::resources::{format_available_dialogs_yaml, get_all_resources};

#[test]
fn dialogs_available_carries_registered_descriptions() {
    // Every FE-registered soft dialog renders its `dialog-registry.ts` description in
    // cmdr://dialogs/available; a dialog without one renders just its type line. This
    // pins the description round-trip the dogfooding flagged as inconsistent.
    let known = vec![
        KnownDialog {
            id: "whats-new".to_string(),
            description: Some("Post-update changelog summary popup".to_string()),
        },
        KnownDialog {
            id: "about".to_string(),
            description: None,
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
