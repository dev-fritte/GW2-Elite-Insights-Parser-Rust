//! Compares the damage totals of the Rust core with `dpsAll[0]` of the C# JSON.

use std::collections::BTreeMap;

use ei_core::api::Api;
use ei_core::pipeline::analyze_damage;
use serde_json::Value;

/// Counts per compared field.
#[derive(Default)]
pub struct DamageStats {
    pub compared: usize,
    pub mismatches: usize,
    pub examples: Vec<String>,
}

/// Compares all players of one log.
///
/// # Errors
/// The text of the analysis error.
#[allow(clippy::too_many_lines)]
pub fn compare_log(
    data: &[u8],
    api: &Api,
    reference: &Value,
    label: &str,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) -> Result<(), String> {
    use ei_core::damage::DamageTotals;
    let timing = crate::buffs::reference_timing(reference);
    let analysis = analyze_damage(data, api, timing).map_err(|e| e.to_string())?;
    let duration = analysis.duration;
    let players = &analysis.players;
    let theirs_all = reference
        .get("players")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for ours in players {
        let Some(theirs) = theirs_all.iter().find(|p| {
            p.get("account").and_then(Value::as_str).is_some_and(|a| {
                a == ours.player.account || a.trim_start_matches(':') == ours.player.account
            })
        }) else {
            continue;
        };
        let Some(dps) = theirs
            .get("dpsAll")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        else {
            continue;
        };
        let t = ours.totals;
        let span = (0, duration);
        // breakbar damage is a decimal number in the JSON
        {
            let expected = dps
                .get("actorBreakbarDamage")
                .or_else(|| dps.get("actorBreakbarDamage"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let s = stats.entry("breakbar").or_default();
            s.compared += 1;
            if (t.breakbar() - expected).abs() > 1e-9 {
                s.mismatches += 1;
                if s.examples.len() < 5000 {
                    s.examples.push(format!(
                        "{label} {}: rust {} reference {expected}",
                        ours.player.character,
                        t.breakbar()
                    ));
                }
            }
        }
        for (field, key, value) in [
            ("damage", "actorDamage", t.damage),
            ("power", "actorPowerDamage", t.power),
            ("condition", "actorCondiDamage", t.condition),
            (
                "dps",
                "actorDps",
                DamageTotals::per_second(t.damage, span.0, span.1),
            ),
            (
                "powerDps",
                "actorPowerDps",
                DamageTotals::per_second(t.power, span.0, span.1),
            ),
            (
                "condiDps",
                "actorCondiDps",
                DamageTotals::per_second(t.condition, span.0, span.1),
            ),
        ] {
            let expected = dps.get(key).and_then(Value::as_i64).unwrap_or(0);
            let s = stats.entry(field).or_default();
            s.compared += 1;
            if value != expected {
                if std::env::var_os("PARITY_PLAYERS").is_some() && field == "damage" {
                    println!(
                        "DMG {label} | {} | {}",
                        ours.player.character,
                        value - expected
                    );
                }
                s.mismatches += 1;
                if s.examples.len() < 5000 {
                    s.examples.push(format!(
                        "{label} {}: rust {value} reference {expected} (diff {})",
                        ours.player.character,
                        value - expected
                    ));
                }
            }
        }
        let note = || format!("{label} {}", ours.player.character);
        if let Some(all) = theirs
            .get("statsAll")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            compare_offense(&ours.offense, all, "offense", &note, stats);
        }
        if let Some(def) = theirs
            .get("defenses")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            compare_defense(&ours.defense, def, &note, stats);
        }
        compare_target_offense(ours, &analysis.targets, theirs, reference, label, stats);
        compare_targets(
            ours,
            &analysis.targets,
            theirs,
            reference,
            duration,
            label,
            stats,
        );
    }
    Ok(())
}

/// Compares the damage to every target with `dpsTargets[target][0]`. Skipped if the target list of
/// the reference is not exactly ours (fake targets and volatile species are not found by the core).
fn compare_targets(
    ours: &ei_core::pipeline::PlayerDamage,
    targets: &[ei_core::encounter::Target],
    theirs: &Value,
    reference: &Value,
    duration: i64,
    label: &str,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    use ei_core::damage::DamageTotals;
    let expected_ids: Vec<i64> = reference
        .get("targets")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .map(|x| x.get("id").and_then(Value::as_i64).unwrap_or(0))
                .collect()
        })
        .unwrap_or_default();
    let our_ids: Vec<i64> = targets.iter().map(|t| i64::from(t.species)).collect();
    if expected_ids != our_ids {
        return;
    }
    for (index, t) in ours.to_targets.iter().enumerate() {
        let Some(dps) = theirs
            .get("dpsTargets")
            .and_then(|d| d.get(index))
            .and_then(Value::as_array)
            .and_then(|p| p.first())
        else {
            continue;
        };
        for (field, key, value) in [
            ("target damage", "actorDamage", t.damage),
            ("target power", "actorPowerDamage", t.power),
            ("target condition", "actorCondiDamage", t.condition),
            (
                "target dps",
                "actorDps",
                DamageTotals::per_second(t.damage, 0, duration),
            ),
        ] {
            let expected = dps.get(key).and_then(Value::as_i64).unwrap_or(0);
            let s = stats.entry(field).or_default();
            s.compared += 1;
            if value != expected {
                s.mismatches += 1;
                if s.examples.len() < 5000 {
                    s.examples.push(format!(
                        "{label} {} target {index} ({}): rust {value} reference {expected} (diff {})",
                        ours.player.character,
                        targets[index].species,
                        value - expected
                    ));
                }
            }
        }
    }
}

/// Records one compared value.
fn check(
    stats: &mut BTreeMap<&'static str, DamageStats>,
    field: &'static str,
    value: i64,
    reference: &Value,
    key: &str,
    note: &dyn Fn() -> String,
) {
    let Some(expected) = reference.get(key).and_then(Value::as_i64) else {
        return;
    };
    let s = stats.entry(field).or_default();
    s.compared += 1;
    if value != expected {
        s.mismatches += 1;
        if s.examples.len() < 5000 {
            s.examples.push(format!(
                "{key} {}: rust {value} reference {expected} (diff {})",
                note(),
                value - expected
            ));
        }
    }
}

/// Compares the offensive counters with `statsAll[0]` or `statsTargets[target][0]`.
fn compare_offense(
    s: &ei_core::offense::OffensiveStats,
    reference: &Value,
    field: &'static str,
    note: &dyn Fn() -> String,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    for (key, value) in [
        ("totalDamageCount", s.total_damage_count),
        ("totalDmg", s.total_damage),
        ("directDamageCount", s.direct_event_count),
        ("directDmg", s.direct_event_damage),
        ("connectedDamageCount", s.connected_count),
        ("connectedDmg", s.connected_damage),
        ("connectedDirectDamageCount", s.connected_direct_count),
        ("connectedDirectDmg", s.connected_direct_damage),
        ("connectedPowerCount", s.power_count),
        ("connectedPowerDamage", s.power_damage),
        ("connectedPowerAbove90HPCount", s.power_above_90_count),
        ("connectedPowerAbove90HPDamage", s.power_above_90_damage),
        ("connectedLifeLeechCount", s.life_leech_count),
        ("connectedLifeLeechDamage", s.life_leech_damage),
        ("connectedConditionCount", s.condition_count),
        ("connectedConditionDamage", s.condition_damage),
        (
            "connectedConditionAbove90HPCount",
            s.condition_above_90_count,
        ),
        (
            "connectedConditionAbove90HPDamage",
            s.condition_above_90_damage,
        ),
        ("critableDirectDamageCount", s.critable_count),
        ("criticalRate", s.critical_count),
        ("criticalDmg", s.critical_damage),
        ("flankingRate", s.flanking),
        ("againstMovingRate", s.against_moving),
        ("glanceRate", s.glancing),
        ("missed", s.missed),
        ("evaded", s.evaded),
        ("blocked", s.blocked),
        ("interrupts", s.interrupts),
        ("invulned", s.invulned),
        ("killed", s.killed),
        ("downed", s.downed),
        ("againstDownedCount", s.against_downed_count),
        ("againstDownedDamage", s.against_downed_damage),
    ] {
        check(stats, field, value, reference, key, note);
    }
}

/// Compares the defensive counters with `defenses[0]`.
fn compare_defense(
    s: &ei_core::offense::DefensiveStats,
    reference: &Value,
    note: &dyn Fn() -> String,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    for (key, value) in [
        ("damageTaken", s.damage_taken),
        ("damageTakenCount", s.damage_taken_count),
        ("conditionDamageTaken", s.condition_taken),
        ("conditionDamageTakenCount", s.condition_taken_count),
        ("powerDamageTaken", s.power_taken),
        ("powerDamageTakenCount", s.power_taken_count),
        ("strikeDamageTaken", s.strike_taken),
        ("strikeDamageTakenCount", s.strike_taken_count),
        ("lifeLeechDamageTaken", s.life_leech_taken),
        ("lifeLeechDamageTakenCount", s.life_leech_taken_count),
        ("downedDamageTaken", s.downed_taken),
        ("downedDamageTakenCount", s.downed_taken_count),
        ("damageBarrier", s.barrier),
        ("damageBarrierCount", s.barrier_count),
        ("blockedCount", s.blocked),
        ("evadedCount", s.evaded),
        ("missedCount", s.missed),
        ("invulnedCount", s.invulned),
        ("interruptedCount", s.interrupted),
        ("deadCount", s.dead_count),
        ("dcCount", s.dc_count),
    ] {
        check(stats, "defense", value, reference, key, note);
    }
}

/// Compares the offensive counters per target with `statsTargets[target][0]`, in logs whose target
/// list is the one of the reference.
fn compare_target_offense(
    ours: &ei_core::pipeline::PlayerDamage,
    targets: &[ei_core::encounter::Target],
    theirs: &Value,
    reference: &Value,
    label: &str,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    let expected_ids: Vec<i64> = reference
        .get("targets")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .map(|x| x.get("id").and_then(Value::as_i64).unwrap_or(0))
                .collect()
        })
        .unwrap_or_default();
    let our_ids: Vec<i64> = targets.iter().map(|t| i64::from(t.species)).collect();
    if expected_ids != our_ids {
        return;
    }
    for (index, s) in ours.target_offense.iter().enumerate() {
        let Some(reference) = theirs
            .get("statsTargets")
            .and_then(|d| d.get(index))
            .and_then(Value::as_array)
            .and_then(|p| p.first())
        else {
            continue;
        };
        let note = || {
            format!(
                "{label} {} target {index} ({})",
                ours.player.character, targets[index].species
            )
        };
        compare_offense(s, reference, "target offense", &note, stats);
    }
}
