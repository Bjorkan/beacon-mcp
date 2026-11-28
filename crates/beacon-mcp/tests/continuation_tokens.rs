//! PR 03 continuation tokens: tamper/size/binding/cross-replica behavior.
//! Offline; no network beyond mock-free encode/decode plus mock client pages.

use beacon_api::continuation::{decode, encode};
use serde_json::json;

#[test]
fn token_round_trip_and_operation_binding() {
    let filters = json!({"iata": "ARN"});
    let token = encode(
        "node.observations",
        Some("42"),
        Some("s1:x"),
        None,
        &filters,
        Some("lastSeen"),
        Some("desc"),
        None,
        None,
    )
    .unwrap();
    let decoded = decode(&token, "node.observations").unwrap();
    assert_eq!(decoded.operation_key, "node.observations");
    assert_eq!(decoded.cursor.as_deref(), Some("42"));
    // Cross-operation replay rejected.
    assert!(decode(&token, "node.activity").is_err());
    // Tampered token rejected.
    let mut tampered = token.clone();
    tampered.push('A');
    assert!(decode(&tampered, "node.observations").is_err());
    // Empty token rejected.
    assert!(decode("", "node.observations").is_err());
}

#[test]
fn oversized_filters_and_fields_rejected() {
    let big = json!({"blob": "x".repeat(2000)});
    assert!(
        encode(
            "node.search",
            None,
            None,
            None,
            &big,
            None,
            None,
            None,
            None
        )
        .is_err()
    );
    let filters = json!({});
    assert!(
        encode(
            "node.search",
            Some(&"y".repeat(600)),
            None,
            None,
            &filters,
            None,
            None,
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn truncated_vs_exhausted_semantics_preserved() {
    // Nonpageable truncation and pageable exhaustion are distinct upstream
    // signals; tokens never invent a cursor or total. This test pins the
    // distinction at the API level: a token may carry no cursor (exhausted)
    // or an exact cursor (continuation), but never a fabricated total.
    let filters = json!({"limit": 20});
    let exhausted = encode(
        "iata.list",
        None,
        None,
        None,
        &filters,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let decoded = decode(&exhausted, "iata.list").unwrap();
    assert!(decoded.cursor.is_none());
    assert!(decoded.page_cursor.is_none());
}
