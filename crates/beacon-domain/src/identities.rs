//! Distinct MeshCore identities: node vs observer UUIDs, full pubkey vs prefix.
//! Prefix collisions are ambiguous, never high-confidence equality.

use uuid::{Uuid, Variant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(Uuid);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObserverId(Uuid);

fn valid_uuid(text: &str) -> Option<Uuid> {
    if text.len() != 36 {
        return None;
    }
    let uuid = Uuid::parse_str(text).ok()?;
    if !(1..=8).contains(&uuid.get_version_num()) || uuid.get_variant() != Variant::RFC4122 {
        return None;
    }
    Some(uuid)
}

impl NodeId {
    pub fn parse(text: &str) -> Option<Self> {
        valid_uuid(text).map(Self)
    }
}

impl ObserverId {
    pub fn parse(text: &str) -> Option<Self> {
        valid_uuid(text).map(Self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashPrefix(String);

impl PublicKey {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self(text.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl HashPrefix {
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || text.len() > 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self(text.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches(&self, key: &PublicKey) -> bool {
        key.as_str().starts_with(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchConfidence {
    High,
    Ambiguous,
    None,
}

pub fn resolve_prefix(
    prefix: &HashPrefix,
    candidates: &[PublicKey],
) -> (MatchConfidence, Vec<usize>) {
    let hits: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, key)| prefix.matches(key))
        .map(|(index, _)| index)
        .collect();
    match hits.len() {
        0 => (MatchConfidence::None, hits),
        1 => {
            let key = &candidates[hits[0]];
            if prefix.as_str().len() >= key.as_str().len() {
                (MatchConfidence::High, hits)
            } else {
                (MatchConfidence::Ambiguous, hits)
            }
        }
        _ => (MatchConfidence::Ambiguous, hits),
    }
}

#[cfg(test)]
mod tests {
    use super::{HashPrefix, MatchConfidence, NodeId, ObserverId, PublicKey, resolve_prefix};

    const UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn node_and_observer_ids_are_distinct_types() {
        let node = NodeId::parse(UUID).unwrap();
        let observer = ObserverId::parse(UUID).unwrap();
        // Same string, different types: no accidental equality.
        fn is_node(_: NodeId) {}
        fn is_observer(_: ObserverId) {}
        is_node(node);
        is_observer(observer);
        assert!(NodeId::parse("not-a-uuid").is_none());
        assert!(ObserverId::parse("not-a-uuid").is_none());
    }

    #[test]
    fn pubkey_and_prefix_are_distinct_and_collisions_stay_ambiguous() {
        let a = PublicKey::parse(&format!("{}{}", "ab".repeat(31), "cd")).unwrap();
        let b = PublicKey::parse(&format!("{}{}", "ab".repeat(31), "ef")).unwrap();
        let prefix = HashPrefix::parse(&"ab".repeat(31)).unwrap();
        assert!(prefix.matches(&a) && prefix.matches(&b));
        let (confidence, hits) = resolve_prefix(&prefix, &[a, b]);
        assert_eq!(confidence, MatchConfidence::Ambiguous);
        assert_eq!(hits.len(), 2);
        let full = HashPrefix::parse(&format!("{}{}", "ab".repeat(31), "cd")).unwrap();
        let (confidence, _) = resolve_prefix(
            &full,
            &[PublicKey::parse(&format!("{}{}", "ab".repeat(31), "cd")).unwrap()],
        );
        assert_eq!(confidence, MatchConfidence::High);
        let none = HashPrefix::parse("ff").unwrap();
        let (confidence, _) = resolve_prefix(&none, &[PublicKey::parse(&"00".repeat(32)).unwrap()]);
        assert_eq!(confidence, MatchConfidence::None);
    }
}
