use crate::mcp::resources::get_all_resources;

#[test]
fn test_resource_count() {
    let resources = get_all_resources();
    assert_eq!(
        resources.len(),
        4,
        "Expected 4 resources (cmdr://state, cmdr://dialogs/available, cmdr://indexing, cmdr://settings)"
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
    let expected_uris = ["cmdr://state", "cmdr://dialogs/available", "cmdr://indexing"];
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
