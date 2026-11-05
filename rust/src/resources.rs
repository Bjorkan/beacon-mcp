use uuid::{Uuid, Variant};

pub fn parse_uri(uri: &str) -> Option<(&'static str, String)> {
    let (kind, id) = uri.strip_prefix("beacon://")?.split_once('/')?;
    let kind = match kind {
        "nodes" => "nodes",
        "observers" => "observers",
        _ => return None,
    };
    if id.len() != 36
        || id.as_bytes().get(8) != Some(&b'-')
        || id.as_bytes().get(13) != Some(&b'-')
        || id.as_bytes().get(18) != Some(&b'-')
        || id.as_bytes().get(23) != Some(&b'-')
    {
        return None;
    }
    let uuid = Uuid::parse_str(id).ok()?;
    if !(1..=8).contains(&uuid.get_version_num()) || uuid.get_variant() != Variant::RFC4122 {
        return None;
    }
    let canonical = format!("beacon://{kind}/{}", uuid);
    if uri.to_ascii_lowercase() != canonical {
        return None;
    }
    Some((kind, uuid.to_string()))
}

#[cfg(test)]
mod tests {
    use super::parse_uri;
    #[test]
    fn canonicalizes_uuid_and_rejects_injected_paths() {
        assert_eq!(
            parse_uri("beacon://nodes/123E4567-E89B-42D3-A456-426614174000").unwrap(),
            ("nodes", "123e4567-e89b-42d3-a456-426614174000".to_owned())
        );
        for uri in [
            "beacon://nodes/../admin",
            "beacon://observers/123e4567-e89b-42d3-a456-426614174000/extra",
            "beacon://nodes/00000000-0000-0000-0000-000000000000",
            "beacon://nodes/%2e%2e",
            "beacon://nodes/123e4567-e89b-42d3-a456-426614174000#fragment",
        ] {
            assert!(parse_uri(uri).is_none(), "{uri}");
        }
    }
}
