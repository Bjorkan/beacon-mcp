use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde_json::Value;
use thiserror::Error;
use url::Url;

const MAX_BYTES: usize = 5_242_880;
const MAX_URL: usize = 8_192;

fn retry_delay() -> Duration {
    // One jittered retry; use independent OS entropy instead of a fixed delay
    // so many replicas do not retry a transient upstream outage together.
    let mut byte = [0_u8; 1];
    getrandom::fill(&mut byte).expect("OS entropy for retry jitter");
    Duration::from_millis(25 + u64::from(byte[0] % 50))
}

#[cfg(test)]
#[test]
fn retry_jitter_stays_within_legacy_window() {
    for _ in 0..128 {
        assert!((25..=74).contains(&retry_delay().as_millis()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    ServerInfo,
    WsTicket,
    Iatas,
    Regions,
    Scopes,
    Nodes,
    Node,
    Observers,
    Observer,
    ObserverActivity,
    Packets,
    Packet,
    Messages,
    Channels,
    ChannelMessages,
    Routes,
    RouteSearch,
    CrossRoutes,
    Traces,
    Trace,
    TopAdvertisers,
    Overview,
    Series,
    ObserverComparison,
    Brokers,
    ChannelDetail,
    IataDetail,
    IataBorder,
    MessagesBackfill,
    NodeActivity,
    NodeNeighbors,
    NodeObservations,
    ObserverAdverts,
    ObserverTelemetry,
    ObserverDirectory,
    PacketsBackfill,
    RegionDetail,
    RouteObservations,
    ScopeDetail,
    ClockDrift,
    NodeTypes,
    ObservationStats,
    PathStats,
    PayloadBreakdown,
    RadioPresets,
    ScopeStats,
    SignalStats,
    TopNodes,
    TopObservers,
    TopTalkers,
}

impl Operation {
    fn endpoint(self) -> (&'static str, &'static str) {
        use Operation::*;
        match self {
            ServerInfo => ("info", ""),
            WsTicket => ("ws-ticket", ""),
            Iatas => ("iatas", ""),
            Regions => ("regions", ""),
            Scopes => ("scopes", ""),
            Nodes => ("nodes", ""),
            Node => ("nodes", "uuid"),
            Observers => ("observers", ""),
            Observer => ("observers", "uuid"),
            ObserverActivity => ("observers", "uuid/activity"),
            Packets => ("packets", ""),
            Packet => ("packets", "hash"),
            Messages => ("messages", ""),
            Channels => ("channels", ""),
            ChannelMessages => ("channels", "number/messages"),
            Routes => ("routes", ""),
            RouteSearch => ("routes/search", ""),
            CrossRoutes => ("routes/cross", ""),
            Traces => ("traces", ""),
            Trace => ("traces", "tag"),
            TopAdvertisers => ("stats/top-advertisers", ""),
            Overview => ("stats/overview", ""),
            Series => ("stats/series", ""),
            ObserverComparison => ("stats/observer-comparison", ""),
            Brokers => ("brokers", ""),
            ChannelDetail => ("channels", "number"),
            IataDetail => ("iatas", "iata"),
            IataBorder => ("iatas", "iata/border"),
            MessagesBackfill => ("messages/backfill", ""),
            NodeActivity => ("nodes", "uuid/activity"),
            NodeNeighbors => ("nodes", "uuid/neighbors"),
            NodeObservations => ("nodes", "uuid/observations"),
            ObserverAdverts => ("observers", "uuid/adverts"),
            ObserverTelemetry => ("observers", "uuid/telemetry"),
            ObserverDirectory => ("observers/directory", ""),
            PacketsBackfill => ("packets/backfill", ""),
            RegionDetail => ("regions", "number"),
            RouteObservations => ("routes", "route"),
            ScopeDetail => ("scopes", "scope"),
            ClockDrift => ("stats/clock-drift", ""),
            NodeTypes => ("stats/node-types", ""),
            ObservationStats => ("stats/observations", ""),
            PathStats => ("stats/paths", ""),
            PayloadBreakdown => ("stats/payload-breakdown", ""),
            RadioPresets => ("stats/radio-presets", ""),
            ScopeStats => ("stats/scopes", ""),
            SignalStats => ("stats/signal", ""),
            TopNodes => ("stats/top-nodes", ""),
            TopObservers => ("stats/top-observers", ""),
            TopTalkers => ("stats/top-talkers", ""),
        }
    }

    fn expensive(self) -> bool {
        matches!(
            self,
            Self::Overview
                | Self::Series
                | Self::ObserverComparison
                | Self::CrossRoutes
                | Self::NodeActivity
                | Self::ObserverDirectory
                | Self::RouteObservations
                | Self::ClockDrift
                | Self::NodeTypes
                | Self::ObservationStats
                | Self::PathStats
                | Self::PayloadBreakdown
                | Self::RadioPresets
                | Self::ScopeStats
                | Self::SignalStats
                | Self::TopNodes
                | Self::TopObservers
                | Self::TopTalkers
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidArgument,
    NotFound,
    DeadlineExceeded,
    ResponseTooLarge,
    ResourceExhausted,
    Unavailable,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "INVALID_ARGUMENT",
            Self::NotFound => "NOT_FOUND",
            Self::DeadlineExceeded => "DEADLINE_EXCEEDED",
            Self::ResponseTooLarge => "RESPONSE_TOO_LARGE",
            Self::ResourceExhausted => "RESOURCE_EXHAUSTED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct GatewayError {
    pub code: ErrorCode,
    pub message: String,
    pub status: Option<StatusCode>,
    pub upstream_code: Option<String>,
    pub retry_after: Option<String>,
}

impl GatewayError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status: None,
            upstream_code: None,
            retry_after: None,
        }
    }
}

#[derive(Clone)]
pub struct BeaconClient {
    http: Client,
    base: Url,
    timeout: Duration,
    stats_timeout: Duration,
}

#[derive(Debug, PartialEq)]
pub struct ArrayPage {
    pub items: Vec<Value>,
    pub complete: bool,
}

impl BeaconClient {
    pub fn new(base: Url, timeout: Duration, stats_timeout: Duration) -> Self {
        Self {
            http: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("HTTP client"),
            base,
            timeout,
            stats_timeout,
        }
    }

    fn url(
        &self,
        operation: Operation,
        id: Option<&str>,
        query: &[(&str, String)],
    ) -> Result<Url, GatewayError> {
        let (endpoint, kind) = operation.endpoint();
        let mut url = self
            .base
            .join(&format!("api/v1/{endpoint}"))
            .map_err(|_| GatewayError::new(ErrorCode::InvalidArgument, "invalid Beacon URL"))?;
        if !kind.is_empty() {
            let id = id.ok_or_else(|| {
                GatewayError::new(ErrorCode::InvalidArgument, "missing identifier")
            })?;
            let (value, suffix) = match kind {
                "uuid" | "uuid/activity" | "uuid/neighbors" | "uuid/observations"
                | "uuid/adverts" | "uuid/telemetry" => (
                    {
                        let uuid = uuid::Uuid::parse_str(id).map_err(|_| {
                            GatewayError::new(ErrorCode::InvalidArgument, "invalid UUID")
                        })?;
                        if id.len() != 36
                            || uuid.get_version_num() == 0
                            || uuid.get_version_num() > 8
                            || uuid.get_variant() != uuid::Variant::RFC4122
                        {
                            return Err(GatewayError::new(
                                ErrorCode::InvalidArgument,
                                "invalid UUID",
                            ));
                        }
                        uuid.to_string()
                    },
                    kind.split_once('/').map(|(_, suffix)| suffix),
                ),
                "hash" if id.len() == 16 && id.bytes().all(|c| c.is_ascii_hexdigit()) => {
                    (id.to_ascii_lowercase(), None)
                }
                "tag" if id.len() == 8 && id.bytes().all(|c| c.is_ascii_hexdigit()) => {
                    (id.to_ascii_lowercase(), None)
                }
                "number" | "number/messages"
                    if id
                        .parse::<u32>()
                        .is_ok_and(|n| n > 0 && n <= i32::MAX as u32) =>
                {
                    (
                        id.to_owned(),
                        kind.split_once('/').map(|(_, suffix)| suffix),
                    )
                }
                "iata" | "iata/border"
                    if id.len() == 3 && id.bytes().all(|b| b.is_ascii_alphanumeric()) =>
                {
                    (
                        id.to_ascii_uppercase(),
                        kind.split_once('/').map(|(_, suffix)| suffix),
                    )
                }
                "scope"
                    if !id.trim().is_empty()
                        && id.len() <= 128
                        && !id.chars().any(char::is_control)
                        && !matches!(id, "." | "..")
                        && !id.contains('/')
                        && !id.contains('\\')
                        && !id.contains('%') =>
                {
                    (id.to_owned(), None)
                }
                "route" => {
                    let (iata, path_key) = id.split_once(':').ok_or_else(|| {
                        GatewayError::new(ErrorCode::InvalidArgument, "invalid route identifier")
                    })?;
                    if iata.len() != 3
                        || !iata.bytes().all(|b| b.is_ascii_alphanumeric())
                        || path_key.len() != 32
                        || !path_key.bytes().all(|b| b.is_ascii_hexdigit())
                    {
                        return Err(GatewayError::new(
                            ErrorCode::InvalidArgument,
                            "invalid route identifier",
                        ));
                    }
                    let mut segments = url.path_segments_mut().map_err(|_| {
                        GatewayError::new(ErrorCode::InvalidArgument, "invalid Beacon path")
                    })?;
                    segments.push(&iata.to_ascii_uppercase());
                    segments.push(&path_key.to_ascii_lowercase());
                    segments.push("observations");
                    drop(segments);
                    if !query.is_empty() {
                        url.query_pairs_mut()
                            .extend_pairs(query.iter().map(|(key, value)| (*key, value.as_str())));
                    }
                    if url.as_str().len() > MAX_URL {
                        return Err(GatewayError::new(
                            ErrorCode::InvalidArgument,
                            "Beacon request filters are too large",
                        ));
                    }
                    return Ok(url);
                }
                _ => {
                    return Err(GatewayError::new(
                        ErrorCode::InvalidArgument,
                        "invalid identifier",
                    ));
                }
            };
            let mut segments = url.path_segments_mut().map_err(|_| {
                GatewayError::new(ErrorCode::InvalidArgument, "invalid Beacon path")
            })?;
            segments.push(&value);
            if let Some(suffix) = suffix {
                segments.push(suffix);
            }
        } else if id.is_some() {
            return Err(GatewayError::new(
                ErrorCode::InvalidArgument,
                "unexpected identifier",
            ));
        }
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(key, value)| (*key, value.as_str())));
        }
        if url.as_str().len() > MAX_URL {
            return Err(GatewayError::new(
                ErrorCode::InvalidArgument,
                "Beacon request filters are too large",
            ));
        }
        Ok(url)
    }

    /// Only an enum of fixed public GET routes can reach this method; callers cannot supply a host or path.
    pub async fn request(
        &self,
        operation: Operation,
        id: Option<&str>,
        query: &[(&str, String)],
    ) -> Result<Value, GatewayError> {
        let url = self.url(operation, id, query)?;
        for attempt in 0..2 {
            let response = self
                .http
                .get(url.clone())
                .timeout(if operation.expensive() {
                    self.stats_timeout
                } else {
                    self.timeout
                })
                .header(reqwest::header::USER_AGENT, "beacon-mcp/2.2.0")
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(error) if error.is_timeout() => {
                    return Err(GatewayError::new(
                        ErrorCode::DeadlineExceeded,
                        "Beacon request timed out",
                    ));
                }
                Err(_) if attempt == 0 => {
                    tokio::time::sleep(retry_delay()).await;
                    continue;
                }
                Err(_) => {
                    return Err(GatewayError::new(
                        ErrorCode::Unavailable,
                        "Beacon is temporarily unavailable",
                    ));
                }
            };
            let status = response.status();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            if status == StatusCode::NO_CONTENT {
                return Ok(Value::Null);
            }
            let bytes = read_bounded(response).await?;
            let body: Value = if status.is_success() {
                serde_json::from_slice(&bytes).map_err(|_| {
                    GatewayError::new(ErrorCode::Unavailable, "Beacon returned invalid JSON")
                })?
            } else {
                // Error envelopes are optional upstream; status and Retry-After
                // still determine the error when the body is malformed.
                serde_json::from_slice(&bytes).unwrap_or(Value::Null)
            };
            if status.is_success() {
                return Ok(body);
            }
            if attempt == 0 && [502, 503, 504].contains(&status.as_u16()) && retry_after.is_none() {
                tokio::time::sleep(retry_delay()).await;
                continue;
            }
            return Err(classify(status, &body, retry_after));
        }
        unreachable!("two attempts always return")
    }

    /// Incrementally read a top-level array. The item budget bounds transfer and
    /// the byte budget bounds parsing even when the server sends an enormous element.
    pub async fn array_page(
        &self,
        operation: Operation,
        query: &[(&str, String)],
        max_items: usize,
    ) -> Result<ArrayPage, GatewayError> {
        self.array_page_with_id(operation, None, query, max_items).await
    }

    /// Stream allowlisted public GET arrays, including fixed identifier paths.
    /// Callers stop parsing after one extra item confirms truncation.
    pub async fn array_page_with_id(
        &self,
        operation: Operation,
        id: Option<&str>,
        query: &[(&str, String)],
        max_items: usize,
    ) -> Result<ArrayPage, GatewayError> {
        if max_items == 0
            || max_items > 51
            || !matches!(
                operation,
                Operation::Iatas
                    | Operation::Regions
                    | Operation::RouteSearch
                    | Operation::CrossRoutes
                    | Operation::TopAdvertisers
                    | Operation::Brokers
                    | Operation::MessagesBackfill
                    | Operation::NodeNeighbors
                    | Operation::PacketsBackfill
                    | Operation::ClockDrift
                    | Operation::NodeTypes
                    | Operation::ObservationStats
                    | Operation::PayloadBreakdown
                    | Operation::RadioPresets
                    | Operation::ScopeStats
                    | Operation::TopNodes
                    | Operation::TopObservers
                    | Operation::TopTalkers
            )
        {
            return Err(GatewayError::new(
                ErrorCode::InvalidArgument,
                "invalid array page request",
            ));
        }
        let url = self.url(operation, id, query)?;
        for attempt in 0..2 {
            let response = self
                .http
                .get(url.clone())
                .timeout(if operation.expensive() {
                    self.stats_timeout
                } else {
                    self.timeout
                })
                .header(reqwest::header::USER_AGENT, "beacon-mcp/2.2.0")
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(error) if error.is_timeout() => {
                    return Err(GatewayError::new(
                        ErrorCode::DeadlineExceeded,
                        "Beacon request timed out",
                    ));
                }
                Err(_) if attempt == 0 => {
                    tokio::time::sleep(retry_delay()).await;
                    continue;
                }
                Err(_) => {
                    return Err(GatewayError::new(
                        ErrorCode::Unavailable,
                        "Beacon is temporarily unavailable",
                    ));
                }
            };
            if response.status().is_success() {
                return read_array_page(response, max_items).await;
            }
            let status = response.status();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let body = serde_json::from_slice::<Value>(&read_bounded(response).await?)
                .unwrap_or(Value::Null);
            if attempt == 0 && [502, 503, 504].contains(&status.as_u16()) && retry_after.is_none() {
                tokio::time::sleep(retry_delay()).await;
                continue;
            }
            return Err(classify(status, &body, retry_after));
        }
        unreachable!("two attempts always return")
    }
}

fn classify(status: StatusCode, body: &Value, retry_after: Option<String>) -> GatewayError {
    let upstream_code = body
        .pointer("/error/code")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let code = match status.as_u16() {
        400 => ErrorCode::InvalidArgument,
        404 => ErrorCode::NotFound,
        429 => ErrorCode::ResourceExhausted,
        503 if upstream_code.as_deref() == Some("deadline_exceeded") => ErrorCode::DeadlineExceeded,
        _ => ErrorCode::Unavailable,
    };
    let message = if status.is_server_error() {
        format!("Beacon request failed with HTTP {status}")
    } else {
        body.pointer("/error/message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Beacon request failed with HTTP {status}"))
    };
    GatewayError {
        code,
        message,
        status: Some(status),
        upstream_code,
        retry_after,
    }
}

async fn read_array_page(
    mut response: reqwest::Response,
    max_items: usize,
) -> Result<ArrayPage, GatewayError> {
    let mut items = Vec::new();
    let mut buffer = Vec::new();
    let mut scanned = 0;
    let mut start: Option<usize> = None;
    let mut started = false;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut consumed = 0usize;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GatewayError::new(ErrorCode::Unavailable, "Beacon response was interrupted"))?
    {
        consumed += chunk.len();
        if consumed > MAX_BYTES {
            if !items.is_empty() {
                return Ok(ArrayPage {
                    items,
                    complete: false,
                });
            }
            return Err(GatewayError::new(
                ErrorCode::ResponseTooLarge,
                "Beacon response exceeded 5 MiB",
            ));
        }
        buffer.extend_from_slice(&chunk);
        while scanned < buffer.len() {
            let byte = buffer[scanned];
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
                scanned += 1;
                continue;
            }
            if !started {
                if byte == b'[' {
                    started = true;
                } else if !byte.is_ascii_whitespace() {
                    // Some upstream array endpoints also return a JSON page
                    // envelope. Read it with the same byte and item bounds.
                    return read_envelope_page(response, buffer, max_items).await;
                }
            } else if start.is_none() {
                if byte == b']' {
                    return Ok(ArrayPage {
                        items,
                        complete: true,
                    });
                }
                if byte != b',' && !byte.is_ascii_whitespace() {
                    start = Some(scanned);
                    if byte == b'{' || byte == b'[' {
                        depth = 1;
                    } else if byte == b'"' {
                        quoted = true;
                    }
                }
            } else if byte == b'"' {
                quoted = true;
            } else if byte == b'{' || byte == b'[' {
                depth += 1;
            } else if byte == b'}' || (byte == b']' && depth > 0) {
                depth -= 1;
                if depth == 0 {
                    let from = start.take().expect("element start");
                    items.push(
                        serde_json::from_slice(&buffer[from..=scanned])
                            .map_err(|_| invalid_array())?,
                    );
                    if items.len() >= max_items {
                        return Ok(ArrayPage {
                            items,
                            complete: false,
                        });
                    }
                }
            } else if depth == 0 && (byte == b',' || byte == b']') {
                let from = start.take().expect("element start");
                items.push(
                    serde_json::from_slice(&buffer[from..scanned]).map_err(|_| invalid_array())?,
                );
                if byte == b']' {
                    return Ok(ArrayPage {
                        items,
                        complete: true,
                    });
                }
                if items.len() >= max_items {
                    return Ok(ArrayPage {
                        items,
                        complete: false,
                    });
                }
            }
            scanned += 1;
        }
        if let Some(from) = start {
            buffer.drain(..from);
            scanned -= from;
            start = Some(0);
        } else {
            buffer.clear();
            scanned = 0;
        }
    }
    if !items.is_empty() {
        return Ok(ArrayPage {
            items,
            complete: false,
        });
    }
    Err(invalid_array())
}

async fn read_envelope_page(
    mut response: reqwest::Response,
    mut bytes: Vec<u8>,
    max_items: usize,
) -> Result<ArrayPage, GatewayError> {
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GatewayError::new(ErrorCode::Unavailable, "Beacon response was interrupted"))?
    {
        if chunk.len() > MAX_BYTES - bytes.len() {
            return Err(GatewayError::new(
                ErrorCode::ResponseTooLarge,
                "Beacon response exceeded 5 MiB",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_array())?;
    let items = value
        .as_array()
        .or_else(|| value.get("items").and_then(Value::as_array))
        .ok_or_else(invalid_array)?;
    Ok(ArrayPage {
        complete: items.len() <= max_items,
        items: items.iter().take(max_items).cloned().collect(),
    })
}

fn invalid_array() -> GatewayError {
    GatewayError::new(
        ErrorCode::Unavailable,
        "Beacon returned an invalid JSON array",
    )
}

async fn read_bounded(mut response: reqwest::Response) -> Result<Vec<u8>, GatewayError> {
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BYTES as u64)
    {
        return Err(GatewayError::new(
            ErrorCode::ResponseTooLarge,
            "Beacon response exceeded 5 MiB",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GatewayError::new(ErrorCode::Unavailable, "Beacon response was interrupted"))?
    {
        if chunk.len() > MAX_BYTES - bytes.len() {
            return Err(GatewayError::new(
                ErrorCode::ResponseTooLarge,
                "Beacon response exceeded 5 MiB",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
