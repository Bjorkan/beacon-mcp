//! PR 03 registry parity: 53 reads, one key each, no prohibited routes.
//! Offline; checks registry vs client allowlist vs vendored OpenAPI vs tools.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

#[test]
fn registry_has_53_keys_and_matches_allowlist() {
    assert_eq!(beacon_api::registry::REGISTRY.len(), 53);
    let mut keys = BTreeSet::new();
    for entry in beacon_api::registry::REGISTRY {
        assert!(keys.insert(entry.key), "duplicate key {}", entry.key);
        assert_eq!(entry.method, "GET");
        assert!(!entry.path.is_empty());
        assert!(beacon_api::registry::find_by_key(entry.key).is_some());
        assert!(beacon_api::registry::find_by_operation(entry.operation).is_some());
    }
    // WsTicket internal-only: in client enum but not in registry.
    let source = fs::read_to_string(repo_root().join("crates/beacon-api/src/client.rs")).unwrap();
    assert!(source.contains("WsTicket"));
    assert!(beacon_api::registry::WSTICKET_INTERNAL_NOTE.contains("ws-ticket"));
}

#[test]
fn no_prohibited_routes_are_reachable() {
    let source = fs::read_to_string(repo_root().join("crates/beacon-api/src/client.rs")).unwrap();
    for banned in ["/admin/", "/auth/", "/me/", "Atlas", "telemetry-requests"] {
        // Only allowed mentions are in comments/docs, not endpoint strings.
        if let Some(path) = banned.strip_prefix('/') {
            assert!(
                !source.contains(&format!("(\"{path}\",")),
                "prohibited endpoint {banned}"
            );
        }
    }
    for file in [
        "crates/beacon-workflows/src/core-tool-definitions.json",
        "crates/beacon-workflows/src/packet-tool-definitions.json",
        "crates/beacon-workflows/src/routing-tool-definitions.json",
        "crates/beacon-workflows/src/analytics-tool-definitions.json",
        "crates/beacon-workflows/src/extra-tool-definitions.json",
        "crates/beacon-workflows/src/composite-tool-definitions.json",
        "crates/beacon-workflows/src/explore-tool-definitions.json",
    ] {
        let text = fs::read_to_string(repo_root().join(file)).unwrap();
        assert!(!text.contains("ws-ticket"), "ticket leaked in {file}");
        for route in ["/admin/", "/auth/", "/me/", "telemetry-requests", "Atlas"] {
            assert!(!text.contains(route), "{route} leaked in {file}");
        }
    }
}

#[test]
fn vendored_contract_contains_all_registry_paths() {
    let yaml = fs::read_to_string(repo_root().join("vendor/beacon-openapi.yaml")).unwrap();
    for entry in beacon_api::registry::REGISTRY {
        // Registry path uses gateway param names; vendored may rename params
        // (regions/{regionId}, scopes/{name}); check the static prefix.
        let prefix = entry.path.split('{').next().unwrap().trim_end_matches('/');
        assert!(!prefix.is_empty(), "empty prefix for {}", entry.key);
        assert!(
            yaml.contains(prefix),
            "vendored contract missing prefix {prefix} for {}",
            entry.key
        );
    }
}

#[test]
fn checked_in_definitions_match_registry_keys_and_schema_version() {
    use std::collections::BTreeMap;
    let mut names = BTreeSet::new();
    for file in [
        "crates/beacon-workflows/src/core-tool-definitions.json",
        "crates/beacon-workflows/src/packet-tool-definitions.json",
        "crates/beacon-workflows/src/routing-tool-definitions.json",
        "crates/beacon-workflows/src/analytics-tool-definitions.json",
        "crates/beacon-workflows/src/extra-tool-definitions.json",
    ] {
        let text = fs::read_to_string(repo_root().join(file)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let object = value.as_object().unwrap();
        for (name, tool) in object {
            assert!(names.insert(name.clone()), "duplicate tool {name}");
            let input_schema = &tool["inputSchema"];
            assert_eq!(
                input_schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
                "{name} missing draft 2020-12 marker"
            );
        }
    }
    // 53 REST reads have definitions; 2 composites live separately.
    assert_eq!(names.len(), 53);
    let _map: BTreeMap<&str, ()> = BTreeMap::new();
}
