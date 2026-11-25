//! `BuffSimulator`, `BuffSimulatorDuration` and `BuffSimulatorIntensity`.

use ei_data::buffs::StackType;

use crate::logic::StackLogic;
use crate::{DELAY, FULL_REMOVAL_BUT_IGNORE_FOR_WASTE, Src, Stack, UNKNOWN_SRC};

/// Why a simulation cannot run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A queue based buff needs a capacity above 1 (C#: `InvalidDataException`).
    CapacityTooSmall,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CapacityTooSmall => {
                f.write_str("queue logic based buffs need a capacity above 1")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A wasted or overstacked amount of duration (`BuffSimulationItemWasted`, `...Overstack`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Waste {
    /// Source of the lost duration.
    pub src: Src,
    /// Lost duration.
    pub value: i64,
    /// Time of the loss.
    pub time: i64,
}

/// A time segment with a constant set of stacks (`BuffSimulationItemDuration`/`...Intensity`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimItem {
    /// Start of the segment.
    pub start: i64,
    /// End of the segment.
    pub end: i64,
    /// Sources of the stacks, in stack order.
    pub sources: Vec<Src>,
    /// Number of stacks that are active (1 for duration buffs, all for intensity buffs).
    pub active_stacks: u32,
}

impl SimItem {
    /// Length of the segment.
    #[must_use]
    pub fn duration(&self) -> i64 {
        self.end - self.start
    }

    /// `GetClampedDuration`: the part of the segment inside `[start, end]`.
    #[must_use]
    pub fn clamped_duration(&self, start: i64, end: i64) -> i64 {
        if start >= end || start >= self.end || self.start >= end {
            return 0;
        }
        (self.end.clamp(start, end) - self.start.clamp(start, end)).max(0)
    }
}

/// An event for the simulator, already reduced to what `BuffEvent.UpdateSimulator` passes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimEvent {
    /// `BuffApplyEvent`.
    Apply {
        /// Time of the event.
        time: i64,
        /// Applied duration.
        duration: i64,
        /// Source credited for the stack.
        src: Src,
        /// Healing score of the source (for `Regeneration`).
        src_healing: u32,
        /// Stack instance id, 0 if none.
        stack_id: u32,
        /// The stack is added as the active one.
        added_active: bool,
        /// Duration of the replaced regeneration stack, 0 if unknown.
        overridden_duration: i64,
        /// Instance id of the replaced regeneration stack, 0 if unknown.
        overridden_stack_id: u32,
    },
    /// `BuffExtensionEvent` (ignored if the extension is below 1).
    Extend {
        /// Time of the event.
        time: i64,
        /// Added duration.
        extension: i64,
        /// Duration before the extension.
        old_value: i64,
        /// Source credited for the extension.
        src: Src,
        /// Healing score of the source.
        src_healing: u32,
        /// Stack instance id, 0 if none.
        stack_id: u32,
    },
    /// `BuffRemoveAllEvent`.
    RemoveAll {
        /// Time of the event.
        time: i64,
        /// Number of removed stacks (see `FULL_REMOVAL_BUT_IGNORE_FOR_WASTE`).
        removed_stacks: i32,
    },
    /// `BuffRemoveSingleEvent`.
    RemoveSingle {
        /// Time of the event.
        time: i64,
        /// Duration of the removed stack.
        removed_duration: i64,
    },
    /// `BuffStackActiveEvent`.
    Activate {
        /// Time of the event.
        time: i64,
        /// Stack instance id.
        stack_id: u32,
    },
    /// `BuffStackDeactiveEvent`; no effect without stack id handling.
    Deactivate {
        /// Time of the event.
        time: i64,
    },
}

impl SimEvent {
    /// Time of the event.
    #[must_use]
    pub fn time(&self) -> i64 {
        match *self {
            Self::Apply { time, .. }
            | Self::Extend { time, .. }
            | Self::RemoveAll { time, .. }
            | Self::RemoveSingle { time, .. }
            | Self::Activate { time, .. }
            | Self::Deactivate { time } => time,
        }
    }
}

/// Simulates the stacks of one buff on one agent.
#[derive(Debug)]
pub struct Simulator {
    logic: StackLogic,
    intensity: bool,
    capacity: usize,
    stacks: Vec<Stack>,
    next_uid: u64,
    last_src_remove: (Src, bool),
    last_src_removes: Vec<(Src, bool)>,
    /// The segments of the simulation.
    pub generation: Vec<SimItem>,
    /// Duration lost by removals and replaced stacks.
    pub wastes: Vec<Waste>,
    /// Duration of stacks that could not be added.
    pub overstacks: Vec<Waste>,
}

/// Arguments of a new stack, to keep the constructor call readable.
struct NewStack {
    start: i64,
    duration: i64,
    src: Src,
    seed: Src,
    is_extension: bool,
    stack_id: u32,
    healing: u32,
}

impl Simulator {
    /// Creates a simulator for a buff of the given stack type and capacity.
    ///
    #[must_use]
    pub fn new(stack_type: StackType, capacity: usize) -> Self {
        let intensity = matches!(
            stack_type,
            StackType::Stacking
                | StackType::StackingUniquePerSrc
                | StackType::StackingConditionalLoss
        );
        Self {
            logic: StackLogic::for_stack_type(stack_type),
            intensity,
            capacity,
            stacks: Vec::new(),
            next_uid: 0,
            last_src_remove: (UNKNOWN_SRC, false),
            last_src_removes: Vec::new(),
            generation: Vec::new(),
            wastes: Vec::new(),
            overstacks: Vec::new(),
        }
    }

    /// `true` if the buff stacks in intensity (as opposed to duration).
    #[must_use]
    pub fn is_intensity(&self) -> bool {
        self.intensity
    }

    fn new_stack(&mut self, n: &NewStack) -> Stack {
        self.next_uid += 1;
        Stack {
            uid: self.next_uid,
            start: n.start,
            duration: n.duration,
            src: n.src,
            seed_src: n.seed,
            is_extension: n.is_extension,
            stack_id: n.stack_id,
            extensions: Vec::new(),
            seed_healing: n.healing,
        }
    }

    fn is_full(&self) -> bool {
        self.logic.is_full(&self.stacks, self.capacity)
    }

    fn add_stack(
        &mut self,
        to_add: &Stack,
        added_active: bool,
        overridden_duration: i64,
        overridden_stack_id: u32,
    ) -> Result<(), Error> {
        if self.is_full() {
            let (src, duration, start) = (to_add.src, to_add.duration, to_add.start);
            let kept = self.logic.find_lowest_value(
                to_add.clone(),
                &mut self.stacks,
                &mut self.wastes,
                overridden_duration,
                overridden_stack_id,
            )?;
            if !kept {
                self.overstacks.push(Waste {
                    src,
                    value: duration,
                    time: start,
                });
            }
        } else {
            self.logic.add(&mut self.stacks, to_add.clone());
        }
        if added_active {
            self.logic.activate_item(&mut self.stacks, to_add);
        }
        Ok(())
    }

    fn push_wastes(&mut self, index: usize, time: i64) {
        let stack = &self.stacks[index];
        self.wastes.push(Waste {
            src: stack.src,
            value: stack.duration,
            time,
        });
        for &(src, value) in &stack.extensions {
            self.wastes.push(Waste { src, value, time });
        }
    }

    fn apply(&mut self, event: SimEvent) -> Result<(), Error> {
        match event {
            SimEvent::Apply {
                time,
                duration,
                src,
                src_healing,
                stack_id,
                added_active,
                overridden_duration,
                overridden_stack_id,
            } => {
                let stack = self.new_stack(&NewStack {
                    start: time,
                    duration,
                    src,
                    seed: src,
                    is_extension: false,
                    stack_id,
                    healing: src_healing,
                });
                self.add_stack(
                    &stack,
                    added_active,
                    overridden_duration,
                    overridden_stack_id,
                )
            }
            SimEvent::Extend {
                time,
                extension,
                old_value,
                src,
                src_healing,
                stack_id,
            } => {
                if extension < 1 {
                    return Ok(()); // no need to bother with 0 extensions
                }
                self.extend(time, extension, old_value, (src, src_healing), stack_id)
            }
            SimEvent::RemoveAll {
                time,
                removed_stacks,
            } => {
                if removed_stacks != FULL_REMOVAL_BUT_IGNORE_FOR_WASTE {
                    for i in 0..self.stacks.len() {
                        self.push_wastes(i, time);
                    }
                }
                self.stacks.clear();
                Ok(())
            }
            SimEvent::RemoveSingle {
                time,
                removed_duration,
            } => {
                if let Some(i) = self
                    .stacks
                    .iter()
                    .position(|s| (removed_duration - s.total_duration()).abs() < DELAY)
                {
                    self.push_wastes(i, time);
                    self.stacks.remove(i);
                }
                Ok(())
            }
            SimEvent::Activate { stack_id, .. } => {
                self.logic.activate_id(&mut self.stacks, stack_id);
                Ok(())
            }
            SimEvent::Deactivate { .. } => Ok(()),
        }
    }

    fn extend(
        &mut self,
        time: i64,
        extension: i64,
        old_value: i64,
        (src, healing): (Src, u32),
        stack_id: u32,
    ) -> Result<(), Error> {
        let extend_existing = (!self.stacks.is_empty() && old_value > 0) || self.is_full();
        if self.intensity {
            if extend_existing {
                let index = (0..self.stacks.len())
                    .min_by_key(|&i| ((self.stacks[i].total_duration() - old_value).abs(), i));
                if let Some(i) = index {
                    self.stacks[i].extensions.push((src, extension));
                }
            } else if self.last_src_removes.is_empty() {
                let stack = self.new_stack(&NewStack {
                    start: time,
                    duration: old_value + extension,
                    src,
                    seed: src,
                    is_extension: false,
                    stack_id,
                    healing,
                });
                self.add_stack(&stack, true, 0, 0)?;
            } else {
                let (seed, is_extension) = self.last_src_removes.remove(0);
                let stack = self.new_stack(&NewStack {
                    start: time,
                    duration: old_value + extension,
                    src,
                    seed,
                    is_extension,
                    stack_id,
                    healing,
                });
                self.add_stack(&stack, false, 0, 0)?;
            }
        } else if extend_existing {
            if let Some(first) = self.stacks.first_mut() {
                first.extensions.push((src, extension));
            }
        } else {
            let (seed, is_extension) = self.last_src_remove;
            let stack = self.new_stack(&NewStack {
                start: time,
                duration: old_value + extension,
                src,
                seed,
                is_extension,
                stack_id,
                healing,
            });
            self.add_stack(&stack, true, 0, 0)?;
        }
        Ok(())
    }

    fn update(&mut self, mut time_passed: i64) {
        while !self.stacks.is_empty() && time_passed > 0 {
            time_passed = if self.intensity {
                self.update_intensity(time_passed)
            } else {
                self.update_duration(time_passed)
            };
        }
    }

    /// Returns the time left over after the segment.
    fn update_duration(&mut self, time_passed: i64) -> i64 {
        self.last_src_remove = (UNKNOWN_SRC, false);
        let start = self.stacks[0].start;
        let mut item = SimItem {
            start,
            end: start + self.stacks[0].duration,
            sources: self.stacks.iter().map(|s| s.src).collect(),
            active_stacks: 1,
        };
        let mut diff = time_passed;
        let mut left_over = 0;
        if self.stacks[0].duration - time_passed < 0 {
            diff = self.stacks[0].duration;
            left_over = time_passed - diff;
        }
        if item.end > item.start + diff {
            item.end = item.start + diff;
        }
        self.generation.push(item);
        self.stacks[0].shift(0, diff);
        for stack in &mut self.stacks {
            stack.shift(diff, 0);
        }
        if self.stacks[0].duration == 0 {
            self.last_src_remove = (self.stacks[0].seed_src, self.stacks[0].is_extension);
            self.stacks.remove(0);
        }
        left_over
    }

    fn update_intensity(&mut self, time_passed: i64) -> i64 {
        let start = self.stacks[0].start;
        let max = self.stacks.iter().map(|s| s.duration).max().unwrap_or(0);
        let mut item = SimItem {
            start,
            end: start + max,
            sources: self.stacks.iter().map(|s| s.src).collect(),
            active_stacks: u32::try_from(self.stacks.len()).unwrap_or(u32::MAX),
        };
        let min = self.stacks.iter().map(|s| s.duration).min().unwrap_or(0);
        let diff = min.min(time_passed);
        let left_over = time_passed - diff;
        if item.end > item.start + diff {
            item.end = item.start + diff;
        }
        self.generation.push(item);
        if left_over == 0 {
            self.last_src_removes.clear();
        }
        for stack in &mut self.stacks {
            stack.shift(diff, diff);
            if stack.duration == 0 && left_over == 0 {
                self.last_src_removes
                    .push((stack.seed_src, stack.is_extension));
            }
        }
        self.stacks.retain(|s| s.duration != 0);
        left_over
    }

    /// Runs the events, which must be sorted by time, over `[log_start, log_end]`.
    /// Does nothing if the simulator already ran.
    ///
    /// # Errors
    /// See [`Error`].
    pub fn simulate(
        &mut self,
        events: &[SimEvent],
        log_start: i64,
        log_end: i64,
    ) -> Result<(), Error> {
        if !self.generation.is_empty() {
            return Ok(());
        }
        let mut previous = events
            .first()
            .map_or(log_start, |e| e.time().min(log_start));
        for event in events {
            let now = event.time();
            debug_assert!(now >= previous, "negative passed time in buff simulation");
            self.update(now - previous);
            self.apply(*event)?;
            previous = now;
        }
        self.update(log_end - previous);
        // `Trim`: the last items must not run past the end of the log
        for item in self.generation.iter_mut().rev() {
            if item.end > log_end {
                item.end = log_end;
            } else {
                break;
            }
        }
        self.generation.retain(|i| i.duration() > 0);
        self.stacks.clear();
        Ok(())
    }
}

/// Uptime over `[start, end]`: for duration buffs the share of time with a stack, for intensity
/// buffs the average number of stacks.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn uptime(items: &[SimItem], start: i64, end: i64, intensity: bool) -> f64 {
    if end <= start {
        return 0.0;
    }
    let weighted: f64 = items
        .iter()
        .map(|i| {
            let factor = if intensity {
                f64::from(i.active_stacks)
            } else {
                1.0
            };
            i.clamped_duration(start, end) as f64 * factor
        })
        .sum();
    weighted / (end - start) as f64
}

#[cfg(test)]
mod tests;
