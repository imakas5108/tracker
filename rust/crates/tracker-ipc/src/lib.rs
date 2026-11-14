//! The local control channel between the Tracker app and its MCP bridge.
//!
//! - [`protocol`]: request/response envelopes (version 1, length-prefixed JSON).
//! - [`catalog`]: every tool's name, description, JSON schema and hints, plus argument
//!   validation. The app validates with the same catalog the bridge advertises.
//! - [`socket`]: a private Unix-domain socket (0700 directory, 0600 socket, peer UID checks,
//!   single-owner lock). There is no network listener.

pub mod catalog;
pub mod protocol;
pub mod socket;

pub use catalog::{Tool, catalog, tool, validate};
pub use protocol::{ErrorBody, Request, Response, PROTOCOL_VERSION};
