//! Native authorization primitives; no registered application commands.

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub mod account;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub mod browser;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub mod exchange;
pub mod loopback;
pub mod pkce;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub mod registration;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub mod session;
pub mod token_response;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
#[cfg(test)]
mod account_lifetime_independent_tests;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
#[cfg(test)]
mod registration_independent_tests;
