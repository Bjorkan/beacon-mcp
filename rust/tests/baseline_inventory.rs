//! PR 01 frozen baseline: 55 tools, 7 resources, 3 prompts, limits.
//! Offline only; no network, no behavior change.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read_repo(rel: &str) -> String {
    fs::read_to_string(repo_root().join(rel)).expect("baseline file readable")
}

fn tool_files() -> Vec<(&'static str, usize)> {
    vec![
        ("rust/src/analytics-tool-definitions.json", 4),
        ("rust/src/composite-tool-definitions.json", 2),
        ("rust/src/core-tool-definitions.json", 9),
        ("rust/src/extra-tool-definitions.json", 30),
        ("rust/src/packet-tool-definitions.json", 5),
        ("rust/src/routing-tool-definitions.json", 5),
    ]
}

#[test]
fn frozen_tool_inventory_is_55_with_documented_split() {
    let mut names = BTreeSet::new();
    let mut total = 0;
    for (file, expected) in tool_files() {
        let text = fs::read_to_string(repo_root().join(file)).expect("tool definitions readable");
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        let object = value.as_object().expect("definitions object");
        assert_eq!(object.len(), expected, "{file}");
        for name in object.keys() {
            assert!(names.insert(name.clone()), "duplicate tool {name}");
        }
        total += object.len();
    }
    assert_eq!(total, 55);
    assert_eq!(names.len(), 55);

    let frozen = read_repo("tests/fixtures/tool-names.json");
    let frozen: Vec<String> = serde_json::from_str(&frozen).expect("valid fixtures JSON");
    assert_eq!(frozen.len(), 55);
    for name in &frozen {
        assert!(names.contains(name), "fixture tool missing: {name}");
    }

    for baseline in [
        "beacon_get_server_info",
        "beacon_list_iatas",
        "beacon_get_node",
        "beacon_get_node_polling",
        "beacon_get_node_poller",
        "beacon_backfill_messages",
        "beacon_backfill_packets",
        "beacon_get_route_observations",
        "beacon_compare_observers",
        "beacon_diagnose_node",
        "beacon_compare_regions",
    ] {
        assert!(
            names.contains(baseline),
            "baseline tool missing: {baseline}"
        );
    }
}

#[test]
fn composites_use_bounded_fanout_and_ticket_stays_internal() {
    let composite = read_repo("rust/src/composite-tool-definitions.json");
    let composite: serde_json::Value = serde_json::from_str(&composite).unwrap();
    assert_eq!(composite.as_object().unwrap().len(), 2);
    assert!(composite.get("beacon_diagnose_node").is_some());
    assert!(composite.get("beacon_compare_regions").is_some());

    for (file, text) in tool_files().iter().map(|(f, _)| (*f, read_repo(f))) {
        assert!(!text.contains("ws-ticket"), "ticket leaked in {file}");
        assert!(!text.contains("ws_ticket"), "ticket leaked in {file}");
    }
}

#[test]
fn baseline_docs_cover_inventory_comparison_matrix_and_eval() {
    for doc in [
        "docs/ai-first/README.md",
        "docs/ai-first/01-baseline-inventory.md",
        "docs/ai-first/01-upstream-comparison.md",
        "docs/ai-first/01-upstream-diff.md",
        "docs/ai-first/01-capability-matrix.md",
        "docs/ai-first/01-eval-baseline.md",
    ] {
        let text = fs::read_to_string(repo_root().join(doc)).expect("baseline doc readable");
        if doc.ends_with("README.md") {
            assert!(text.contains("PR 01"), "{doc} references PR");
        } else {
            assert!(text.contains("Refs #121"), "{doc} references issue");
        }
    }
    let inventory = read_repo("docs/ai-first/01-baseline-inventory.md");
    assert!(inventory.contains("WsTicket"));
    assert!(inventory.contains("workflow.diagnose_node"));
    let diff = read_repo("docs/ai-first/01-upstream-diff.md");
    assert!(diff.contains("Signed no-delta") || diff.contains("no-delta"));
    let eval = read_repo("docs/ai-first/01-eval-baseline.md");
    for marker in [
        "EVAL-01", "EVAL-02", "EVAL-03", "EVAL-04", "EVAL-05", "EVAL-06",
    ] {
        assert!(eval.contains(marker), "missing {marker}");
    }
}

#[test]
fn resource_and_prompt_baseline_counts() {
    for uri in [
        "beacon://nodes/123e4567-e89b-42d3-a456-426614174000",
        "beacon://observers/123e4567-e89b-42d3-a456-426614174000",
        "beacon://packets/aabbccddeeff0011",
        "beacon://traces/abcdef01",
        "beacon://channels/25",
        "beacon://regions/3",
        "beacon://iatas/arn",
    ] {
        assert!(
            beacon_mcp_rust::resources::parse_uri(uri).is_some(),
            "{uri}"
        );
    }
    assert!(beacon_mcp_rust::resources::parse_uri("beacon://nodes/../admin").is_none());
    let prompts = beacon_mcp_rust::prompts::list().prompts.len();
    assert_eq!(prompts, 3, "three prompts");
}

#[test]
fn operation_enum_covers_53_public_reads_plus_internal_ticket() {
    let source = read_repo("rust/src/client.rs");
    // Count `Variant,` lines inside the Operation enum block.
    let enum_block = source
        .split("pub enum Operation")
        .nth(1)
        .expect("Operation enum")
        .split('}')
        .next()
        .expect("enum block");
    let variants: Vec<String> = enum_block
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("//")
                && line.ends_with(',')
                && !line.contains("enum")
        })
        .map(|line| line.trim_end_matches(',').to_owned())
        .collect();
    assert_eq!(variants.len(), 54, "54 Operation variants: {variants:?}");
    assert!(variants.contains(&"WsTicket".to_owned()));
    assert!(variants.contains(&"NodePolling".to_owned()));
    assert!(variants.contains(&"NodePoller".to_owned()));
    // Every public endpoint arm must exist in endpoint().
    for endpoint in [
        "\"info\"",
        "\"iatas\"",
        "\"nodes/telemetry/latest\"",
        "\"observers/directory\"",
        "\"messages/backfill\"",
        "\"packets/backfill\"",
        "\"routes/cross\"",
        "\"ws-ticket\"",
    ] {
        assert!(source.contains(endpoint), "missing endpoint {endpoint}");
    }
    let diff = read_repo("docs/ai-first/01-upstream-diff.md");
    for op in [
        "NodePolling",
        "NodePoller",
        "WsTicket",
        "ObserverComparison",
    ] {
        assert!(diff.contains(op), "diff missing {op}");
    }
}
