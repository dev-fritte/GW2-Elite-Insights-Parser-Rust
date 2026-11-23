//! The parts of the GW2 API data the core needs.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Map ids and their API type (`"Public"`, `"Instance"`, ...), read from `MapList.json`.
#[derive(Debug, Clone, Default)]
pub struct MapList {
    types: HashMap<i64, String>,
}

#[derive(Deserialize)]
struct MapEntry {
    id: i64,
    #[serde(rename = "type")]
    kind: Option<String>,
}

impl MapList {
    /// Parses the content of `MapList.json` (an array of API map objects).
    ///
    /// # Errors
    /// Fails if the text is not a JSON array of objects with an `id`.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let entries: Vec<MapEntry> = serde_json::from_str(text)?;
        let types = entries
            .into_iter()
            .map(|e| (e.id, e.kind.unwrap_or_default()))
            .collect();
        Ok(Self { types })
    }

    /// Reads and parses `MapList.json`.
    ///
    /// # Errors
    /// I/O errors and the errors of [`MapList::from_json`].
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::from_json(&std::fs::read_to_string(path)?)?)
    }

    /// `true` if the API knows the map and its type is `Instance`.
    #[must_use]
    pub fn is_instance(&self, map_id: i64) -> bool {
        self.types.get(&map_id).is_some_and(|t| t == "Instance")
    }

    /// Number of maps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// `true` if the list has no maps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ids_and_types() {
        let list = MapList::from_json(
            r#"[{"name":"A","type":"Public","id":15},{"name":"B","type":"Instance","id":1062},{"id":7}]"#,
        )
        .unwrap();
        assert_eq!(list.len(), 3);
        assert!(list.is_instance(1062));
        assert!(!list.is_instance(15));
        assert!(!list.is_instance(7));
        assert!(!list.is_instance(99));
    }

    #[test]
    fn rejects_other_json() {
        assert!(MapList::from_json("{}").is_err());
    }
}
