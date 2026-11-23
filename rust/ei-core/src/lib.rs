//! Event model, indices and log metadata on top of `evtc-ingest`.
//!
//! Ported from the GW2 Elite Insights parser (MIT License, Copyright (c) 2018 baaron4), see the
//! `LICENSE` file in the repository root.

pub mod api;
pub mod csr;
pub mod keep;
pub mod meta;
pub mod model;
pub mod rules;

/// Constants generated from the C# sources by `tools/gen`. Do not edit by hand.
pub mod generated {
    pub mod state_change;
}

pub use model::{AgentId, Model};
