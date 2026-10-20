//! Shared, transport-neutral browser engine contracts.

pub mod contracts;
pub mod limits;

#[cfg(feature = "native-engine")]
#[doc(hidden)]
pub mod native_engine;
