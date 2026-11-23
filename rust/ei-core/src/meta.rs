//! Log metadata taken from the kept records: build, map, language, region, fractal scale and the
//! squad combat start and end time.
//!
//! Port of the metadata handling in `CombatEventFactory` (the last record of a kind wins, except
//! for the squad start) and of `LogMetadata`/`JsonLogBuilder`. Checked against the fields of the
//! C# reference JSON by `ei-parity`.

use crate::api::MapList;
use crate::generated::state_change as sc;
use crate::keep::{Kept, TRIGGER_INSTANCE};

/// `ArcDPSEnums.LanguageEnum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// `English`
    English = 0,
    /// `Missing`
    Missing = 1,
    /// `French`
    French = 2,
    /// `German`
    German = 3,
    /// `Spanish`
    Spanish = 4,
    /// `Chinese`
    Chinese = 5,
    /// `Unknown`, also used when the log has no language record
    Unknown = 6,
}

impl Language {
    /// `ArcDPSEnums.GetLanguage`: values from 6 on are `Unknown`.
    #[must_use]
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::English,
            1 => Self::Missing,
            2 => Self::French,
            3 => Self::German,
            4 => Self::Spanish,
            5 => Self::Chinese,
            _ => Self::Unknown,
        }
    }

    /// `ArcDPSEnums.LanguageToString`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Missing => "Missing",
            Self::French => "French",
            Self::German => "German",
            Self::Spanish => "Spanish",
            Self::Chinese => "Chinese",
            Self::Unknown => "Unknown",
        }
    }
}

/// `ArcDPSEnums.RegionEnum` without the `Unknown` case, which is represented by `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// North America
    Na,
    /// Europe
    Eu,
    /// China
    Cn,
}

impl Region {
    /// `ArcDPSEnums.RegionToString`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Na => "NA",
            Self::Eu => "EU",
            Self::Cn => "China",
        }
    }

    /// `ShardEvent.GetRegion`.
    #[must_use]
    pub fn from_shard(shard: u64) -> Option<Self> {
        if (1001..2000).contains(&shard) {
            Some(Self::Na)
        } else if (2001..3000).contains(&shard) {
            Some(Self::Eu)
        } else if (7001..8000).contains(&shard) {
            Some(Self::Cn)
        } else {
            None
        }
    }
}

/// The metadata the first Rust core exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// `EVTC<build>`, with the build updated by the arc build record (JSON `arcVersion`).
    pub arc_version: String,
    /// Revision from the arc build record, `-1` if there is none (JSON `arcRevision`).
    pub arc_revision: i32,
    /// Trigger id after the instance correction (JSON `triggerID`).
    pub trigger_id: u16,
    /// `true` for instance logs (JSON `isInstanceLog`).
    pub is_instance_log: bool,
    /// Map id of the last map record (JSON `mapID`).
    pub map_id: Option<i32>,
    /// GW2 build (JSON `gW2Build`).
    pub gw2_build: u64,
    /// Language (JSON `language`, `languageID`).
    pub language: Language,
    /// Region (JSON `region`, omitted when unknown).
    pub region: Option<Region>,
    /// Fractal scale, 0 if there is none (JSON `fractalScale`).
    pub fractal_scale: u8,
    /// Server unix time of the squad combat start, if there is one.
    pub log_start_unix: Option<u32>,
    /// Server unix time of the squad combat end, if there is one.
    pub log_end_unix: Option<u32>,
}

/// Reads the metadata from the kept records.
///
/// `maps` is only needed for the region of logs whose shard record carries no world id.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // mirrors the C# casts of the record fields
pub fn extract(kept: &Kept, maps: Option<&MapList>) -> Metadata {
    let mut map_id: Option<i32> = None;
    let mut language = Language::Unknown;
    let mut region: Option<Region> = None;
    let mut fractal_scale = 0u8;
    let mut log_start: Option<u32> = None;
    let mut log_end: Option<u32> = None;

    for e in &kept.events {
        match e.state_change {
            sc::MAP_ID => map_id = Some(e.src_agent as i32),
            sc::LANGUAGE => language = Language::from_byte(e.src_agent as u8),
            sc::FRACTAL_SCALE if e.src_agent != 0 => fractal_scale = e.src_agent as u8,
            sc::SHARD_ID => {
                // `ShardEvent`: the world id decides, else the shard of an instance map
                let world = e.value.cast_unsigned();
                region = if world > 0 {
                    Region::from_shard(u64::from(world))
                } else if map_id.is_some_and(|m| maps.is_some_and(|l| l.is_instance(i64::from(m))))
                {
                    Region::from_shard(e.src_agent)
                } else {
                    None
                };
            }
            sc::SQUAD_COMBAT_START if e.value != 0 && e.buff_dmg != 0 => {
                let ts = e.value.cast_unsigned();
                let end_not_after = log_end.is_some_and(|end| end <= ts);
                if log_start.is_none() && !end_not_after {
                    log_start = Some(ts);
                }
            }
            sc::SQUAD_COMBAT_END if e.value != 0 && e.buff_dmg != 0 => {
                log_end = Some(e.value.cast_unsigned());
            }
            _ => {}
        }
    }

    Metadata {
        arc_version: format!("EVTC{}", kept.version.build),
        arc_revision: kept.version.revision,
        trigger_id: kept.trigger_id,
        is_instance_log: kept.trigger_id == TRIGGER_INSTANCE,
        map_id,
        gw2_build: kept.gw2_build,
        language,
        region,
        fractal_scale,
        log_start_unix: log_start,
        log_end_unix: log_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keep::{ArcVersion, KeepSettings, keep_events};
    use evtc_ingest::CombatRecord;

    fn rec(time: i64, state_change: u8, src: u64) -> CombatRecord {
        CombatRecord {
            time,
            src_agent: src,
            dst_agent: 0,
            value: 0,
            buff_dmg: 0,
            overstack_value: 0,
            skill_id: 0,
            src_instid: 1,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            is_buff: 0,
            result: 0,
            is_activation: 0,
            is_buff_remove: 0,
            is_ninety: 0,
            is_fifty: 0,
            is_moving: 0,
            state_change,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    fn kept(records: &[CombatRecord]) -> Kept {
        let settings = KeepSettings {
            too_short_limit: 0,
            parse_extensions: true,
        };
        keep_events(records, 15429, 20_260_920, settings).unwrap()
    }

    #[test]
    fn last_record_of_a_kind_wins() {
        let k = kept(&[
            rec(1, sc::LANGUAGE, 2),
            rec(2, sc::LANGUAGE, 3),
            rec(3, sc::GW_BUILD, 100),
            rec(4, sc::GW_BUILD, 0), // zero builds are ignored
            rec(5, sc::FRACTAL_SCALE, 0),
            rec(6, sc::FRACTAL_SCALE, 99),
            rec(1000, sc::COMBAT, 1),
        ]);
        let m = extract(&k, None);
        assert_eq!(m.language, Language::German);
        assert_eq!(m.gw2_build, 100);
        assert_eq!(m.fractal_scale, 99);
        assert_eq!(m.arc_version, "EVTC20260920");
        assert_eq!(m.arc_revision, -1);
        assert_eq!(m.map_id, None);
        assert!(!m.is_instance_log);
    }

    #[test]
    fn missing_records_fall_back_to_defaults() {
        let m = extract(&kept(&[rec(1000, sc::COMBAT, 1)]), None);
        assert_eq!(m.language, Language::Unknown);
        assert_eq!(
            (m.region, m.fractal_scale, m.log_start_unix),
            (None, 0, None)
        );
    }

    #[test]
    fn region_comes_from_the_world_id_or_the_shard_of_an_instance_map() {
        let maps =
            MapList::from_json(r#"[{"id":1062,"type":"Instance"},{"id":15,"type":"Public"}]"#)
                .unwrap();
        let mut world = rec(2, sc::SHARD_ID, 0);
        world.value = 2105;
        let m = extract(&kept(&[world, rec(1000, sc::COMBAT, 1)]), Some(&maps));
        assert_eq!(m.region, Some(Region::Eu));

        let mut shard = rec(3, sc::SHARD_ID, 1500);
        shard.value = 0;
        let instance = kept(&[rec(1, sc::MAP_ID, 1062), shard, rec(1000, sc::COMBAT, 1)]);
        assert_eq!(extract(&instance, Some(&maps)).region, Some(Region::Na));
        let public = kept(&[rec(1, sc::MAP_ID, 15), shard, rec(1000, sc::COMBAT, 1)]);
        assert_eq!(extract(&public, Some(&maps)).region, None);
        assert_eq!(extract(&instance, None).region, None);
    }

    #[test]
    fn squad_start_keeps_the_first_valid_record_and_the_end_closes_the_log() {
        let stamp = |state_change, ts: i32| {
            let mut r = rec(1, state_change, 0);
            r.value = ts;
            r.buff_dmg = 1;
            r
        };
        let mut invalid = stamp(sc::SQUAD_COMBAT_START, 50);
        invalid.buff_dmg = 0;
        let k = kept(&[
            invalid,
            stamp(sc::SQUAD_COMBAT_START, 100),
            stamp(sc::SQUAD_COMBAT_START, 200),
            stamp(sc::SQUAD_COMBAT_END, 300),
            stamp(sc::SQUAD_COMBAT_END, 400),
            rec(1000, sc::COMBAT, 1),
        ]);
        let m = extract(&k, None);
        // after the first end record only extension records are kept, so the end at 400 never counts
        assert_eq!((m.log_start_unix, m.log_end_unix), (Some(100), Some(300)));
    }

    #[test]
    fn instance_logs_are_detected_by_trigger_id() {
        let mut k = kept(&[rec(1000, sc::COMBAT, 1)]);
        k.trigger_id = 2;
        k.version = ArcVersion {
            build: 7,
            revision: 3,
        };
        let m = extract(&k, None);
        assert!(m.is_instance_log);
        assert_eq!((m.arc_version.as_str(), m.arc_revision), ("EVTC7", 3));
    }

    #[test]
    fn region_and_language_names() {
        assert_eq!(Region::from_shard(1000), None);
        assert_eq!(Region::from_shard(2000), None);
        assert_eq!(Region::from_shard(7500).map(Region::name), Some("China"));
        assert_eq!(Language::from_byte(200), Language::Unknown);
        assert_eq!(Language::Missing.name(), "Missing");
    }
}
