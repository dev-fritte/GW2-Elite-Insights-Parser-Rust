//! Sub phases of the encounter logics (`GetPhases` of the logics): the phases that split a fight by
//! an invulnerability of the main target or by its health.
//!
//! Ported: the encounters in [`encounter_phases`]. The others have their own phase code in C#.
//! Names are not computed; the phases are compared by their start and end.

use evtc_ingest::CombatRecord;

use crate::encounter::Target;
use crate::generated::state_change as sc;
use crate::keep::Kept;
use crate::phases::active_segments;
use ei_data::skill_ids as skill;
use ei_data::species_ids::target_id as species;

/// `ParserHelper.PhaseTimeLimit`: shorter phases are dropped.
const PHASE_TIME_LIMIT: i64 = 1000;
/// `ParserHelper.ServerDelayConstant`.
const SERVER_DELAY: i64 = 10;

/// A sub phase of the fight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubPhase {
    /// Start, relative to the start of the fight.
    pub start: i64,
    /// End, relative to the start of the fight.
    pub end: i64,
    /// Indices of the targets of the phase.
    pub targets: Vec<usize>,
}

struct Fight<'a> {
    kept: &'a Kept,
    targets: &'a [Target],
    offset: i64,
    duration: i64,
}

impl Fight<'_> {
    fn redirect(&self, a: u64) -> u64 {
        *self.kept.agent_redirection.get(&a).unwrap_or(&a)
    }

    /// Records of a state change that are about the target (as source).
    fn of_target<'b>(
        &'b self,
        target: &'b Target,
        states: &'b [u8],
    ) -> impl Iterator<Item = &'b CombatRecord> {
        self.kept.events.iter().filter(move |e| {
            states.contains(&e.state_change)
                && e.src_agent != 0
                && target.addresses.contains(&self.redirect(e.src_agent))
        })
    }

    /// The times a target is neither down, dead nor gone.
    fn actives(&self, target: &Target) -> Vec<(i64, i64)> {
        let status: Vec<(i64, u8)> = self
            .of_target(
                target,
                &[
                    sc::CHANGE_DOWN,
                    sc::CHANGE_UP,
                    sc::CHANGE_DEAD,
                    sc::SPAWN,
                    sc::DESPAWN,
                ],
            )
            .map(|e| (e.time - self.offset, e.state_change))
            .collect();
        active_segments(&status, target.first, target.last)
    }

    /// `PhaseData.AddTarget`: the target counts if it is active inside the phase.
    fn add_target(&self, phase: &mut SubPhase, index: usize) {
        let intersects = self
            .actives(&self.targets[index])
            .iter()
            .any(|&(s, e)| e.min(phase.end) - s.max(phase.start) > 0);
        if intersects && !phase.targets.contains(&index) {
            phase.targets.push(index);
        }
    }

    fn add_targets_of(&self, phase: &mut SubPhase, species_ids: &[i32]) {
        for (index, target) in self.targets.iter().enumerate() {
            if species_ids.contains(&target.species) {
                self.add_target(phase, index);
            }
        }
    }

    /// `PhaseData.OverrideTimes`: the phase lasts as long as its targets are around.
    fn fit(&self, phase: &mut SubPhase) {
        if phase.targets.is_empty() {
            return;
        }
        let (mut start, mut end) = (i64::MAX, i64::MIN);
        for &index in &phase.targets {
            let target = &self.targets[index];
            let first_of = |states: &[u8]| {
                self.of_target(target, states)
                    .map(|e| e.time - self.offset)
                    .find(|t| (target.first..=target.last).contains(t))
            };
            let enter = first_of(&[sc::ENTER_COMBAT]).or_else(|| first_of(&[sc::SPAWN]));
            start = start.min(enter.unwrap_or(target.first));
            let dead = first_of(&[sc::CHANGE_DEAD]);
            let despawn = self
                .of_target(target, &[sc::DESPAWN])
                .map(|e| e.time - self.offset)
                .filter(|t| (target.first..=target.last).contains(t))
                .last();
            end = end.max(dead.or(despawn).unwrap_or(target.last));
        }
        phase.start = phase.start.max(start).max(0);
        phase.end = phase.end.min(end).min(self.duration);
    }

    /// `GetBuffApplyRemoveSequence` with a dummy removal at the end: (time, is apply).
    fn invuln_sequence(
        &self,
        ids: &[i64],
        target: &Target,
        begin_with_apply: bool,
    ) -> Vec<(i64, bool)> {
        let mut all: Vec<(i64, bool)> = Vec::new();
        for &id in ids {
            let main: Vec<(i64, bool)> = self
                .kept
                .events
                .iter()
                .filter(|e| i64::from(e.skill_id) == id)
                .filter_map(|e| match e.state_change {
                    sc::BUFF_APPLY | sc::BUFF_INITIAL
                        if e.dst_agent != 0
                            && target.addresses.contains(&self.redirect(e.dst_agent)) =>
                    {
                        Some((e.time - self.offset, true))
                    }
                    // the source of a removal is the one that holds the buff
                    sc::BUFF_REMOVE_ALL
                        if e.src_agent != 0
                            && target.addresses.contains(&self.redirect(e.src_agent)) =>
                    {
                        Some((e.time - self.offset, false))
                    }
                    _ => None,
                })
                .collect();
            let mut need_start = begin_with_apply;
            let mut filtered: Vec<(i64, bool)> = Vec::new();
            for (i, &(time, apply)) in main.iter().enumerate() {
                if need_start && apply {
                    need_start = false;
                    if let Some(&(prev, _)) = filtered.last()
                        && time - prev < SERVER_DELAY
                    {
                        filtered.pop();
                        continue;
                    }
                    filtered.push((time, true));
                } else if !need_start && !apply {
                    // only the last removal before another application counts
                    if main.get(i + 1).is_none_or(|n| n.1) {
                        need_start = true;
                        filtered.push((time, false));
                    }
                }
            }
            if filtered.last().is_some_and(|l| l.1) {
                filtered.push((target.last, false));
            }
            all.extend(filtered);
        }
        all.sort_by_key(|e| e.0);
        all
    }

    /// `GetSubPhasesByInvul`.
    fn by_invuln(
        &self,
        ids: &[i64],
        target_index: usize,
        add_skip: bool,
        begin_with_start: bool,
        (start, end): (i64, i64),
    ) -> Vec<SubPhase> {
        let sequence = self.invuln_sequence(ids, &self.targets[target_index], begin_with_start);
        let mut last = start;
        let mut phases: Vec<SubPhase> = Vec::new();
        let new = |s: i64, e: i64| SubPhase {
            start: s,
            end: e,
            targets: Vec::new(),
        };
        let mut next_is_skip = !begin_with_start;
        for (time, apply) in sequence.into_iter().filter(|e| e.0 >= 0) {
            let cur_end = time.min(end);
            if apply {
                phases.push(new(last, cur_end));
                last = cur_end.max(start);
                next_is_skip = true;
            } else {
                if add_skip {
                    phases.push(new(last, cur_end));
                }
                last = cur_end.max(start);
                next_is_skip = false;
            }
        }
        if !next_is_skip || add_skip {
            phases.push(new(last, end));
        }
        phases.retain(|p| p.end - p.start > 100);
        phases
    }

    /// The health percent updates of a target: (time, percent).
    fn health_updates(&self, target: &Target) -> Vec<(i64, f64)> {
        self.of_target(target, &[sc::HEALTH_UPDATE])
            .map(|e| {
                #[allow(clippy::cast_precision_loss)]
                let percent = e.dst_agent as f64 / 100.0;
                (e.time - self.offset, percent)
            })
            .collect()
    }

    /// `GetPhasesByHealthPercent`.
    fn by_health(
        &self,
        target_index: usize,
        thresholds: &[f64],
        (start, end): (i64, i64),
    ) -> Vec<SubPhase> {
        let mut phases: Vec<SubPhase> = Vec::new();
        if thresholds.is_empty() {
            return phases;
        }
        let updates = self.health_updates(&self.targets[target_index]);
        let mut phase_start = start;
        for &threshold in thresholds {
            let Some(&(time, _)) = updates.iter().find(|u| u.1 <= threshold) else {
                break;
            };
            let mut phase = SubPhase {
                start: phase_start,
                end: time.min(end),
                targets: Vec::new(),
            };
            self.add_target(&mut phase, target_index);
            phases.push(phase);
            phase_start = time.max(start);
        }
        if !phases.is_empty() && phases.len() < thresholds.len() {
            let mut last = SubPhase {
                start: phase_start,
                end,
                targets: Vec::new(),
            };
            self.add_target(&mut last, target_index);
            phases.push(last);
        }
        phases
    }

    /// The animated casts of skills by a target: (start, end, skill) of each, by start.
    fn animated_casts_of(&self, target: &Target, skill_ids: &[i64]) -> Vec<(i64, i64, i64)> {
        let events: Vec<&CombatRecord> = self
            .of_target(target, &[sc::ANIMATION_START, sc::ANIMATION_STOP])
            .filter(|e| skill_ids.is_empty() || skill_ids.contains(&i64::from(e.skill_id)))
            .collect();
        let mut casts = Vec::new();
        for (i, e) in events.iter().enumerate() {
            if e.state_change != sc::ANIMATION_START {
                continue;
            }
            let start = e.time - self.offset;
            let end = events[i + 1..]
                .iter()
                .find(|n| n.state_change == sc::ANIMATION_STOP && n.skill_id == e.skill_id)
                .map_or(start, |n| n.time - self.offset);
            if (0..=self.duration).contains(&start) {
                casts.push((start, end, i64::from(e.skill_id)));
            }
        }
        casts
    }

    /// The animated casts of one skill by a target: (start, end) of each.
    fn animated_casts(&self, target: &Target, skill_id: i64) -> Vec<(i64, i64)> {
        self.animated_casts_of(target, &[skill_id])
            .into_iter()
            .map(|(s, e, _)| (s, e))
            .collect()
    }

    /// Sabir: a phase ends with each walloping wind and the next one starts with the attack that
    /// follows it.
    fn sabir(&self, main: usize) -> Vec<SubPhase> {
        let target = &self.targets[main];
        let casts = self.animated_casts_of(
            target,
            &[
                skill::WALLOPING_WIND,
                skill::STORMS_EDGE_RIGHT_HAND,
                skill::STORMS_EDGE_LEFT_HAND,
                skill::CHAIN_LIGHTNING,
            ],
        );
        let mut phases = Vec::new();
        let mut start = 0;
        let mut count = 0;
        for &(time, end, _) in casts.iter().filter(|c| c.2 == skill::WALLOPING_WIND) {
            phases.push(SubPhase {
                start,
                end: time,
                targets: Vec::new(),
            });
            let Some(next) = casts
                .iter()
                .find(|c| c.0 >= end && c.2 != skill::WALLOPING_WIND)
            else {
                break;
            };
            start = next.0;
            count += 1;
        }
        if count > 0 {
            phases.push(SubPhase {
                start,
                end: self.duration,
                targets: Vec::new(),
            });
        }
        for phase in &mut phases {
            self.add_target(phase, main);
        }
        phases
    }

    /// Slothasor: the fight is split by his narcolepsy casts.
    fn slothasor(&self, main: usize) -> Vec<SubPhase> {
        let mut phases = Vec::new();
        let mut start = 0;
        for (cast_start, cast_end) in
            self.animated_casts(&self.targets[main], skill::NARCOLEPSY_SKILL)
        {
            phases.push(SubPhase {
                start,
                end: cast_start.min(self.duration),
                targets: Vec::new(),
            });
            start = cast_end;
        }
        phases.push(SubPhase {
            start,
            end: self.duration,
            targets: Vec::new(),
        });
        for phase in &mut phases {
            self.add_target(phase, main);
        }
        phases
    }

    /// Sabetha: her phases alternate with the one mini boss that is around (they are tried in
    /// reverse order of appearance).
    fn sabetha(&self, main: usize) -> Vec<SubPhase> {
        const BOSSES: [i32; 3] = [species::KARDE, species::KNUCKLES, species::KERNAN];
        let mut phases = self.by_invuln(
            &[skill::INVULNERABILITY757],
            main,
            true,
            true,
            (0, self.duration),
        );
        for (i, phase) in phases.iter_mut().enumerate() {
            if (i + 1) % 2 == 0 {
                for boss in BOSSES {
                    self.add_targets_of(phase, &[boss]);
                    self.fit(phase);
                    if !phase.targets.is_empty() {
                        break;
                    }
                }
            } else {
                self.add_target(phase, main);
            }
            self.add_targets_of(phase, &BOSSES);
        }
        phases
    }

    /// The phases of a golem: its numbers (by health) and the health steps.
    fn golem(&self, main: usize) -> Vec<SubPhase> {
        let updates = self.health_updates(&self.targets[main]);
        let mut phases = Vec::new();
        if updates.is_empty() {
            return phases;
        }
        let thresholds = [80.0, 60.0, 40.0, 20.0, 0.0];
        for &threshold in &thresholds[..4] {
            if let Some(&(time, _)) = updates.iter().find(|u| u.1 <= threshold) {
                let mut phase = SubPhase {
                    start: 0,
                    end: time,
                    targets: Vec::new(),
                };
                self.add_target(&mut phase, main);
                phases.push(phase);
            }
        }
        phases.extend(self.by_health(main, &thresholds, (0, self.duration)));
        phases
    }

    /// Whether the target gains a buff during the fight (`Determined895` marks the healed phase of
    /// the challenge mode of Ura).
    fn gains_buff(&self, target: &Target, buff: i64) -> bool {
        self.kept.events.iter().any(|e| {
            e.state_change == sc::BUFF_APPLY
                && i64::from(e.skill_id) == buff
                && e.dst_agent != 0
                && target.addresses.contains(&self.redirect(e.dst_agent))
                && (0..=self.duration).contains(&(e.time - self.offset))
        })
    }

    /// The stacks of an intensity buff on a target at a time (applications minus removals).
    fn stacks_at(&self, target: &Target, buff: i64, time: i64) -> i64 {
        let mut stacks = 0i64;
        for e in self
            .kept
            .events
            .iter()
            .filter(|e| i64::from(e.skill_id) == buff)
        {
            if e.time - self.offset > time {
                break;
            }
            let on = |a: u64| a != 0 && target.addresses.contains(&self.redirect(a));
            match e.state_change {
                sc::BUFF_APPLY | sc::BUFF_INITIAL if on(e.dst_agent) => stacks += 1,
                sc::BUFF_REMOVE_SINGLE if on(e.src_agent) => stacks = (stacks - 1).max(0),
                sc::BUFF_REMOVE_ALL if on(e.src_agent) => stacks = 0,
                _ => {}
            }
        }
        stacks
    }

    /// Ura (normal mode): the fight is split at 70% and 40% of her health. `None` for the
    /// challenge mode with its healed phase.
    fn ura(&self, main: usize) -> Option<Vec<SubPhase>> {
        if self.gains_buff(&self.targets[main], skill::DETERMINED895) {
            return None;
        }
        let end = self.duration;
        let updates = self.health_updates(&self.targets[main]);
        let first_below = |limit: f64| {
            updates
                .iter()
                .find(|u| u.1 < limit && u.1 != 0.0)
                .map(|u| u.0)
        };
        let new = |start: i64, end: i64| SubPhase {
            start,
            end,
            targets: Vec::new(),
        };
        let mut phases = Vec::new();
        let hp70 = first_below(70.0);
        phases.push(new(0, hp70.unwrap_or(end)));
        if let Some(hp70) = hp70 {
            if let Some(hp40) = first_below(40.0) {
                phases.push(new(hp70, hp40.min(end)));
                phases.push(new(hp40, end));
            }
            phases.push(new(hp70, end));
        }
        for phase in &mut phases {
            self.add_target(phase, main);
        }
        Some(phases)
    }

    /// Decima (normal mode): the fight alternates with her nova shield. The shield she gets while
    /// she enrages (many charges) does not make a phase.
    fn decima(&self, main: usize) -> Vec<SubPhase> {
        let mut phases =
            self.by_invuln(&[skill::NOVA_SHIELD], main, true, true, (0, self.duration));
        for (i, phase) in phases.iter_mut().enumerate() {
            let enrage = (i + 1) % 2 == 0
                && self.stacks_at(
                    &self.targets[main],
                    skill::CHARGE_DECIMA,
                    phase.start + SERVER_DELAY,
                ) >= 10;
            if !enrage {
                self.add_target(phase, main);
            }
        }
        phases
    }

    /// The times (relative to the start of the fight) of the buff events of a skill, in order.
    fn buff_times(&self, buff: i64, removal_only: bool) -> Vec<i64> {
        self.kept
            .events
            .iter()
            .filter(|e| {
                i64::from(e.skill_id) == buff
                    && match e.state_change {
                        sc::BUFF_REMOVE_ALL | sc::BUFF_REMOVE_SINGLE => true,
                        sc::BUFF_APPLY | sc::BUFF_INITIAL => !removal_only,
                        _ => false,
                    }
            })
            .map(|e| e.time - self.offset)
            .collect()
    }

    /// Matthias: ice, fire, storm and abomination phase, each starting with a buff of the next
    /// element.
    fn matthias(&self, main: usize) -> Vec<SubPhase> {
        let new = |start: i64, end: i64| SubPhase {
            start,
            end,
            targets: Vec::new(),
        };
        let end = self.duration;
        let first = |buff: i64| self.buff_times(buff, false).first().copied();
        let mut phases = Vec::new();
        if let Some(heat_wave) = first(skill::HEAT_WAVE_MATTHIAS) {
            phases.push(new(0, heat_wave));
            if let Some(downpour) = first(skill::DOWNPOUR_MATTHIAS) {
                phases.push(new(heat_wave, downpour));
                if let Some(unstable) = first(skill::UNSTABLE) {
                    phases.push(new(downpour, unstable));
                    let target = &self.targets[main];
                    let removal = self
                        .kept
                        .events
                        .iter()
                        .filter(|e| {
                            i64::from(e.skill_id) == skill::INVULNERABILITY757
                                && matches!(
                                    e.state_change,
                                    sc::BUFF_REMOVE_ALL | sc::BUFF_REMOVE_SINGLE
                                )
                                && e.src_agent != 0
                                && target.addresses.contains(&self.redirect(e.src_agent))
                        })
                        .map(|e| e.time - self.offset)
                        .find(|&t| t >= unstable && t <= unstable + 10_000);
                    if let Some(removal) = removal {
                        phases.push(new(removal, end));
                    }
                } else {
                    phases.push(new(downpour, end));
                }
            } else {
                phases.push(new(heat_wave, end));
            }
        }
        for phase in &mut phases {
            self.add_target(phase, main);
            self.add_targets_of(phase, &[species::MATTHIAS_SACRIFICE_CRYSTAL]);
        }
        phases
    }

    /// Whisper of Jormag: the times before the doppelgangers (teleports at 75% and 25%) and the
    /// final phase.
    fn whisper_of_jormag(&self, main: usize) -> Vec<SubPhase> {
        let mut out = self.buff_times(skill::WHISPER_TELEPORT_OUT, true);
        let mut back = self.buff_times(skill::WHISPER_TELEPORT_BACK, true);
        let new = |start: i64, end: i64| SubPhase {
            start,
            end,
            targets: Vec::new(),
        };
        let mut phases = Vec::new();
        if let Some(&end) = out.iter().min() {
            phases.push(new(0, end));
            out.retain(|&t| t > end + 1000);
        }
        if let Some(&start) = back.iter().min() {
            let end = if let Some(&end) = out.iter().min() {
                back.retain(|&t| t > end);
                end
            } else {
                back.clear();
                self.duration
            };
            phases.push(new(start, end));
            if let Some(&start) = back.iter().min() {
                phases.push(new(start, self.duration));
            }
        }
        for phase in &mut phases {
            self.add_target(phase, main);
        }
        phases
    }

    /// Qadim: phases alternate with his invulnerability; while he is invulnerable the other
    /// targets (hydra, apocalypse bringer, wyverns) fight, afterwards the pyres burn.
    fn qadim(&self, main: usize, pyre_starts: &[i64]) -> Vec<SubPhase> {
        let mut phases = self.by_invuln(
            &[skill::QADIM_INVULNERABLE],
            main,
            true,
            false,
            (0, self.duration),
        );
        for (i, phase) in phases.iter_mut().enumerate() {
            if (i + 1) % 2 == 0 {
                let latest = pyre_starts
                    .iter()
                    .copied()
                    .filter(|t| (phase.start..=phase.end).contains(t))
                    .max();
                if let Some(latest) = latest
                    && latest > phase.start
                {
                    phase.start = latest;
                }
                self.add_target(phase, main);
            } else {
                self.add_targets_of(
                    phase,
                    &[
                        species::WYVERN_MATRIARCH,
                        species::WYVERN_PATRIARCH,
                        species::ANCIENT_INVOKED_HYDRA,
                        species::APOCALYPSE_BRINGER,
                        species::QADIM_LAMP,
                    ],
                );
                self.fit(phase);
            }
        }
        phases.retain(|p| p.start < p.end);
        phases
    }

    /// Dhuum: the pre event, the fight up to 10%, the shielded Dhuum and the ritual, and in
    /// between the soul splits.
    fn dhuum(&self, main: usize) -> Vec<SubPhase> {
        let end = self.duration;
        let target = &self.targets[main];
        let casts = self.animated_casts_of(target, &[]);
        let has_pre_event = !casts.iter().any(|c| {
            c.2 != skill::WEAPON_STOW && c.2 != skill::WEAPON_DRAW && (0..=40_000).contains(&c.0)
        });
        let new = |start: i64, end: i64| SubPhase {
            start,
            end,
            targets: Vec::new(),
        };
        let mut phases: Vec<SubPhase> = Vec::new();
        // the part of the fight in which the soul splits happen: its start and its fight phase
        let mut main_start = None;
        let mut fight_end = None;
        let mut fight = |start: i64, phases: &mut Vec<SubPhase>| {
            let Some(shield) = casts.iter().find(|c| c.2 == skill::MAJOR_SOUL_SPLIT) else {
                return;
            };
            let shield = shield.0;
            phases.push(new(start, shield));
            fight_end = Some(shield);
            let vulnerable = casts
                .iter()
                .find(|c| c.2 == skill::DHUUM_VULNERABLE_LAST10_PERCENT && c.0 >= shield);
            if let Some(vulnerable) = vulnerable {
                phases.push(new(shield, vulnerable.0));
                phases.push(new(vulnerable.0, end));
            } else {
                phases.push(new(shield, end));
            }
        };
        if has_pre_event {
            let released = self
                .kept
                .events
                .iter()
                .filter(|e| {
                    e.state_change == sc::BUFF_REMOVE_ALL
                        && i64::from(e.skill_id) == skill::DETERMINED762
                        && e.src_agent != 0
                        && target.addresses.contains(&self.redirect(e.src_agent))
                })
                .map(|e| e.time - self.offset)
                .find(|&t| t > 115_000);
            if let Some(pre_end) = released {
                phases.push(new(0, pre_end));
                phases.push(new(pre_end, end));
                main_start = Some(pre_end);
                fight(pre_end, &mut phases);
            }
        } else {
            main_start = Some(0);
            fight(0, &mut phases);
        }
        if let Some(start) = main_start {
            let splits_end = fight_end.unwrap_or(end);
            phases.extend(self.soul_splits(&casts, start, splits_end));
        }
        for phase in &mut phases {
            self.add_target(phase, main);
        }
        // the pre event also has the enforcers as targets
        if has_pre_event
            && let Some(pre) = phases.first_mut()
            && pre.start == 0
        {
            self.add_targets_of(pre, &[species::DHUUMS_ENFORCER]);
        }
        phases
    }

    /// The phases between the soul splits of Dhuum.
    fn soul_splits(&self, casts: &[(i64, i64, i64)], start: i64, end: i64) -> Vec<SubPhase> {
        let cycles: Vec<_> = casts
            .iter()
            .filter(|c| c.2 == skill::CATACLYSMIC_CYCLE)
            .collect();
        let marks: Vec<_> = casts
            .iter()
            .filter(|c| c.2 == skill::GREATER_DEATH_MARK)
            .collect();
        if marks.len() < cycles.len() {
            return Vec::new();
        }
        let new = |start: i64, end: i64| SubPhase {
            start,
            end,
            targets: Vec::new(),
        };
        let mut phases = Vec::new();
        let mut from = start;
        for (cycle, mark) in cycles.iter().zip(&marks) {
            let split_start = mark.0.min(end);
            let mut pre = new(from, split_start);
            self.add_targets_of(&mut pre, &[species::DHUUMS_ENFORCER]);
            phases.push(pre);
            phases.push(new(split_start, cycle.1.min(end)));
            from = cycle.1;
        }
        phases.push(new(from, end));
        phases
    }

    /// The times a target gains (`apply`) or loses all of (`!apply`) a buff, in order.
    fn buff_times_on(&self, target: &Target, buff: i64, apply: bool) -> Vec<i64> {
        self.kept
            .events
            .iter()
            .filter(|e| i64::from(e.skill_id) == buff)
            .filter(|e| {
                if apply {
                    e.state_change == sc::BUFF_APPLY
                        && e.dst_agent != 0
                        && target.addresses.contains(&self.redirect(e.dst_agent))
                } else {
                    e.state_change == sc::BUFF_REMOVE_ALL
                        && e.src_agent != 0
                        && target.addresses.contains(&self.redirect(e.src_agent))
                }
            })
            .map(|e| e.time - self.offset)
            .collect()
    }

    /// Fraenir of Jormag: his first and last part of the fight and the icebrood construct in between.
    fn fraenir(&self, main: usize) -> Vec<SubPhase> {
        let new = |start: i64, end: i64| SubPhase {
            start,
            end,
            targets: Vec::new(),
        };
        let fraenir = &self.targets[main];
        let icebrood = self
            .targets
            .iter()
            .position(|t| t.species == species::ICEBROOD_CONSTRUCT_FRAENIR);
        let Some(&split) = self
            .buff_times_on(fraenir, skill::DETERMINED762, true)
            .first()
        else {
            return Vec::new();
        };
        // the first phase stands for the whole fight, which is not part of the result
        let mut phases = vec![new(0, self.duration), new(0, split)];
        if let Some(ice) = icebrood.map(|i| &self.targets[i]) {
            let enter = self
                .of_target(ice, &[sc::ENTER_COMBAT])
                .map(|e| e.time - self.offset)
                .last();
            if let Some(enter) = enter {
                let mut ice_end = self.duration;
                let applied = self.buff_times_on(ice, skill::INVULNERABILITY757, true);
                let removed = self.buff_times_on(ice, skill::INVULNERABILITY757, false);
                if let (Some(&apply), Some(&remove)) = (applied.first(), removed.first()) {
                    phases.push(new(enter + 1, apply));
                    let back = self
                        .buff_times_on(fraenir, skill::DETERMINED762, false)
                        .first()
                        .copied();
                    if let Some(back) = back {
                        let death = self
                            .of_target(ice, &[sc::CHANGE_DEAD])
                            .map(|e| e.time - self.offset)
                            .last();
                        ice_end = death.unwrap_or(back - 1);
                        phases.push(new(back, self.duration));
                    }
                    phases.push(new(remove, ice_end));
                }
                phases.push(new(enter, ice_end));
            }
        }
        phases.sort_by_key(|p| p.start);
        for (i, phase) in phases.iter_mut().enumerate().skip(1) {
            if i == 1 || i == 5 {
                self.add_target(phase, main);
            } else if let Some(ice) = icebrood {
                self.add_target(phase, ice);
            }
        }
        phases.remove(0);
        phases
    }

    /// The time frames in which a target does not have a buff, from the start of the fight to its
    /// end.
    fn buff_free_segments(&self, target: &Target, buff: i64) -> Vec<(i64, i64)> {
        let mut changes: Vec<(i64, bool)> = Vec::new();
        for e in self
            .kept
            .events
            .iter()
            .filter(|e| i64::from(e.skill_id) == buff)
        {
            let on = |a: u64| a != 0 && target.addresses.contains(&self.redirect(a));
            match e.state_change {
                sc::BUFF_APPLY | sc::BUFF_INITIAL if on(e.dst_agent) => {
                    changes.push((e.time - self.offset, true));
                }
                sc::BUFF_REMOVE_ALL if on(e.src_agent) => {
                    changes.push((e.time - self.offset, false));
                }
                _ => {}
            }
        }
        let mut segments = Vec::new();
        let mut from = 0;
        let mut present = false;
        for (time, gained) in changes {
            if gained == present {
                continue;
            }
            if !present {
                segments.push((from, time));
            }
            from = time;
            present = gained;
        }
        if !present {
            segments.push((from, self.duration));
        }
        segments
    }

    /// The time frames in which a target has a buff.
    fn buff_present_segments(&self, target: &Target, buff: i64) -> Vec<(i64, i64)> {
        let free = self.buff_free_segments(target, buff);
        let mut present = Vec::new();
        let mut from = 0;
        for &(start, end) in &free {
            if start > from {
                present.push((from, start));
            }
            from = end;
        }
        if from < self.duration {
            present.push((from, self.duration));
        }
        // a buff present from the start would have a free segment of zero length
        present.retain(|p| p.0 < p.1);
        present
    }

    /// Kardinal Adina: splits while she is determined (with the hands), main phases between the
    /// quakes (all six pillars pandemonium at one time).
    fn adina(&self, main: usize) -> Vec<SubPhase> {
        const HANDS: [i32; 2] = [species::HAND_OF_EROSION, species::HAND_OF_ERUPTION];
        let target = &self.targets[main];
        let end = self.duration;
        let mut splits = Vec::new();
        let mut split_ends = Vec::new();
        let invuls = self.buff_present_segments(target, skill::DETERMINED762);
        for &(start, stop) in &invuls {
            let mut phase = SubPhase {
                start,
                end: stop,
                targets: Vec::new(),
            };
            split_ends.push(stop);
            self.add_targets_of(&mut phase, &HANDS);
            self.fit(&mut phase);
            splits.push(phase);
        }
        // the quakes: all six pillars at the same time
        let applies = self.buff_times_on(target, skill::PILLAR_PANDEMONIUM, true);
        let mut quakes: Vec<i64> = Vec::new();
        for &time in &applies {
            if !quakes.contains(&time) && applies.iter().filter(|&&t| t == time).count() == 6 {
                quakes.push(time);
            }
        }
        let barrage = self
            .animated_casts(target, skill::BOULDER_BARRAGE)
            .into_iter()
            .find(|c| c.0 < 6000);
        let mut start = barrage.map_or(0, |c| c.1);
        let mut mains: Vec<(i64, i64)> = Vec::new();
        if quakes.is_empty() {
            if start > 0 && invuls.is_empty() {
                mains.push((start, end));
            }
        } else {
            for &quake in &quakes {
                let mut current = split_ends
                    .iter()
                    .rfind(|&&e| e < quake)
                    .copied()
                    .unwrap_or(0);
                if current == 0 {
                    current = start;
                }
                if let Some(&next) = split_ends.iter().find(|&&e| e > quake) {
                    start = next;
                }
                mains.push((current, quake));
            }
            if mains.last().is_some_and(|m| m.0 != start) {
                mains.push((start, end));
            }
        }
        let mut phases = splits;
        for (from, to) in mains {
            let mut phase = SubPhase {
                start: from,
                end: to,
                targets: Vec::new(),
            };
            self.add_target(&mut phase, main);
            for (index, t) in self.targets.iter().enumerate() {
                if HANDS.contains(&t.species) && (from..=to).contains(&t.first) {
                    self.add_target(&mut phase, index);
                }
            }
            phases.push(phase);
        }
        phases
    }

    /// Statue of Darkness: from the loss of the first invulnerability each eye has a phase
    /// whenever it is not determined.
    fn statue_of_darkness(&self) -> Vec<SubPhase> {
        let mut phases = Vec::new();
        for id in [species::EYE_OF_FATE, species::EYE_OF_JUDGEMENT] {
            let Some(index) = self.targets.iter().position(|t| t.species == id) else {
                continue;
            };
            let eye = &self.targets[index];
            let Some(&loss) = self.buff_times_on(eye, skill::DETERMINED762, false).first() else {
                continue;
            };
            for (start, end) in self.buff_free_segments(eye, skill::DETERMINED895) {
                let mut phase = SubPhase {
                    start: loss.max(start),
                    end: end.min(self.duration),
                    targets: Vec::new(),
                };
                self.add_target(&mut phase, index);
                phases.push(phase);
            }
        }
        phases
    }

    /// Eparch (challenge mode): phases alternate with the splits of the incarnations.
    fn eparch(&self, main: usize) -> Vec<SubPhase> {
        let split = [
            species::INCARNATION_OF_CRUELTY,
            species::INCARNATION_OF_JUDGEMENT,
            species::KRYPTIS_RIFT,
        ];
        // without the incarnations it is the normal mode, which has no phases
        if !self.targets.iter().any(|t| {
            t.species == species::INCARNATION_OF_CRUELTY
                || t.species == species::INCARNATION_OF_JUDGEMENT
        }) {
            return Vec::new();
        }
        let mut phases = self.by_invuln(
            &[skill::DETERMINED762],
            main,
            true,
            true,
            (0, self.duration),
        );
        for (i, phase) in phases.iter_mut().enumerate() {
            if (i + 1) % 2 == 0 {
                self.add_targets_of(phase, &split);
            } else {
                self.add_target(phase, main);
            }
        }
        phases
    }

    /// Bandit trio: one phase per bandit, from the last time it enters combat to its death.
    fn bandit_trio(&self) -> Vec<SubPhase> {
        let mut phases = Vec::new();
        for id in [species::BERG, species::ZANE, species::NARELLA] {
            let Some(index) = self.targets.iter().position(|t| t.species == id) else {
                continue;
            };
            let target = &self.targets[index];
            let Some(start) = self
                .of_target(target, &[sc::ENTER_COMBAT])
                .map(|e| e.time - self.offset)
                .filter(|&t| t >= 0)
                .last()
            else {
                continue;
            };
            let death = self
                .of_target(target, &[sc::CHANGE_DEAD])
                .map(|e| e.time - self.offset)
                .last();
            let mut phase = SubPhase {
                start,
                end: death.unwrap_or(self.duration).min(self.duration),
                targets: Vec::new(),
            };
            self.add_target(&mut phase, index);
            phases.push(phase);
        }
        phases
    }

    /// Whispering Shadow: a phase before each guttering light and the darkness that lasts until
    /// the shadow is stunned or its breakbar breaks.
    fn whispering_shadow(&self, main: usize) -> Vec<SubPhase> {
        let target = &self.targets[main];
        let breakbar_ends: Vec<i64> = self
            .of_target(target, &[sc::BREAKBAR_STATE])
            .filter(|e| e.value != 0)
            .map(|e| e.time - self.offset)
            .collect();
        let stuns: Vec<i64> = self
            .kept
            .events
            .iter()
            .filter(|e| {
                e.state_change == sc::BUFF_APPLY
                    && i64::from(e.skill_id) == skill::STUN
                    && e.dst_agent != 0
                    && target.addresses.contains(&self.redirect(e.dst_agent))
            })
            .map(|e| e.time - self.offset)
            .collect();
        let new = |start: i64, end: i64| SubPhase {
            start,
            end,
            targets: Vec::new(),
        };
        let mut phases = Vec::new();
        let mut start = 0;
        let mut first = true;
        for (cast_start, cast_end, id) in self.animated_casts_of(target, &[]) {
            if cast_end <= cast_start {
                continue;
            }
            if id == skill::GUTTERING_LIGHT || id == skill::GUTTERING_LIGHT_CM {
                if first {
                    phases.push(new(start, cast_start));
                    let stunned = stuns.iter().copied().find(|&t| t > cast_start);
                    let broken = breakbar_ends.iter().copied().find(|&t| t > cast_start);
                    let end = stunned
                        .unwrap_or(self.duration)
                        .min(broken.unwrap_or(i64::MAX));
                    phases.push(new(cast_start, end));
                    start = end;
                }
                first = false;
            } else {
                first = true;
            }
        }
        if start < self.duration {
            phases.push(new(start, self.duration));
        }
        for phase in &mut phases {
            self.add_target(phase, main);
        }
        phases
    }

    /// Old Lion's Court: each prototype has phases between its ley woven shieldings.
    fn old_lions_court(&self, challenge_mode: bool) -> Vec<SubPhase> {
        let find = |challenge: i32, normal: i32| {
            self.targets
                .iter()
                .position(|t| t.species == challenge)
                .or_else(|| self.targets.iter().position(|t| t.species == normal))
        };
        let prototypes = [
            find(
                species::PROTOTYPE_VERMILION_CM,
                species::PROTOTYPE_VERMILION,
            ),
            find(species::PROTOTYPE_INDIGO_CM, species::PROTOTYPE_INDIGO),
            find(species::PROTOTYPE_ARSENITE_CM, species::PROTOTYPE_ARSENITE),
        ];
        // Vermilion has to be there and start shielded, otherwise the log is incomplete
        let Some(vermilion) = prototypes[0] else {
            return Vec::new();
        };
        if self.stacks_at(&self.targets[vermilion], skill::LEY_WOVEN_SHIELDING, 500) == 0 {
            return Vec::new();
        }
        let limit = if challenge_mode { 3 } else { 4 };
        let mut phases = Vec::new();
        for index in prototypes.into_iter().flatten() {
            let death = self
                .of_target(&self.targets[index], &[sc::CHANGE_DEAD])
                .map(|e| e.time - self.offset)
                .last();
            let end = death.map_or(self.duration, |d| d.min(self.duration));
            let mut sub = self.by_invuln(
                &[
                    skill::LEY_WOVEN_SHIELDING,
                    skill::MALFUNCTIONING_LEY_WOVEN_SHIELDING,
                ],
                index,
                false,
                true,
                (0, end),
            );
            if sub.len() > limit {
                continue;
            }
            for phase in &mut sub {
                self.add_target(phase, index);
            }
            phases.extend(sub);
        }
        phases
    }

    /// Soulless Horror: the time before each howling death, the howling death and the rest.
    fn soulless_horror(&self, main: usize) -> Vec<SubPhase> {
        let end = self.duration;
        let mut phases = Vec::new();
        let mut start = 0;
        for (cast_start, cast_end) in self.animated_casts(&self.targets[main], skill::HOWLING_DEATH)
        {
            phases.push(SubPhase {
                start,
                end: cast_start.min(end),
                targets: Vec::new(),
            });
            phases.push(SubPhase {
                start: cast_start.min(end),
                end: cast_end.min(end),
                targets: Vec::new(),
            });
            start = cast_end;
        }
        if end - start > PHASE_TIME_LIMIT {
            phases.push(SubPhase {
                start,
                end,
                targets: Vec::new(),
            });
        }
        for phase in &mut phases {
            self.add_target(phase, main);
            self.add_targets_of(phase, &[species::TORMENTED_DEAD]);
        }
        phases
    }

    /// Cerus: phases alternate with the splits of the embodiments, after the last phase the
    /// enraged smash starts a final phase.
    fn cerus(&self, main: usize) -> Vec<SubPhase> {
        const EMBODIMENTS: [i32; 6] = [
            species::EMBODIMENT_OF_DESPAIR,
            species::EMBODIMENT_OF_ENVY,
            species::EMBODIMENT_OF_GLUTTONY,
            species::EMBODIMENT_OF_MALICE,
            species::EMBODIMENT_OF_RAGE,
            species::EMBODIMENT_OF_REGRET,
        ];
        // embodiments that gain the invulnerability of the kill
        let killed: Vec<usize> = self
            .targets
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                EMBODIMENTS.contains(&t.species)
                    && self.kept.events.iter().any(|e| {
                        i64::from(e.skill_id) == skill::INVULNERABILITY757
                            && matches!(
                                e.state_change,
                                sc::BUFF_APPLY
                                    | sc::BUFF_INITIAL
                                    | sc::BUFF_REMOVE_ALL
                                    | sc::BUFF_REMOVE_SINGLE
                            )
                            && (e.dst_agent != 0
                                && t.addresses.contains(&self.redirect(e.dst_agent))
                                || e.src_agent != 0
                                    && t.addresses.contains(&self.redirect(e.src_agent)))
                    })
            })
            .map(|(i, _)| i)
            .collect();
        let invuln_phases = self.by_invuln(
            &[skill::INVULNERABILITY_CERUS],
            main,
            true,
            true,
            (0, self.duration),
        );
        let mut phases = invuln_phases.clone();
        for (i, phase) in phases.iter_mut().enumerate() {
            if (i + 1) % 2 == 0 {
                self.add_targets_of(phase, &EMBODIMENTS);
                self.fit(phase);
                for &index in &killed {
                    let applied = self.kept.events.iter().any(|e| {
                        matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
                            && i64::from(e.skill_id) == skill::INVULNERABILITY757
                            && e.dst_agent != 0
                            && self.targets[index]
                                .addresses
                                .contains(&self.redirect(e.dst_agent))
                            && (phase.start..=phase.end).contains(&(e.time - self.offset))
                    });
                    if applied {
                        self.add_target(phase, index);
                    }
                }
            } else {
                self.add_target(phase, main);
            }
        }
        let smash = self
            .animated_casts_of(
                &self.targets[main],
                &[skill::ENRAGED_SMASH_NM, skill::ENRAGED_SMASH_CM],
            )
            .first()
            .map(|c| c.0);
        if let Some(smash) = smash {
            let mut last = SubPhase {
                start: smash,
                end: self.duration,
                targets: Vec::new(),
            };
            self.add_target(&mut last, main);
            phases.push(last);
            if let Some(third) = invuln_phases
                .iter()
                .rev()
                .find(|p| (p.start..=p.end).contains(&smash))
            {
                let mut before = SubPhase {
                    start: third.start,
                    end: smash,
                    targets: Vec::new(),
                };
                self.add_target(&mut before, main);
                phases.push(before);
            }
        }
        phases
    }

    /// Phases that alternate between the main target and a split into other targets: the odd
    /// phases belong to the main target, the even ones to the split targets.
    fn alternating(
        &self,
        main: usize,
        buff: i64,
        split_species: &[i32],
        fit_splits: bool,
    ) -> Vec<SubPhase> {
        let mut phases = self.by_invuln(&[buff], main, true, true, (0, self.duration));
        for (i, phase) in phases.iter_mut().enumerate() {
            if (i + 1) % 2 == 0 {
                self.add_targets_of(phase, split_species);
                if fit_splits {
                    self.fit(phase);
                }
            } else {
                self.add_target(phase, main);
            }
        }
        phases
    }
}

/// What the phases need to know about agents that are no targets.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    /// The first times of the pyre guardians of Qadim.
    pub pyre_starts: Vec<i64>,
    /// Agents of volatile species that are no regular targets (the lamps of Qadim, the hands of
    /// Adina).
    pub extra_targets: Vec<Target>,
}

/// The sub phases of an encounter, or `None` if its phases are not ported. Times are relative to
/// the start of the fight, which lasts `duration`.
#[must_use]
#[allow(clippy::too_many_lines)] // a table of the encounters
pub fn encounter_phases(
    kept: &Kept,
    targets: &[Target],
    offset: i64,
    duration: i64,
    extras: &Extras,
) -> Option<Vec<SubPhase>> {
    let all_targets: Vec<Target> = targets
        .iter()
        .chain(&extras.extra_targets)
        .cloned()
        .collect();
    let targets = &all_targets[..];
    let fight = Fight {
        kept,
        targets,
        offset,
        duration,
    };
    let trigger = i32::from(kept.trigger_id);
    let main = targets.iter().position(|t| t.species == trigger)?;
    let mut phases = match trigger {
        species::GORSEVAL => fight.alternating(
            main,
            skill::PROTECTIVE_SHADOW,
            &[species::CHARGED_SOUL],
            true,
        ),
        species::VALE_GUARDIAN => fight.alternating(
            main,
            skill::INVULNERABILITY757,
            &[
                species::BLUE_GUARDIAN,
                species::GREEN_GUARDIAN,
                species::RED_GUARDIAN,
            ],
            true,
        ),
        species::SAMAROG => fight.alternating(
            main,
            skill::DETERMINED762,
            &[species::RIGOM, species::GULDHEM],
            true,
        ),
        species::SKORVALD => fight.alternating(
            main,
            skill::DETERMINED762,
            &[
                species::FLUX_ANOMALY_SWORD_SWORD,
                species::FLUX_ANOMALY_AXE_MACE,
                species::FLUX_ANOMALY_SCEPTER_SCEPTER,
                species::FLUX_ANOMALY_HAMMER,
                species::FLUX_ANOMALY_SWORD_SWORD_CM,
                species::FLUX_ANOMALY_AXE_MACE_CM,
                species::FLUX_ANOMALY_SCEPTER_SCEPTER_CM,
                species::FLUX_ANOMALY_HAMMER_CM,
            ],
            true,
        ),
        species::SIAX => fight.alternating(
            main,
            skill::DETERMINED762,
            &[species::ECHO_OF_THE_UNCLEAN],
            true,
        ),
        species::MAMA => fight.alternating(
            main,
            skill::DETERMINED762,
            &[
                species::BLUE_KNIGHT,
                species::GREEN_KNIGHT,
                species::RED_KNIGHT,
            ],
            true,
        ),
        species::SABETHA => fight.sabetha(main),
        species::SLOTHASOR => fight.slothasor(main),
        species::SABIR => fight.sabir(main),
        species::ICEBROOD_CONSTRUCT => {
            let mut phases = fight.by_invuln(
                &[skill::INVULNERABILITY757],
                main,
                false,
                true,
                (0, duration),
            );
            for phase in &mut phases {
                fight.add_target(phase, main);
            }
            phases
        }
        species::ENSOLYSS => {
            // every phase belongs to Ensolyss himself
            let mut phases =
                fight.by_invuln(&[skill::DETERMINED762], main, true, true, (0, duration));
            for phase in &mut phases {
                fight.add_target(phase, main);
            }
            phases
        }
        species::CERUS => fight.cerus(main),
        species::ADINA => fight.adina(main),
        species::EYE_OF_FATE => fight.statue_of_darkness(),
        species::FRAENIR_OF_JORMAG => fight.fraenir(main),
        species::DHUUM => fight.dhuum(main),
        species::QADIM => fight.qadim(main, &extras.pyre_starts),
        species::EPARCH_LONELY_TOWER => fight.eparch(main),
        species::BERG => fight.bandit_trio(),
        species::WHISPERING_SHADOW => fight.whispering_shadow(main),
        species::PROTOTYPE_VERMILION => fight.old_lions_court(false),
        species::PROTOTYPE_VERMILION_CM => fight.old_lions_court(true),
        species::MATTHIAS => fight.matthias(main),
        species::WHISPER_OF_JORMAG => fight.whisper_of_jormag(main),
        species::SOULLESS_HORROR => fight.soulless_horror(main),
        species::ARTSARIIV => fight.alternating(
            main,
            skill::DETERMINED762,
            &[species::CLONE_ARTSARIIV],
            true,
        ),
        species::URA => fight.ura(main)?,
        species::DECIMA => fight.decima(main),
        species::MURSAAT_OVERSEER => fight.by_health(main, &[75.0, 50.0, 25.0, 0.0], (0, duration)),
        16_169 | 16_202 | 16_178 | 16_198 | 16_177 | 16_199 | 19_676 | 19_645 | 16_174 | 16_176 => {
            fight.golem(main)
        }
        _ => return None,
    };
    // `LogData`: phases without targets and very short phases are dropped, the rest is sorted
    finish(&mut phases);
    Some(phases)
}

/// `LogData`: phases without targets and very short phases are dropped, the rest is sorted by
/// start, longer phases first.
fn finish(phases: &mut Vec<SubPhase>) {
    phases.retain(|p| !p.targets.is_empty() && p.end - p.start >= PHASE_TIME_LIMIT);
    phases.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then((b.end - b.start).cmp(&(a.end - a.start)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phase(start: i64, end: i64, targets: &[usize]) -> SubPhase {
        SubPhase {
            start,
            end,
            targets: targets.to_vec(),
        }
    }

    #[test]
    fn short_phases_and_phases_without_targets_are_dropped() {
        let mut phases = vec![
            phase(0, 999, &[0]),
            phase(0, 5000, &[]),
            phase(0, 1000, &[0]),
        ];
        finish(&mut phases);
        assert_eq!(phases, vec![phase(0, 1000, &[0])]);
    }

    #[test]
    fn phases_are_sorted_by_start_and_then_by_length() {
        let mut phases = vec![
            phase(5000, 9000, &[0]),
            phase(0, 3000, &[0]),
            phase(0, 8000, &[0]),
        ];
        finish(&mut phases);
        assert_eq!(
            phases,
            vec![
                phase(0, 8000, &[0]),
                phase(0, 3000, &[0]),
                phase(5000, 9000, &[0])
            ]
        );
    }
}
