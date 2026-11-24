//! Compares the JSON of the Rust core with the C# reference JSON, field by field, for every log
//! of the corpus manifest.
//!
//! ```text
//! ei-parity --corpus <root> --manifest <corpus-manifest.tsv> --reference <dir> --content <dir>
//! ```
//!
//! `<dir>` holds `<TargetId>_<File>.json` as written by `GW2EIParserReference`. Exit code 0 if
//! every compared field of every log matches.

mod buffs;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ei_core::api::Api;
use ei_core::pipeline::{analyze, to_json};
use evtc_ingest::read_log;
use serde_json::Value;

/// Fields of the Rust JSON that exist under the same name in the C# JSON.
const SHARED_FIELDS: &[&str] = &[
    "arcVersion",
    "arcRevision",
    "triggerID",
    "isInstanceLog",
    "mapID",
    "gW2Build",
    "language",
    "languageID",
    "fractalScale",
    "region",
];

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

struct Options {
    corpus: PathBuf,
    manifest: PathBuf,
    reference: PathBuf,
    content: Option<PathBuf>,
    verbose: bool,
    buffs: bool,
}

fn options() -> Result<Options, Box<dyn Error>> {
    let (mut corpus, mut manifest, mut reference, mut content) = (None, None, None, None);
    let mut verbose = false;
    let mut with_buffs = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--buffs" {
            with_buffs = true;
            continue;
        }
        if arg == "--verbose" {
            verbose = true;
            continue;
        }
        let value = PathBuf::from(
            args.next()
                .ok_or_else(|| format!("missing value for {arg}"))?,
        );
        match arg.as_str() {
            "--corpus" => corpus = Some(value),
            "--manifest" => manifest = Some(value),
            "--reference" => reference = Some(value),
            "--content" => content = Some(value),
            _ => return Err(format!("unknown argument {arg}").into()),
        }
    }
    Ok(Options {
        corpus: corpus.ok_or("--corpus is required")?,
        manifest: manifest.ok_or("--manifest is required")?,
        reference: reference.ok_or("--reference is required")?,
        content,
        verbose,
        buffs: with_buffs,
    })
}

/// Counts and a few examples of the logs a field differs in.
#[derive(Default)]
struct FieldStats {
    compared: usize,
    mismatches: usize,
    examples: Vec<String>,
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<bool, Box<dyn Error>> {
    let opts = options()?;
    let api = match opts.content.as_deref() {
        Some(dir) => Api::load(dir)?,
        None => Api::default(),
    };
    let manifest = read_manifest(&opts.manifest)?;

    let mut stats: BTreeMap<String, FieldStats> = BTreeMap::new();
    let mut failed_logs = 0usize;
    let mut compared_logs = 0usize;
    let mut buff_stats: BTreeMap<i64, buffs::BuffStats> = BTreeMap::new();
    let mut buff_failed = 0usize;

    for entry in &manifest {
        let log_path = opts.corpus.join(&entry.folder).join(&entry.file);
        let ref_path = opts
            .reference
            .join(format!("{}_{}.json", entry.target, entry.file));
        let label = format!("{} {}", entry.folder, entry.file);

        let data = read_log(&log_path).map_err(|e| e.to_string());
        let ours = match data
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|d| analyze(d, &api).map_err(|e| e.to_string()))
        {
            Ok(a) => to_json(&a),
            Err(e) => {
                println!("FAIL {label}: {e}");
                failed_logs += 1;
                continue;
            }
        };
        let reference: Value = serde_json::from_slice(&std::fs::read(&ref_path)?)
            .map_err(|e| format!("{}: {e}", ref_path.display()))?;
        compared_logs += 1;
        if opts.buffs
            && let Ok(d) = &data
            && let Err(e) = buffs::compare_log(d, &api, &reference, &label, &mut buff_stats)
        {
            println!("BUFF FAIL {label}: {e}");
            buff_failed += 1;
        }

        for field in SHARED_FIELDS {
            record(
                &mut stats,
                field,
                &label,
                ours.get(*field),
                reference.get(*field),
            );
        }
        compare_players(&mut stats, &label, &ours, &reference);
        for (ours_key, reference_key) in [
            ("coreLogStartUnix", "timeStartStd"),
            ("coreLogEndUnix", "timeEndStd"),
        ] {
            let expected = reference
                .get(reference_key)
                .and_then(Value::as_str)
                .and_then(unix_from_std_time);
            let expected = expected.map(Value::from);
            record(
                &mut stats,
                ours_key,
                &label,
                ours.get(ours_key),
                expected.as_ref(),
            );
        }
    }

    let mut all_match = failed_logs == 0;
    println!(
        "{compared_logs}/{} logs compared, {failed_logs} failed to analyse",
        manifest.len()
    );
    for (field, s) in &stats {
        println!(
            "{field:<18} {:>4} compared, {:>4} mismatches",
            s.compared, s.mismatches
        );
        if s.mismatches > 0 {
            all_match = false;
            for example in s.examples.iter().take(if opts.verbose { 50 } else { 3 }) {
                println!("    {example}");
            }
        }
    }
    if opts.buffs {
        let compared: usize = buff_stats.values().map(|s| s.compared).sum();
        let bad: usize = buff_stats.values().map(|s| s.mismatches).sum();
        println!("buff graphs: {compared} compared, {bad} mismatches, {buff_failed} logs failed");
        let mut worst: Vec<_> = buff_stats
            .iter()
            .filter(|(_, s)| s.mismatches > 0)
            .collect();
        worst.sort_by_key(|(_, s)| std::cmp::Reverse(s.mismatches));
        for (id, s) in worst.iter().take(if opts.verbose { 40 } else { 12 }) {
            println!("  buff {id}: {}/{} differ", s.mismatches, s.compared);
            for e in s.examples.iter().take(1) {
                println!("      {e}");
            }
        }
        if bad > 0 || buff_failed > 0 {
            all_match = false;
        }
    }
    Ok(all_match)
}

/// The fields of a player the core computes, as a comparable key.
fn player_key(player: &Value) -> String {
    let text = |key: &str| player.get(key).map_or(String::new(), ToString::to_string);
    format!(
        "{}|{}|{}|{}",
        text("name"),
        text("account"),
        text("group"),
        text("profession")
    )
}

/// Compares the player lists as sets of (name, account, group, profession): the C# list is sorted
/// with a culture sensitive comparison that is not reproduced here.
fn compare_players(
    stats: &mut BTreeMap<String, FieldStats>,
    label: &str,
    ours: &Value,
    reference: &Value,
) {
    let keys = |v: &Value| {
        let mut keys: Vec<String> = v
            .get("players")
            .and_then(Value::as_array)
            .map(|players| {
                players
                    .iter()
                    // friendly NPCs are added by the encounter logic, which is not ported
                    .filter(|p| p.get("friendlyNPC").and_then(Value::as_bool) != Some(true))
                    .map(player_key)
                    .collect()
            })
            .unwrap_or_default();
        keys.sort();
        keys
    };
    let (a, b) = (keys(ours), keys(reference));
    let s = stats.entry("players".to_string()).or_default();
    s.compared += 1;
    if a != b {
        s.mismatches += 1;
        let missing = b.iter().filter(|k| !a.contains(k)).count();
        let extra = a.iter().filter(|k| !b.contains(k)).count();
        s.examples.push(format!(
            "{label}: rust has {} players, reference {}; {missing} reference players missing, {extra} extra",
            a.len(),
            b.len()
        ));
    }
}

fn record(
    stats: &mut BTreeMap<String, FieldStats>,
    field: &str,
    label: &str,
    ours: Option<&Value>,
    reference: Option<&Value>,
) {
    let s = stats.entry(field.to_string()).or_default();
    s.compared += 1;
    // an absent value and `null` mean the same thing in the C# JSON
    let norm = |v: Option<&Value>| v.filter(|v| !v.is_null()).cloned();
    let (a, b) = (norm(ours), norm(reference));
    if a != b {
        s.mismatches += 1;
        s.examples.push(format!(
            "{label}: rust={} reference={}",
            a.map_or("<absent>".into(), |v| v.to_string()),
            b.map_or("<absent>".into(), |v| v.to_string())
        ));
    }
}

/// Parses `yyyy-MM-dd HH:mm:ss +HH:MM` (the `Std` time format of the C# JSON) to unix seconds.
/// `MISSING` and anything else that does not parse gives `None`.
fn unix_from_std_time(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once(' ')?;
    let (time, zone) = rest.split_once(' ')?;
    let mut d = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (t.next()?.ok()?, t.next()?.ok()?, t.next()?.ok()?);
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (zh, zm) = zone[1..].split_once(':')?;
    let offset = sign * (zh.parse::<i64>().ok()? * 3600 + zm.parse::<i64>().ok()? * 60);
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

struct Entry {
    folder: String,
    target: String,
    file: String,
}

fn read_manifest(path: &Path) -> Result<Vec<Entry>, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let mut lines = text.lines().filter(|l| !l.is_empty());
    let header: Vec<&str> = lines.next().ok_or("empty manifest")?.split('\t').collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or(format!("column {name} missing"))
    };
    let (folder, target, file) = (col("Folder")?, col("TargetId")?, col("File")?);
    Ok(lines
        .map(|line| {
            let c: Vec<&str> = line.split('\t').collect();
            Entry {
                folder: c[folder].to_string(),
                target: c[target].to_string(),
                file: c[file].to_string(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_std_time_format() {
        assert_eq!(unix_from_std_time("1970-01-01 00:00:00 +00:00"), Some(0));
        assert_eq!(
            unix_from_std_time("2026-09-28 20:23:46 +02:00"),
            Some(1_790_619_826)
        );
        assert_eq!(
            unix_from_std_time("2026-09-28 18:23:46 +00:00"),
            Some(1_790_619_826)
        );
        assert_eq!(
            unix_from_std_time("2026-09-28 13:23:46 -05:00"),
            Some(1_790_619_826)
        );
        assert_eq!(unix_from_std_time("MISSING"), None);
    }

    #[test]
    fn absent_and_null_are_equal() {
        let mut stats = BTreeMap::new();
        record(&mut stats, "region", "x", None, Some(&Value::Null));
        record(&mut stats, "region", "x", Some(&Value::from("EU")), None);
        assert_eq!(stats["region"].compared, 2);
        assert_eq!(stats["region"].mismatches, 1);
    }
}
