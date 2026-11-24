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
    let players = analysis.players;
    let theirs_all = reference
        .get("players")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for ours in &players {
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
                if s.examples.len() < 80 {
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
                if s.examples.len() < 80 {
                    s.examples.push(format!(
                        "{label} {}: rust {value} reference {expected} (diff {})",
                        ours.player.character,
                        value - expected
                    ));
                }
            }
        }
        compare_main_target(ours, theirs, reference, duration, label, stats);
    }
    Ok(())
}

/// Compares the damage to the main target with `dpsTargets[target][0]`. Skipped if the reference
/// has no single target of the species of the trigger id.
fn compare_main_target(
    ours: &ei_core::pipeline::PlayerDamage,
    theirs: &Value,
    reference: &Value,
    duration: i64,
    label: &str,
    stats: &mut BTreeMap<&'static str, DamageStats>,
) {
    use ei_core::damage::DamageTotals;
    let Some(t) = ours.to_main_target else {
        return;
    };
    let trigger = reference.get("triggerID").and_then(Value::as_i64);
    let targets = reference
        .get("targets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut own = targets
        .iter()
        .enumerate()
        .filter(|(_, x)| x.get("id").and_then(Value::as_i64) == trigger);
    let (Some((index, _)), None) = (own.next(), own.next()) else {
        return;
    };
    let Some(dps) = theirs
        .get("dpsTargets")
        .and_then(|d| d.get(index))
        .and_then(Value::as_array)
        .and_then(|p| p.first())
    else {
        return;
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
            if s.examples.len() < 80 {
                s.examples.push(format!(
                    "{label} {}: rust {value} reference {expected} (diff {})",
                    ours.player.character,
                    value - expected
                ));
            }
        }
    }
}
