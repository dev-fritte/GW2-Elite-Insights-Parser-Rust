//! Compares what `evtc-ingest` reads from the corpus logs with the table the C# oracle wrote
//! (`docs/phase1/oracle-hashes.tsv`, see `docs/phase1/ORACLE.md`).
//!
//! ```text
//! evtc-oracle --corpus <root> --manifest <corpus-manifest.tsv> --oracle <oracle-hashes.tsv>
//! ```
//!
//! Exit code 0 if every log matches in every column, 1 otherwise.

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ei_core::keep::{KeepSettings, keep_events};
use evtc_ingest::{Evtc, read_log};
use sha2::{Digest, Sha256};

/// Values the oracle table holds for one log, in the form of its TSV columns.
type Row = Vec<(&'static str, String)>;

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

fn run() -> Result<bool, Box<dyn Error>> {
    let mut corpus = None;
    let mut manifest = None;
    let mut oracle = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--corpus" => corpus = Some(PathBuf::from(value)),
            "--manifest" => manifest = Some(PathBuf::from(value)),
            "--oracle" => oracle = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown argument {arg}").into()),
        }
    }
    let corpus = corpus.ok_or("--corpus is required")?;
    let manifest = read_table(&manifest.ok_or("--manifest is required")?)?;
    let oracle_rows = read_table(&oracle.ok_or("--oracle is required")?)?;

    let expected: HashMap<(String, String), &HashMap<String, String>> = oracle_rows
        .iter()
        .map(|r| ((r["TargetId"].clone(), r["File"].clone()), r))
        .collect();

    let mut mismatches = 0usize;
    let mut checked = 0usize;
    for entry in &manifest {
        let (folder, target, file) = (&entry["Folder"], &entry["TargetId"], &entry["File"]);
        let Some(want) = expected.get(&(target.clone(), file.clone())) else {
            println!("MISSING in oracle table: {folder} {file}");
            mismatches += 1;
            continue;
        };
        let path = corpus.join(folder).join(file);
        let got = match read_log(&path)
            .map_err(|e| e.to_string())
            .and_then(|data| summarize(&data).map_err(|e| e.to_string()))
        {
            Ok(got) => got,
            Err(e) => {
                println!("FAIL {folder} {file}: {e}");
                mismatches += 1;
                continue;
            }
        };
        checked += 1;
        for (column, value) in &got {
            let expected_value = want.get(*column).map_or("<none>", String::as_str);
            if expected_value != value {
                println!("MISMATCH {folder} {file}: {column} rust={value} oracle={expected_value}");
                mismatches += 1;
            }
        }
    }
    println!(
        "{checked}/{} logs compared, {} columns each, {mismatches} mismatches",
        manifest.len(),
        COLUMNS
    );
    Ok(mismatches == 0)
}

/// Number of columns compared per log when the log is accepted.
const COLUMNS: usize = 19;

/// Computes the oracle columns of layer A (header, agents, skills, event area and events).
fn summarize(data: &[u8]) -> Result<Row, evtc_ingest::Error> {
    let evtc = Evtc::parse(data)?;
    let header = evtc.header();

    let mut agents = Sha256::new();
    for a in evtc.agents().iter() {
        agents.update(a.address().to_le_bytes());
        agents.update(a.profession().to_le_bytes());
        agents.update(a.is_elite().to_le_bytes());
        agents.update(a.toughness().to_le_bytes());
        agents.update(a.concentration().to_le_bytes());
        agents.update(a.healing().to_le_bytes());
        agents.update(a.condition().to_le_bytes());
        agents.update(a.hitbox_width().to_le_bytes());
        agents.update(a.hitbox_height().to_le_bytes());
        let name = a.name();
        agents.update(u32::try_from(name.len()).unwrap_or(u32::MAX).to_le_bytes());
        agents.update(name.as_bytes());
    }

    let mut skills = Sha256::new();
    let mut unique = 0usize;
    for s in evtc.skills().unique() {
        unique += 1;
        skills.update(s.id().to_le_bytes());
        let name = s.name();
        skills.update(u32::try_from(name.len()).unwrap_or(u32::MAX).to_le_bytes());
        skills.update(name.as_bytes());
    }

    let mut events = Sha256::new();
    for record in evtc.events() {
        events.update(record.to_canonical());
    }

    let records: Vec<_> = evtc.events().collect();
    let kept = keep_events(
        &records,
        header.trigger_id(),
        header.build(),
        KeepSettings::default(),
    );

    let mut row: Row = vec![
        ("BuildString", header.build_string().to_string()),
        ("Revision", header.revision().to_string()),
        ("TriggerId", header.trigger_id().to_string()),
        ("Agents", evtc.agents().len().to_string()),
        ("AgentsHash", hex(&agents.finalize())),
        ("SkillsRaw", evtc.skills().len().to_string()),
        ("SkillsUnique", unique.to_string()),
        ("SkillsHash", hex(&skills.finalize())),
        ("Events", evtc.event_count().to_string()),
        ("TrailingBytes", evtc.trailing_bytes().to_string()),
        ("EventsHash", hex(&events.finalize())),
    ];
    match kept {
        Ok(kept) => {
            let mut hash = Sha256::new();
            for record in &kept.events {
                hash.update(record.to_canonical());
            }
            row.push(("KeptCount", kept.events.len().to_string()));
            row.push(("KeptHash", hex(&hash.finalize())));
            row.push(("KeptTriggerId", kept.trigger_id.to_string()));
            row.push(("MapId", kept.map_id.to_string()));
            row.push(("LogStartOffset", kept.log_start_offset.to_string()));
            row.push(("LogEndTime", kept.log_end_time.to_string()));
            row.push(("Gw2Build", kept.gw2_build.to_string()));
            row.push(("KeptError", String::new()));
        }
        Err(e) => row.push(("KeptError", e.to_string())),
    }
    Ok(row)
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02X}");
            s
        })
}

/// Reads a tab separated table with a header row. A UTF-8 byte order mark is ignored.
fn read_table(path: &Path) -> Result<Vec<HashMap<String, String>>, Box<dyn Error>> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let mut lines = text.lines().filter(|l| !l.is_empty());
    let header: Vec<&str> = lines.next().ok_or("empty table")?.split('\t').collect();
    Ok(lines
        .map(|line| {
            header
                .iter()
                .zip(line.split('\t'))
                .map(|(k, v)| ((*k).to_string(), v.to_string()))
                .collect()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_uppercase_and_padded() {
        assert_eq!(hex(&[0x00, 0xAB, 0x0F]), "00AB0F");
    }

    #[test]
    fn summarize_reports_every_column() {
        let mut data = b"EVTC20260920".to_vec();
        data.extend_from_slice(&[1, 0x45, 0x3C, 0]);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        let row = summarize(&data).unwrap();
        // no combat events: all layer A columns plus the keep error
        assert_eq!(row.len(), 12);
        assert_eq!(row[11], ("KeptError", "no combat events found".to_string()));
        assert_eq!(row[0], ("BuildString", "EVTC20260920".to_string()));
        assert_eq!(row[2], ("TriggerId", "15429".to_string()));
        // SHA256 of the empty input
        assert_eq!(
            row[4].1,
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        );
    }
}
