//! Buff stack simulation, a port of the C# `BuffSimulator` classes without agent ids
//! (`BuffSimulatorDuration` and `BuffSimulatorIntensity`).
//!
//! Ported from the GW2 Elite Insights parser (MIT License, Copyright (c) 2018 baaron4), see the
//! `LICENSE` file in the repository root.
//!
//! The simulator is fed [`SimEvent`]s in time order and produces [`SimItem`]s: time segments with
//! the number of stacks, plus waste and overstack records. Agents are plain `u32` ids.
//! Not ported: the stack-id variants (`BuffSimulatorID*`), the distribution of the items over
//! sources (`BuffDistribution`), englobing agents.
//!
//! Known difference: the C# `HealingLogic` sorts with the unstable `List.Sort`; ties in the
//! healing score keep their insertion order here.

mod logic;
mod simulator;

pub use logic::StackLogic;
pub use simulator::{Error, SimEvent, SimItem, Simulator, Waste, uptime};

/// Agent id of a source; [`UNKNOWN_SRC`] if the source is not known.
pub type Src = u32;
/// Source of stacks whose origin is unknown (`ParserHelper._unknownAgent`).
pub const UNKNOWN_SRC: Src = u32::MAX;
/// `ParserHelper.BuffSimulatorDelayConstant`: allowed difference when a removal is matched to a stack.
pub const DELAY: i64 = 15;
/// `BuffRemoveAllEvent.FullRemovalButIgnoreForWaste`.
pub const FULL_REMOVAL_BUT_IGNORE_FOR_WASTE: i32 = i32::MAX - 1;

/// One stack of a buff (`BuffStackItem`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    pub(crate) uid: u64,
    /// Time the stack starts at (moves with the simulation).
    pub start: i64,
    /// Remaining duration.
    pub duration: i64,
    /// Current source (changes to the source of an extension when the stack runs out).
    pub src: Src,
    /// Source that created the stack.
    pub seed_src: Src,
    /// `true` if the stack currently runs on an extension.
    pub is_extension: bool,
    /// Stack instance id of the log, 0 if none.
    pub stack_id: u32,
    /// Extensions queued behind the duration.
    pub extensions: Vec<(Src, i64)>,
    /// Healing score of the seed source, used to sort `Regeneration` stacks.
    pub seed_healing: u32,
}

impl Stack {
    /// Duration including all extensions.
    #[must_use]
    pub fn total_duration(&self) -> i64 {
        self.duration + self.extensions.iter().map(|e| e.1).sum::<i64>()
    }

    pub(crate) fn shift(&mut self, start_shift: i64, duration_shift: i64) {
        self.start += start_shift;
        self.duration -= duration_shift;
        if self.duration == 0 && !self.extensions.is_empty() {
            let (src, duration) = self.extensions.remove(0);
            self.src = src;
            self.duration = duration;
            self.is_extension = true;
        }
    }
}
