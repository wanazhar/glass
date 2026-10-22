//! Native browser engine implementation and shared adapter contracts.
//!
//! `glass-browser-core` is an implementation crate used by `glass-browser`.
//! The browser package owns public sessions and runtime selection. This crate
//! owns shared data contracts and, with the `native-engine` feature, the
//! native browser implementation. The default feature set keeps only the
//! shared contracts available for lightweight adapter builds.
//!
//! This package is separate from the `glass-core` program used for resident
//! agent sessions. Applications should normally depend on `glass-browser`
//! instead of constructing the native engine directly.

pub mod contracts;
pub mod limits;

#[cfg(feature = "native-engine")]
#[doc(hidden)]
pub mod native_engine;
