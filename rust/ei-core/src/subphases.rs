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
                    .next()
            };
            let enter = first_of(&[sc::ENTER_COMBAT]).or_else(|| first_of(&[sc::SPAWN]));
            start = start.min(enter.unwrap_or(target.first));
            let dead = first_of(&[sc::CHANGE_DEAD]);
            let despawn = self
                .of_target(target, &[sc::DESPAWN])
                .map(|e| e.time - self.offset)
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
            .filter(|e| skill_ids.contains(&i64::from(e.skill_id)))
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

/// The sub phases of an encounter, or `None` if its phases are not ported. Times are relative to
/// the start of the fight, which lasts `duration`.
#[must_use]
pub fn encounter_phases(
    kept: &Kept,
    targets: &[Target],
    offset: i64,
    duration: i64,
) -> Option<Vec<SubPhase>> {
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
