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
    compare_mechanics(&analysis, reference, label, stats);
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
        if let Some(sup) = theirs
            .get("support")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            compare_support(&ours.support, sup, &note, stats);
        }
        compare_graphs(ours, &analysis.targets, theirs, reference, &note, stats);
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

/// Records one compared decimal value.
fn check_f(
    stats: &mut BTreeMap<&'static str, DamageStats>,
    field: &'static str,
    value: f64,
    reference: &Value,
    key: &str,
    note: &dyn Fn() -> String,
) {
    let Some(expected) = reference.get(key).and_then(Value::as_f64) else {
        return;
    };
    let s = stats.entry(field).or_default();
    s.compared += 1;
    if (value - expected).abs() > 1e-9 {
        s.mismatches += 1;
        if s.examples.len() < 5000 {
            s.examples.push(format!(
                "{key} {}: rust {value} reference {expected}",
                note()
            ));
        }
    }
}

/// Compares the strips and cleanses with `support[0]`.
fn compare_support(
    s: &ei_core::offense::SupportStats,
    reference: &Value,
    note: &dyn Fn() -> String,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    for (key, value) in [
        ("condiCleanse", s.condition_cleanse),
        ("condiCleanseSelf", s.condition_cleanse_self),
        ("boonStrips", s.boon_strips),
    ] {
        check(stats, "support", value, reference, key, note);
    }
    for (key, value) in [
        ("condiCleanseTime", s.condition_cleanse_time),
        ("condiCleanseTimeSelf", s.condition_cleanse_time_self),
        ("boonStripsTime", s.boon_strips_time),
    ] {
        check_f(stats, "support", value, reference, key, note);
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
        ("boonStrips", s.boon_strips),
        ("conditionCleanses", s.condition_cleanses),
    ] {
        check(stats, "defense", value, reference, key, note);
    }
    for (key, value) in [
        ("boonStripsTime", s.boon_strips_time),
        ("conditionCleansesTime", s.condition_cleanses_time),
    ] {
        check_f(stats, "defense", value, reference, key, note);
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

/// Compares one graph with the reference (a vector of numbers).
fn check_graph(
    stats: &mut BTreeMap<&'static str, DamageStats>,
    field: &'static str,
    ours: &[i64],
    reference: Option<&Value>,
    note: &dyn Fn() -> String,
) {
    let Some(expected) = reference.and_then(Value::as_array) else {
        return;
    };
    let expected: Vec<i64> = expected.iter().filter_map(Value::as_i64).collect();
    let s = stats.entry(field).or_default();
    s.compared += 1;
    if ours != expected.as_slice() {
        s.mismatches += 1;
        if s.examples.len() < 5000 {
            let first = ours
                .iter()
                .zip(&expected)
                .position(|(a, b)| a != b)
                .map_or_else(
                    || format!("length {} vs {}", ours.len(), expected.len()),
                    |i| format!("second {i}: rust {} reference {}", ours[i], expected[i]),
                );
            s.examples.push(format!("{} {first}", note()));
        }
    }
}

/// Compares the damage graphs of a player and the totals that include its minions.
fn compare_graphs(
    ours: &ei_core::pipeline::PlayerDamage,
    targets: &[ei_core::encounter::Target],
    theirs: &Value,
    reference: &Value,
    note: &dyn Fn() -> String,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    let phase0 = |key: &str| theirs.get(key).and_then(|v| v.get(0));
    check_graph(
        stats,
        "graph damage",
        &ours.graphs.damage,
        phase0("damage1S"),
        note,
    );
    check_graph(
        stats,
        "graph power",
        &ours.graphs.power,
        phase0("powerDamage1S"),
        note,
    );
    check_graph(
        stats,
        "graph condition",
        &ours.graphs.condition,
        phase0("conditionDamage1S"),
        note,
    );
    check_graph(
        stats,
        "graph taken",
        &ours.graphs.taken,
        phase0("damageTaken1S"),
        note,
    );
    // the totals that include the minions are the end of the graphs
    if let Some(all) = theirs
        .get("dpsAll")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        let last = |g: &[i64]| g.last().copied().unwrap_or(0);
        check(
            stats,
            "minion damage",
            last(&ours.graphs.damage),
            all,
            "damage",
            note,
        );
        check(
            stats,
            "minion damage",
            last(&ours.graphs.power),
            all,
            "powerDamage",
            note,
        );
        check(
            stats,
            "minion damage",
            last(&ours.graphs.condition),
            all,
            "condiDamage",
            note,
        );
    }
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
    for (index, graph) in ours.graphs.targets.iter().enumerate() {
        let reference = theirs
            .get("targetDamage1S")
            .and_then(|v| v.get(index))
            .and_then(|v| v.get(0));
        let note = || format!("{} target {index}", note());
        check_graph(stats, "graph target", graph, reference, &note);
    }
}

/// Compares the mechanics every encounter has: for each of them the sorted times and players.
/// Counts a mechanic of enemies whose times agree with the reference but whose names do not.
fn note_enemy_names(
    stats: &mut BTreeMap<&'static str, DamageStats>,
    label: &str,
    name: &str,
    ours: &[(i64, String)],
    expected: &[(i64, String)],
) {
    let times = |v: &[(i64, String)]| v.iter().map(|x| x.0).collect::<Vec<_>>();
    if ours == expected || times(ours) != times(expected) {
        return;
    }
    let n = stats.entry("mechanics (enemy names)").or_default();
    n.compared += 1;
    n.mismatches += 1;
    if n.examples.len() < 20 {
        let names = |v: &[(i64, String)]| {
            v.iter()
                .map(|x| x.1.clone())
                .collect::<std::collections::BTreeSet<_>>()
        };
        n.examples.push(format!(
            "{label} {name}: rust {:?} reference {:?}",
            names(ours),
            names(expected)
        ));
    }
}

fn compare_mechanics(
    analysis: &ei_core::pipeline::DamageAnalysis,
    reference: &Value,
    label: &str,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    const NAMES: [&str; 14] = [
        "Dead",
        "Downed",
        "Got up",
        "DC",
        "Resp",
        "Res",
        "Knck.Dwn",
        "Knck.Pll",
        "Flt",
        "Lnch",
        "Lckt",
        "Wtr.Flt.Snk",
        "Miss.Qck",
        "Miss.Alac",
    ];
    let theirs_all = reference
        .get("mechanics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let encounter_names: std::collections::BTreeSet<&str> = analysis
        .mechanics
        .iter()
        .map(|m| m.name)
        .filter(|n| !NAMES.contains(n))
        .collect();
    for (name, key) in NAMES.into_iter().map(|n| (n, "mechanics (generic)")).chain(
        encounter_names
            .into_iter()
            .map(|n| (n, "mechanics (encounter)")),
    ) {
        let mut expected: Vec<(i64, String)> = theirs_all
            .iter()
            .filter(|m| m.get("name").and_then(Value::as_str) == Some(name))
            .flat_map(|m| {
                m.get("mechanicsData")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|x| {
                Some((
                    x.get("time")?.as_i64()?,
                    x.get("actor")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        let mut ours: Vec<(i64, String)> = analysis
            .mechanics
            .iter()
            .filter(|m| m.name == name)
            .flat_map(|m| {
                m.events
                    .iter()
                    .map(|&(time, player)| {
                        (time, analysis.mechanic_players[player].character.clone())
                    })
                    .chain(m.enemies.iter().cloned())
            })
            .collect();
        expected.sort();
        ours.sort();
        // enemy mechanics: C# renames enemies in single logics (not ported), so the times decide
        // and the names are counted on their own
        let enemy = analysis
            .mechanics
            .iter()
            .any(|m| m.name == name && !m.enemies.is_empty());
        let (key, same) = if enemy {
            note_enemy_names(stats, label, name, &ours, &expected);
            let times = |v: &[(i64, String)]| v.iter().map(|x| x.0).collect::<Vec<_>>();
            ("mechanics (enemy)", times(&ours) == times(&expected))
        } else {
            (key, ours == expected)
        };
        let s = stats.entry(key).or_default();
        s.compared += 1;
        if !same {
            s.mismatches += 1;
            if s.examples.len() < 50 {
                s.examples.push(format!(
                    "{label} {name}: rust {ours:?} reference {expected:?}"
                ));
            }
        }
    }
}
