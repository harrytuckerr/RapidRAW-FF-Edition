//! Native authorization primitives; no registered application commands.

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub mod exchange;
pub mod loopback;
pub mod pkce;
pub mod token_response;
