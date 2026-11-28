//! Shared entity resolver: name, UUID, pubkey, prefix, type, area.
//! Deterministic, never guesses; ambiguity returns bounded candidates.

use beacon_domain::{
    geography::Iata,
    identities::{HashPrefix, MatchConfidence, NodeId, ObserverId, PublicKey, resolve_prefix},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityType {
    Node,
    Observer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveInput {
    pub name: Option<String>,
    pub id: Option<String>,
    pub pubkey: Option<String>,
    pub prefix: Option<String>,
    pub entity: Option<EntityType>,
    pub iata: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub identity: String,
    pub entity: EntityType,
    pub name: Option<String>,
    pub iata: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    Node(NodeId),
    Observer(ObserverId),
    Key(PublicKey),
    Ambiguous {
        candidates: Vec<Candidate>,
        coverage: String,
    },
    NotFound,
    Invalid(String),
}

pub const MAX_CANDIDATES: usize = 10;

pub fn resolve(
    input: &ResolveInput,
    nodes_by_name: &[Candidate],
    pubkeys: &[PublicKey],
) -> Resolved {
    if let Some(id) = input.id.as_deref() {
        match input.entity.unwrap_or(EntityType::Node) {
            EntityType::Node => match NodeId::parse(id) {
                Some(node) => return Resolved::Node(node),
                None => return Resolved::Invalid("invalid node UUID".to_owned()),
            },
            EntityType::Observer => match ObserverId::parse(id) {
                Some(observer) => return Resolved::Observer(observer),
                None => return Resolved::Invalid("invalid observer UUID".to_owned()),
            },
        }
    }
    if let Some(key) = input.pubkey.as_deref() {
        match PublicKey::parse(key) {
            Some(parsed) => return Resolved::Key(parsed),
            None => return Resolved::Invalid("invalid public key".to_owned()),
        }
    }
    if let Some(prefix_text) = input.prefix.as_deref() {
        let prefix = match HashPrefix::parse(prefix_text) {
            Some(prefix) => prefix,
            None => return Resolved::Invalid("invalid prefix".to_owned()),
        };
        let (confidence, hits) = resolve_prefix(&prefix, pubkeys);
        match confidence {
            MatchConfidence::High => {
                return Resolved::Key(pubkeys[hits[0]].clone());
            }
            MatchConfidence::Ambiguous => {
                let candidates: Vec<Candidate> = hits
                    .into_iter()
                    .take(MAX_CANDIDATES)
                    .map(|index| Candidate {
                        identity: pubkeys[index].as_str().to_owned(),
                        entity: EntityType::Node,
                        name: None,
                        iata: input.iata.clone(),
                    })
                    .collect();
                return Resolved::Ambiguous {
                    candidates,
                    coverage: "prefix collides; exact key required".to_owned(),
                };
            }
            MatchConfidence::None => return Resolved::NotFound,
        }
    }
    if let Some(name) = input.name.as_deref() {
        let normalized = name.trim().to_ascii_lowercase();
        if normalized.is_empty() || normalized.len() > 256 {
            return Resolved::Invalid("invalid name".to_owned());
        }
        let name_matches: Vec<&Candidate> = nodes_by_name
            .iter()
            .filter(|candidate| {
                candidate
                    .name
                    .as_deref()
                    .is_some_and(|candidate_name| candidate_name.to_ascii_lowercase() == normalized)
            })
            .collect();
        if name_matches.is_empty() {
            return Resolved::NotFound;
        }
        if input.entity.is_some()
            && !name_matches
                .iter()
                .any(|candidate| Some(candidate.entity) == input.entity)
        {
            return Resolved::Invalid("entity type mismatch".to_owned());
        }
        let mut hits: Vec<Candidate> = name_matches
            .into_iter()
            .filter(|candidate| input.entity.is_none_or(|entity| candidate.entity == entity))
            .filter(|candidate| {
                input.iata.as_deref().is_none_or(|wanted| {
                    Iata::parse(wanted)
                        .is_some_and(|parsed| candidate.iata.as_deref() == Some(parsed.as_str()))
                        || candidate.iata.as_deref() == Some(wanted)
                })
            })
            .take(MAX_CANDIDATES + 1)
            .cloned()
            .collect();
        match hits.len() {
            0 => return Resolved::NotFound,
            1 => {
                let Some(only) = hits.pop() else {
                    return Resolved::NotFound;
                };
                if only.entity == EntityType::Node {
                    if let Some(node) = NodeId::parse(&only.identity) {
                        return Resolved::Node(node);
                    }
                } else if let Some(observer) = ObserverId::parse(&only.identity) {
                    return Resolved::Observer(observer);
                }
                return Resolved::Ambiguous {
                    candidates: vec![only],
                    coverage: "single candidate needs UUID confirmation".to_owned(),
                };
            }
            _ => {
                hits.truncate(MAX_CANDIDATES);
                return Resolved::Ambiguous {
                    candidates: hits,
                    coverage: "name collides; UUID required".to_owned(),
                };
            }
        }
    }
    Resolved::Invalid("no resolvable field".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{Candidate, EntityType, ResolveInput, Resolved, resolve};
    use beacon_domain::identities::PublicKey;

    const UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn exact_uuid_resolves_and_cross_type_rejected() {
        let node = resolve(
            &ResolveInput {
                name: None,
                id: Some(UUID.to_owned()),
                pubkey: None,
                prefix: None,
                entity: Some(EntityType::Node),
                iata: None,
            },
            &[],
            &[],
        );
        assert!(matches!(node, Resolved::Node(_)));
        let observer = resolve(
            &ResolveInput {
                name: None,
                id: Some(UUID.to_owned()),
                pubkey: None,
                prefix: None,
                entity: Some(EntityType::Observer),
                iata: None,
            },
            &[],
            &[],
        );
        assert!(matches!(observer, Resolved::Observer(_)));
        let bad = resolve(
            &ResolveInput {
                name: None,
                id: Some("bad".to_owned()),
                pubkey: None,
                prefix: None,
                entity: Some(EntityType::Node),
                iata: None,
            },
            &[],
            &[],
        );
        assert!(matches!(bad, Resolved::Invalid(_)));
    }

    #[test]
    fn ambiguous_names_return_bounded_candidates() {
        let candidates = vec![
            Candidate {
                identity: UUID.to_owned(),
                entity: EntityType::Node,
                name: Some("Kalle".to_owned()),
                iata: Some("ARN".to_owned()),
            },
            Candidate {
                identity: "223e4567-e89b-42d3-a456-426614174001".to_owned(),
                entity: EntityType::Node,
                name: Some("kalle".to_owned()),
                iata: Some("ARN".to_owned()),
            },
        ];
        let shared = "ab".repeat(31);
        let keys = vec![
            PublicKey::parse(&format!("{shared}cd")).unwrap(),
            PublicKey::parse(&format!("{shared}ef")).unwrap(),
        ];
        let ambiguous = resolve(
            &ResolveInput {
                name: Some("Kalle".to_owned()),
                id: None,
                pubkey: None,
                prefix: None,
                entity: Some(EntityType::Node),
                iata: None,
            },
            &candidates,
            &keys,
        );
        assert!(matches!(ambiguous, Resolved::Ambiguous { .. }));
        let prefix_ambiguous = resolve(
            &ResolveInput {
                name: None,
                id: None,
                pubkey: None,
                prefix: Some(shared),
                entity: None,
                iata: None,
            },
            &[],
            &keys,
        );
        assert!(matches!(prefix_ambiguous, Resolved::Ambiguous { .. }));
    }
}

#[cfg(test)]
mod cross_type_tests {
    use super::{Candidate, EntityType, ResolveInput, Resolved, resolve};

    #[test]
    fn name_with_wrong_entity_is_invalid_not_notfound() {
        let candidates = vec![Candidate {
            identity: "223e4567-e89b-42d3-a456-426614174001".to_owned(),
            entity: EntityType::Observer,
            name: Some("Kalle".to_owned()),
            iata: None,
        }];
        let result = resolve(
            &ResolveInput {
                name: Some("Kalle".to_owned()),
                id: None,
                pubkey: None,
                prefix: None,
                entity: Some(EntityType::Node),
                iata: None,
            },
            &candidates,
            &[],
        );
        assert!(matches!(result, Resolved::Invalid(_)));
    }
}
