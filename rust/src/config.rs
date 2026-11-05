use std::{env, net::SocketAddr, time::Duration};
use url::Url;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub beacon_base_url: Url,
    pub beacon_timeout: Duration,
    pub stats_timeout: Duration,
    pub shutdown_grace: Duration,
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
        let bind = format!("{host}:{port}")
            .parse::<SocketAddr>()
            .map_err(|_| "HOST must be a numeric IP address".to_string())?;
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
        Ok(Self {
            bind,
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
    }

    #[test]
    fn rejects_credentials_query_and_invalid_bounds() {
        for url in ["https://u:p@x.test", "https://x.test/?q=1", "file:///tmp/x"] {
            assert!(parse(&[("BEACON_BASE_URL", url)]).is_err());
        }
        assert!(parse(&[("BEACON_BASE_URL", "https://x.test"), ("PORT", "80")]).is_err());
        assert!(
            parse(&[
                ("BEACON_BASE_URL", "https://x.test"),
                ("BEACON_TIMEOUT_MS", "0")
            ])
            .is_err()
        );
    }
}
