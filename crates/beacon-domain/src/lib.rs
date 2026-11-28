#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod evidence;
pub mod geography;
pub mod identities;
pub mod polling;
pub mod radio;
pub mod resources;
pub mod semantics;
pub mod telemetry;
pub mod time;
