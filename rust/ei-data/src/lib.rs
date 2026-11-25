//! Constants and tables extracted from the C# sources of the GW2 Elite Insights parser (MIT
//! License, Copyright (c) 2018 baaron4), see the `LICENSE` file in the repository root.
//!
//! Nothing in `generated` is typed by hand: `tools/gen/Gen-Ids.ps1` writes it from the C# files,
//! so the values follow the upstream source.

#[allow(missing_docs, clippy::unreadable_literal, clippy::excessive_precision)]
#[rustfmt::skip]
pub mod generated;

pub use generated::{
    buffs, effect_guids, marker_guids, mechanics, skill_ids, species_ids, targets,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_ids_keep_their_values() {
        assert_eq!(skill_ids::UNKNOWN, -1);
        assert_eq!(skill_ids::WEAPON_SWAP, -2);
        assert_eq!(skill_ids::NO_BUFF, i64::MIN);
    }

    #[test]
    fn species_ids_resolve_references_and_implicit_values() {
        assert_eq!(species_ids::target_id::GLENNA, 15014);
        assert_eq!(species_ids::target_id::VALE_GUARDIAN, 15438);
        // `EtherealBarrier = SpeciesIDs.EtherealBarrier` refers to a constant of the class
        assert_eq!(
            species_ids::target_id::ETHEREAL_BARRIER,
            species_ids::species::ETHEREAL_BARRIER
        );
        assert_eq!(species_ids::target_id::UNKNOWN, i32::MAX);
    }

    #[test]
    fn guids_are_sixteen_bytes_in_reading_order() {
        assert_eq!(effect_guids::GENERIC240_UNIT_RADIUS[0], 0xE7);
        assert_eq!(effect_guids::GENERIC240_UNIT_RADIUS[15], 0xA4);
        assert_eq!(marker_guids::RED_COMMANDER_TAG[0], 0x42);
    }

    #[test]
    fn buffs_are_complete_and_resolved() {
        use crate::buffs::{BUFFS, Classification, StackType};
        assert_eq!(BUFFS.len(), 2372);
        let might = BUFFS.iter().find(|b| b.name == "Might").unwrap();
        assert_eq!(might.id, skill_ids::MIGHT);
        assert_eq!(might.stack_type, StackType::Stacking);
        assert_eq!(might.capacity, 25);
        assert_eq!(might.classification, Classification::Boon);
        assert!(BUFFS.iter().all(|b| !b.sources.is_empty()));
    }
}
