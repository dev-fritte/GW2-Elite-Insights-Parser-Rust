//! Profession and elite specialisation names of agents.
//!
//! Port of `GW2APIController.GetSpec` together with the `SpecList.json` API data.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Returned when a profession or specialisation id is not known.
pub const UNKNOWN_SPEC: &str = "Unknown";

#[derive(Deserialize)]
struct SpecEntry {
    id: i64,
    name: String,
    profession: String,
    elite: bool,
}

/// Specialisations from `SpecList.json`: id to the name `GetSpec` returns for it.
#[derive(Debug, Clone, Default)]
pub struct SpecList {
    names: HashMap<i64, String>,
}

impl SpecList {
    /// Parses the content of `SpecList.json`.
    ///
    /// # Errors
    /// Fails if the text is not a JSON array of specialisation objects.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let entries: Vec<SpecEntry> = serde_json::from_str(text)?;
        let names = entries
            .into_iter()
            .map(|e| (e.id, if e.elite { e.name } else { e.profession }))
            .collect();
        Ok(Self { names })
    }

    /// Reads and parses `SpecList.json`.
    ///
    /// # Errors
    /// I/O errors and the errors of [`SpecList::from_json`].
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::from_json(&std::fs::read_to_string(path)?)?)
    }

    /// `GW2APIController.GetSpec`: `NPC` and `GDG` for non players, base professions and the
    /// first elite specialisations without the API, everything else through the list.
    #[must_use]
    pub fn spec_name(&self, profession: u32, elite: u32) -> &str {
        const BASE: [&str; 9] = [
            "Guardian",
            "Warrior",
            "Engineer",
            "Ranger",
            "Thief",
            "Elementalist",
            "Mesmer",
            "Necromancer",
            "Revenant",
        ];
        const HEART_OF_THORNS: [&str; 9] = [
            "Dragonhunter",
            "Berserker",
            "Scrapper",
            "Druid",
            "Daredevil",
            "Tempest",
            "Chronomancer",
            "Reaper",
            "Herald",
        ];
        let index = profession
            .checked_sub(1)
            .map(|p| p as usize)
            .filter(|&p| p < 9);
        match elite {
            0xFFFF_FFFF => {
                if profession & 0xFFFF_0000 == 0xFFFF_0000 {
                    "GDG"
                } else {
                    "NPC"
                }
            }
            0 => index.map_or(UNKNOWN_SPEC, |i| BASE[i]),
            1 => index.map_or(UNKNOWN_SPEC, |i| HEART_OF_THORNS[i]),
            _ => self
                .names
                .get(&i64::from(elite))
                .map_or(UNKNOWN_SPEC, String::as_str),
        }
    }

    /// Number of specialisations in the list.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// `true` if the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> SpecList {
        SpecList::from_json(
            r#"[{"id":7,"name":"Reaper","profession":"Necromancer","elite":true},
                {"id":19,"name":"Dueling","profession":"Mesmer","elite":false}]"#,
        )
        .unwrap()
    }

    #[test]
    fn non_players_and_old_professions() {
        let l = list();
        assert_eq!(l.spec_name(5, 0xFFFF_FFFF), "NPC");
        assert_eq!(l.spec_name(0xFFFF_0001, 0xFFFF_FFFF), "GDG");
        assert_eq!(l.spec_name(8, 0), "Necromancer");
        assert_eq!(l.spec_name(9, 1), "Herald");
        assert_eq!(l.spec_name(10, 0), UNKNOWN_SPEC);
        assert_eq!(l.spec_name(0, 1), UNKNOWN_SPEC);
    }

    #[test]
    fn list_entries_use_the_elite_name_or_the_profession() {
        let l = list();
        assert_eq!(l.spec_name(8, 7), "Reaper");
        assert_eq!(l.spec_name(7, 19), "Mesmer");
        assert_eq!(l.spec_name(7, 99), UNKNOWN_SPEC);
    }
}
