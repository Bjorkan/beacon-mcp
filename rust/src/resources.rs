//! Canonical, path-injection-safe resource URIs for public Beacon GET endpoints.
use uuid::{Uuid, Variant};

pub fn parse_uri(uri: &str) -> Option<(&'static str, String)> {
    let (kind, raw) = uri.strip_prefix("beacon://")?.split_once('/')?;
    let kind = match kind {
        "nodes" => "nodes",
        "observers" => "observers",
        "packets" => "packets",
        "channels" => "channels",
        "traces" => "traces",
        "iatas" => "iatas",
        "regions" => "regions",
        _ => return None,
    };
    if raw.is_empty() || raw.chars().any(char::is_control) {
        return None;
    }
    let id = match kind {
        "nodes" | "observers" => {
            if raw.len() != 36
                || raw.as_bytes().get(8) != Some(&b'-')
                || raw.as_bytes().get(13) != Some(&b'-')
                || raw.as_bytes().get(18) != Some(&b'-')
                || raw.as_bytes().get(23) != Some(&b'-')
            {
                return None;
            }
            let uuid = Uuid::parse_str(raw).ok()?;
            if !(1..=8).contains(&uuid.get_version_num()) || uuid.get_variant() != Variant::RFC4122
            {
                return None;
            }
            uuid.to_string()
        }
        "packets" | "traces" => {
            let expected = if kind == "packets" { 16 } else { 8 };
            if raw.len() != expected || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            raw.to_ascii_lowercase()
        }
        "channels" | "regions" => {
            let number = raw.parse::<u32>().ok()?;
            if number == 0 || number > i32::MAX as u32 {
                return None;
            }
            number.to_string()
        }
        "iatas" => {
            if raw.len() != 3 || !raw.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
                return None;
            }
            raw.to_ascii_uppercase()
        }
        _ => return None,
    };
    let canonical = format!("beacon://{kind}/{id}");
    if !uri.eq_ignore_ascii_case(&canonical) {
        return None;
    }
    Some((kind, id))
}

#[cfg(test)]
mod tests {
    use super::parse_uri;

    #[test]
    fn valid_canonical_public_resource_uris() {
        for (uri, kind, id) in [
            (
                "beacon://nodes/123E4567-E89B-42D3-A456-426614174000",
                "nodes",
                "123e4567-e89b-42d3-a456-426614174000",
            ),
            (
                "beacon://observers/123e4567-e89b-42d3-a456-426614174000",
                "observers",
                "123e4567-e89b-42d3-a456-426614174000",
            ),
            (
                "beacon://packets/AABBCCDDEEFF0011",
                "packets",
                "aabbccddeeff0011",
            ),
            ("beacon://traces/ABCDEF01", "traces", "abcdef01"),
            ("beacon://channels/25", "channels", "25"),
            ("beacon://regions/3", "regions", "3"),
            ("beacon://iatas/arn", "iatas", "ARN"),
        ] {
            assert_eq!(parse_uri(uri), Some((kind, id.to_owned())), "{uri}");
        }
    }

    #[test]
    fn invalid_uris_cannot_escape_fixed_public_paths() {
        for uri in [
            "beacon://nodes/../admin",
            "beacon://observers/123e4567-e89b-42d3-a456-426614174000/extra",
            "beacon://nodes/00000000-0000-0000-0000-000000000000",
            "beacon://nodes/%2e%2e",
            "beacon://nodes/123e4567-e89b-42d3-a456-426614174000#fragment",
            "beacon://packets/../../admin",
            "beacon://packets/0123",
            "beacon://traces/%2e%2e",
            "beacon://channels/0",
            "beacon://channels/0001",
            "beacon://regions/2147483648",
            "beacon://iatas/ARN?query=1",
            "beacon://iatas/AR/",
            "beacon://scopes/%23bc",
        ] {
            assert!(parse_uri(uri).is_none(), "{uri}");
        }
    }
}
