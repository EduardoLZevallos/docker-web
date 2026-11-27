//! Isolation, recovery, concurrency, and cleanup scenarios.
//!
//! The modules stay independent so a contract failure in one scenario cannot
//! suppress the others.

pub mod cleanup;
pub mod concurrency;
pub mod recovery;
