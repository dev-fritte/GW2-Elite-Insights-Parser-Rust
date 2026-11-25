//! Buffs the C# parser invents for consumables it does not know (`BuffsContainer` constructor):
//! if the known foods of a log share one buff info category, every other buff of that category is
//! an unknown nourishment; the same for enhancements (utilities, writs, slaying potions).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use ei_data::buffs::{BUFFS, BuffDef, Classification, Source, StackType};

/// Buff info of one buff: its category byte and its maximal stacks.
#[derive(Debug, Clone, Copy)]
pub(super) struct Info {
    pub category: u8,
    pub max_stacks: u16,
}

const FOOD_LISTS: [&str; 2] = ["FoodBuffs.NormalFoods", "FoodBuffs.AscendedFood"];
const ENHANCEMENT_LISTS: [&str; 3] = [
    "UtilityBuffs.Utilities",
    "UtilityBuffs.Writs",
    "UtilityBuffs.SlayingPotions",
];

type Cache = Mutex<HashMap<(i64, u16, Classification), &'static BuffDef>>;

/// Custom buff definitions live for the whole process; the cache keeps their number bounded by
/// the number of different ids of the consumable categories.
fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn custom_buff(id: i64, capacity: u16, classification: Classification) -> &'static BuffDef {
    let mut cache = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .entry((id, capacity, classification))
        .or_insert_with(|| {
            let name: &'static str = Box::leak(format!("Unknown consumable {id}").into_boxed_str());
            Box::leak(Box::new(BuffDef {
                list: "custom",
                name,
                id,
                classification,
                sources: &[Source::Item],
                // `CreateCustomBuff`: stacking if there is more than one stack
                stack_type: if capacity > 1 {
                    StackType::Stacking
                } else {
                    StackType::Force
                },
                capacity: i32::from(capacity),
                min_build: 0,
                max_build: u64::MAX,
                min_evtc_build: i32::MIN,
                max_evtc_build: i32::MAX,
                link: "",
            }))
        })
}

fn category_of(lists: &[&str], infos: &HashMap<u32, Info>) -> Option<u8> {
    let categories: HashSet<u8> = BUFFS
        .iter()
        .filter(|b| lists.contains(&b.list))
        .filter_map(|b| u32::try_from(b.id).ok().and_then(|id| infos.get(&id)))
        .map(|i| i.category)
        .collect();
    if categories.len() == 1 {
        categories.into_iter().next()
    } else {
        None
    }
}

/// The unknown nourishments and enhancements of a log, by buff id.
pub(super) fn unknown_consumables(
    known: &HashSet<i64>,
    infos: &HashMap<u32, Info>,
) -> HashMap<i64, &'static BuffDef> {
    let mut out = HashMap::new();
    for (lists, classification) in [
        (&FOOD_LISTS[..], Classification::Nourishment),
        (&ENHANCEMENT_LISTS[..], Classification::Enhancement),
    ] {
        let Some(category) = category_of(lists, infos) else {
            continue;
        };
        let mut ids: Vec<(&u32, &Info)> = infos
            .iter()
            .filter(|(_, i)| i.category == category)
            .collect();
        ids.sort_by_key(|(id, _)| **id);
        for (&id, info) in ids {
            let id = i64::from(id);
            if !known.contains(&id) {
                out.entry(id)
                    .or_insert_with(|| custom_buff(id, info.max_stacks, classification));
            }
        }
    }
    out
}
