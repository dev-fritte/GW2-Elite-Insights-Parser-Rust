//! Event model, indices and log metadata on top of `evtc-ingest`.
//!
//! Ported from the GW2 Elite Insights parser (MIT License, Copyright (c) 2018 baaron4), see the
//! `LICENSE` file in the repository root.

pub mod api;
pub mod buff_events;
pub mod csr;
pub mod damage;
pub mod encounter;
pub mod graphs;
pub mod keep;
pub mod mechanics;
pub mod meta;
pub mod model;
pub mod offense;
pub mod phases;
pub mod pipeline;
pub mod players;
pub mod rules;
pub mod spec;
mod special;
pub mod subphases;

/// Constants generated from the C# sources by `tools/gen`. Do not edit by hand.
pub mod generated {
    pub mod state_change;
}

pub use model::{AgentId, Model};
