#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod config;
pub mod prompts;
pub mod request_logging;
pub mod result;
pub mod server;
pub mod subscriptions;
