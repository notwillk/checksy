//! Private durable-state substrate for the pull-based agent.
//!
//! The legacy `check` and `install` commands deliberately do not use this
//! module yet.  It is kept crate-private until the later apply/status work can
//! expose the complete public contract without bypassing authentication or
//! promotion invariants.

#![allow(dead_code)]

pub(crate) mod identity;
pub(crate) mod integrity;
pub(crate) mod model;
pub(crate) mod store;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod store_tests;
