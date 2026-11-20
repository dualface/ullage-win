//! A daemon on another host, reached through its HTTP API.
//!
//! Nothing here knows about windows: it runs, and is tested, anywhere.

pub mod address;
pub mod client;
pub mod fetch;
pub mod http;
pub mod pairing;
pub mod resolve;
pub mod store;
#[cfg(test)]
mod testserver;
