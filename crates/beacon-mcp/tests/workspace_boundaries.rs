//! PR 02 dependency-boundary guard: API and domain stay MCP-free.
//! Offline; parses member manifests only.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

fn manifest(crate_name: &str) -> String {
    fs::read_to_string(repo_root().join(format!("crates/{crate_name}/Cargo.toml")))
        .expect("member manifest readable")
}

#[test]
fn api_and_domain_have_no_mcp_transport_dependency() {
    for crate_name in ["beacon-api", "beacon-domain"] {
        let text = manifest(crate_name);
        assert!(
            !text.contains("beacon-mcp"),
            "{crate_name} must not depend on beacon-mcp"
        );
        assert!(
            !text.contains("rmcp"),
            "{crate_name} must not depend on rmcp"
        );
        assert!(
            !text.contains("axum"),
            "{crate_name} must not depend on axum transport"
        );
    }
    let workflows = manifest("beacon-workflows");
    assert!(
        workflows.contains("beacon-api"),
        "workflows orchestrates api"
    );
    let mcp = manifest("beacon-mcp");
    for dep in ["beacon-api", "beacon-domain", "beacon-workflows"] {
        assert!(mcp.contains(dep), "beacon-mcp wires {dep}");
    }
}

#[test]
fn no_inter_crate_network_or_persistence_boundary() {
    for (crate_name, file) in [
        ("beacon-api", "src/client.rs"),
        ("beacon-domain", "src/resources.rs"),
    ] {
        let text = fs::read_to_string(repo_root().join(format!("crates/{crate_name}/{file}")))
            .expect("crate source readable");
        assert!(
            !text.contains("TcpListener") && !text.contains("serve("),
            "{crate_name}/{file} must not host a service boundary"
        );
    }
    for crate_name in [
        "beacon-api",
        "beacon-domain",
        "beacon-workflows",
        "beacon-mcp",
    ] {
        let text = manifest(crate_name);
        for banned in ["redis", "postgres", "sqlite", "sled", "kafka"] {
            assert!(!text.contains(banned), "{crate_name} must not add storage");
        }
    }
}
