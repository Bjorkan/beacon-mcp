use std::{env, net::IpAddr, time::Duration};
use url::Url;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub log_level: String,
    pub beacon_base_url: Url,
    pub beacon_timeout: Duration,
    pub stats_timeout: Duration,
    pub shutdown_grace: Duration,
    pub subscriptions: SubscriptionConfig,
}

#[derive(Clone, Debug)]
pub struct SubscriptionConfig {
    pub enabled: bool,
    pub ws_url: Url,
    pub max_streams: usize,
    pub debounce: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::parse(|name| env::var(name).ok())
    }

    pub fn parse(mut get: impl FnMut(&str) -> Option<String>) -> Result<Self, String> {
        let host = get("HOST").unwrap_or_else(|| "0.0.0.0".into());
        let port: u16 = parse_u64(&mut get, "PORT", 3000, 1024, 65535)?
            .try_into()
            .map_err(|_| "invalid PORT".to_string())?;
        let bind = if let Ok(address) = host.parse::<IpAddr>() {
            std::net::SocketAddr::new(address, port).to_string()
        } else if host.len() <= 253
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                    && !label.starts_with('-')
                    && !label.ends_with('-')
            })
        {
            format!("{host}:{port}")
        } else {
            return Err("HOST must be an IP address or hostname".into());
        };
        let log_level = get("LOG_LEVEL").unwrap_or_else(|| "info".into());
        if !["fatal", "error", "warn", "info", "debug", "trace", "silent"]
            .contains(&log_level.as_str())
        {
            return Err("invalid LOG_LEVEL".into());
        }
        let source = get("BEACON_BASE_URL").ok_or("BEACON_BASE_URL is required")?;
        let mut beacon_base_url = Url::parse(&source).map_err(|_| "invalid BEACON_BASE_URL")?;
        if !["http", "https"].contains(&beacon_base_url.scheme())
            || beacon_base_url.host_str().is_none()
            || !beacon_base_url.username().is_empty()
            || beacon_base_url.password().is_some()
            || beacon_base_url.query().is_some()
            || beacon_base_url.fragment().is_some()
        {
            return Err(
                "BEACON_BASE_URL requires an http(s) URL without credentials, query or fragment"
                    .into(),
            );
        }
        let path = format!("{}/", beacon_base_url.path().trim_end_matches('/'));
        beacon_base_url.set_path(&path);
        let ws_url = match get("BEACON_WS_URL").filter(|value| !value.is_empty()) {
            Some(value) => Url::parse(&value).map_err(|_| "invalid BEACON_WS_URL")?,
            None => {
                let mut derived = beacon_base_url.clone();
                derived
                    .set_scheme(if derived.scheme() == "https" {
                        "wss"
                    } else {
                        "ws"
                    })
                    .map_err(|_| "invalid Beacon WS scheme")?;
                derived.set_path("/ws");
                derived
            }
        };
        if !["ws", "wss"].contains(&ws_url.scheme())
            || ws_url.host_str().is_none()
            || !ws_url.username().is_empty()
            || ws_url.password().is_some()
            || ws_url.query().is_some()
            || ws_url.fragment().is_some()
            || !ws_url.path().ends_with("/ws")
        {
            return Err("BEACON_WS_URL must be a ws/wss URL ending in /ws without credentials, query or fragment".into());
        }
        let enabled = match get("BEACON_SUBSCRIPTIONS_ENABLED").as_deref() {
            None | Some("true") => true,
            Some("false") => false,
            _ => return Err("BEACON_SUBSCRIPTIONS_ENABLED must be true or false".into()),
        };
        let max_streams =
            parse_u64(&mut get, "BEACON_SUBSCRIPTIONS_MAX_STREAMS", 64, 1, 1024)? as usize;
        let debounce = Duration::from_millis(parse_u64(
            &mut get,
            "BEACON_SUBSCRIPTIONS_DEBOUNCE_MS",
            500,
            0,
            5000,
        )?);
        Ok(Self {
            bind,
            log_level,
            beacon_base_url,
            beacon_timeout: Duration::from_millis(parse_u64(
                &mut get,
                "BEACON_TIMEOUT_MS",
                10_000,
                100,
                120_000,
            )?),
            stats_timeout: Duration::from_millis(parse_u64(
                &mut get,
                "BEACON_STATS_TIMEOUT_MS",
                20_000,
                100,
                300_000,
            )?),
            shutdown_grace: Duration::from_millis(parse_u64(
                &mut get,
                "SHUTDOWN_GRACE_MS",
                10_000,
                100,
                60_000,
            )?),
            subscriptions: SubscriptionConfig {
                enabled,
                ws_url,
                max_streams,
                debounce,
            },
        })
    }
}

fn parse_u64(
    get: &mut impl FnMut(&str) -> Option<String>,
    name: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    let value = match get(name) {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| format!("invalid {name}"))?,
        None => default,
    };
    if !(min..=max).contains(&value) {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::Config;
    use std::collections::HashMap;

    fn parse(env: &[(&str, &str)]) -> Result<Config, String> {
        let values: HashMap<_, _> = env.iter().copied().collect();
        Config::parse(|key| values.get(key).map(|value| (*value).to_string()))
    }

    #[test]
    fn defaults_and_normalizes_base_url() {
        let config = parse(&[("BEACON_BASE_URL", "https://beacon.example/prefix")]).unwrap();
        assert_eq!(
            config.beacon_base_url.as_str(),
            "https://beacon.example/prefix/"
        );
        assert_eq!(config.bind.to_string(), "0.0.0.0:3000");
        assert_eq!(config.beacon_timeout.as_millis(), 10_000);
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn rejects_credentials_query_and_invalid_bounds() {
        for url in ["https://u:p@x.test", "https://x.test/?q=1", "file:///tmp/x"] {
            assert!(parse(&[("BEACON_BASE_URL", url)]).is_err());
        }
        assert!(parse(&[("BEACON_BASE_URL", "https://x.test"), ("PORT", "80")]).is_err());
        assert!(parse(&[
            ("BEACON_BASE_URL", "https://x.test"),
            ("BEACON_TIMEOUT_MS", "0")
        ])
        .is_err());
    }

    #[test]
    fn accepts_ipv6_and_hostnames_and_validates_log_level() {
        let ipv6 = parse(&[("BEACON_BASE_URL", "https://x.test"), ("HOST", "::1")]).unwrap();
        assert_eq!(ipv6.bind, "[::1]:3000");
        let hostname =
            parse(&[("BEACON_BASE_URL", "https://x.test"), ("HOST", "localhost")]).unwrap();
        assert_eq!(hostname.bind, "localhost:3000");
        let silent = parse(&[
            ("BEACON_BASE_URL", "https://x.test"),
            ("LOG_LEVEL", "silent"),
        ])
        .unwrap();
        assert_eq!(silent.log_level, "silent");
        for (key, value) in [("HOST", ""), ("HOST", "bad host"), ("LOG_LEVEL", "verbose")] {
            assert!(parse(&[("BEACON_BASE_URL", "https://x.test"), (key, value)]).is_err());
        }
    }

    #[test]
    fn subscription_defaults_and_strict_bounds() {
        let config = parse(&[("BEACON_BASE_URL", "https://beacon.example/prefix")]).unwrap();
        assert_eq!(
            config.subscriptions.ws_url.as_str(),
            "wss://beacon.example/ws"
        );
        assert!(config.subscriptions.enabled);
        assert_eq!(config.subscriptions.max_streams, 64);
        assert!(parse(&[
            ("BEACON_BASE_URL", "https://beacon.example"),
            ("BEACON_WS_URL", "https://example/ws")
        ])
        .is_err());
        assert!(parse(&[
            ("BEACON_BASE_URL", "https://beacon.example"),
            ("BEACON_SUBSCRIPTIONS_MAX_STREAMS", "0")
        ])
        .is_err());
        assert!(parse(&[
            ("BEACON_BASE_URL", "https://beacon.example"),
            ("BEACON_SUBSCRIPTIONS_ENABLED", "yes")
        ])
        .is_err());
    }
}
