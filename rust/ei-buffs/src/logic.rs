//! The stacking logics of the C# `EffectStackingLogic` folder.

use ei_data::buffs::StackType;

use crate::Stack;
use crate::simulator::{Error, Waste};

/// How stacks are ordered, replaced and activated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackLogic {
    /// `QueueLogic`: stacks in arrival order, the first one runs.
    Queue,
    /// `HealingLogic` (`Regeneration`): a queue sorted by the healing score of the seed source.
    Healing {
        /// Set once a stack-active event was seen; sorting stops then.
        no_sort: bool,
    },
    /// `ForceOverrideLogic`: one stack, a new one replaces it.
    Force,
    /// `OverrideLogic`: stacks sorted by total duration, the shortest is replaced first.
    Override,
}

impl StackLogic {
    /// The logic the C# `BuffSimulator` constructor picks for a stack type.
    #[must_use]
    pub fn for_stack_type(stack_type: StackType) -> Self {
        match stack_type {
            StackType::Queue => Self::Queue,
            StackType::Regeneration => Self::Healing { no_sort: false },
            StackType::Force => Self::Force,
            StackType::Stacking
            | StackType::StackingUniquePerSrc
            | StackType::StackingConditionalLoss => Self::Override,
        }
    }

    pub(crate) fn is_full(&self, stacks: &[Stack], capacity: usize) -> bool {
        match self {
            Self::Force => stacks.len() == 1,
            _ => stacks.len() == capacity,
        }
    }

    pub(crate) fn add(&self, stacks: &mut Vec<Stack>, item: Stack) {
        match self {
            Self::Queue | Self::Force => stacks.push(item),
            Self::Healing { no_sort } => {
                stacks.push(item);
                if !no_sort {
                    // descending healing score; stable, see the crate documentation
                    stacks.sort_by_key(|s| std::cmp::Reverse(s.seed_healing));
                }
            }
            Self::Override => {
                if stacks.is_empty() {
                    stacks.push(item);
                    return;
                }
                let last = isize::try_from(stacks.len() - 1).unwrap_or(isize::MAX);
                let index = binary_search(stacks, item.total_duration(), 0, last);
                if index > stacks.len() - 1 {
                    stacks.push(item);
                } else {
                    stacks.insert(index, item);
                }
            }
        }
    }

    /// Replaces a stack when the buff is full. Returns `false` if `to_add` is dropped (overstack).
    pub(crate) fn find_lowest_value(
        &self,
        to_add: Stack,
        stacks: &mut Vec<Stack>,
        wastes: &mut Vec<Waste>,
        overridden_duration: i64,
        overridden_stack_id: u32,
    ) -> Result<bool, Error> {
        match self {
            Self::Queue => {
                if stacks.len() <= 1 {
                    return Err(Error::CapacityTooSmall);
                }
                // `MinBy` over every stack but the first one: the first minimum wins
                let index = (1..stacks.len())
                    .min_by_key(|&i| (stacks[i].total_duration(), i))
                    .unwrap_or(1);
                replace(stacks, index, to_add, wastes);
                Ok(true)
            }
            Self::Healing { no_sort } => {
                if stacks.len() <= 1 {
                    return Err(Error::CapacityTooSmall);
                }
                let mut index = None;
                if overridden_stack_id > 0 {
                    index = stacks
                        .iter()
                        .position(|s| s.stack_id == overridden_stack_id);
                }
                if index.is_none() && overridden_duration > 0 {
                    index = (0..stacks.len()).min_by_key(|&i| {
                        ((stacks[i].total_duration() - overridden_duration).abs(), i)
                    });
                }
                // old school evtc without stack ids: the stack with the lowest healing score
                let index = index.unwrap_or(stacks.len() - 1);
                replace(stacks, index, to_add, wastes);
                if !no_sort {
                    stacks.sort_by_key(|s| std::cmp::Reverse(s.seed_healing));
                }
                Ok(true)
            }
            Self::Force => {
                if stacks.is_empty() {
                    return Ok(false);
                }
                replace(stacks, 0, to_add, wastes);
                Ok(true)
            }
            Self::Override => {
                if stacks.is_empty() {
                    return Ok(false);
                }
                let removed = stacks.remove(0);
                push_wastes(&removed, wastes);
                self.add(stacks, to_add);
                Ok(true)
            }
        }
    }

    /// `Activate(stacks, stackItem)`: the queue based logics move the stack to the front.
    pub(crate) fn activate_item(&self, stacks: &mut Vec<Stack>, item: &Stack) {
        if matches!(self, Self::Queue | Self::Healing { .. }) {
            // `Remove` is a no-op if the stack is not in the list, `Insert` happens anyway
            if let Some(i) = stacks.iter().position(|s| s.uid == item.uid) {
                stacks.remove(i);
            }
            stacks.insert(0, item.clone());
        }
    }

    /// `Activate(stacks, stackID)`: only `HealingLogic` reacts.
    pub(crate) fn activate_id(&mut self, stacks: &mut Vec<Stack>, stack_id: u32) {
        if let Self::Healing { no_sort } = self
            && let Some(i) = stacks.iter().position(|s| s.stack_id == stack_id)
        {
            *no_sort = true;
            let to_activate = stacks.remove(i);
            if !stacks.is_empty() && stacks[0].total_duration() < 50 {
                stacks[0] = to_activate;
            } else {
                stacks.insert(0, to_activate);
            }
        }
    }
}

fn push_wastes(removed: &Stack, wastes: &mut Vec<Waste>) {
    wastes.push(Waste {
        src: removed.src,
        value: removed.duration,
        time: removed.start,
    });
    for &(src, value) in &removed.extensions {
        wastes.push(Waste {
            src,
            value,
            time: removed.start,
        });
    }
}

fn replace(stacks: &mut [Stack], index: usize, to_add: Stack, wastes: &mut Vec<Waste>) {
    push_wastes(&stacks[index], wastes);
    stacks[index] = to_add;
}

/// Port of `OverrideLogic.BinarySearchBuffStackItem`, including its order of checks.
fn binary_search(stacks: &[Stack], total: i64, min: isize, max: isize) -> usize {
    let at = |i: isize| usize::try_from(i).ok().and_then(|i| stacks.get(i));
    let index = |i: isize| usize::try_from(i).unwrap_or(0);
    if at(min)
        .map(Stack::total_duration)
        .is_some_and(|t| t > total)
    {
        return index(min);
    }
    if at(max)
        .map(Stack::total_duration)
        .is_some_and(|t| t < total)
    {
        return index(max + 1);
    }
    if min > max {
        return index(min);
    }
    let mid = isize::midpoint(min, max);
    let mid_total = at(mid).map_or(i64::MIN, Stack::total_duration);
    match total.cmp(&mid_total) {
        std::cmp::Ordering::Equal => index(mid + 1),
        std::cmp::Ordering::Less => binary_search(stacks, total, min, mid - 1),
        std::cmp::Ordering::Greater => binary_search(stacks, total, mid + 1, max),
    }
}
