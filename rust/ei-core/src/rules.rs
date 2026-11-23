//! Which fields of a combat record hold an agent address or a timestamp, depending on the
//! state change. Ported from `CombatItem.SrcIsAgent`, `DstIsAgent` and `HasTime`
//! (`GW2EIEvtcParser/CombatItem.cs`), without the extension handler overrides.

use crate::generated::state_change as sc;

/// `true` if the state change is a position, teleport, rotation or velocity event.
#[must_use]
pub fn is_geographical(state_change: u8) -> bool {
    matches!(
        state_change,
        sc::POSITION | sc::TELEPORT | sc::ROTATION | sc::VELOCITY
    )
}

/// `true` if `src_agent` holds an agent address for this state change.
#[must_use]
pub fn src_is_agent(state_change: u8) -> bool {
    is_geographical(state_change)
        || matches!(
            state_change,
            sc::COMBAT
                | sc::ENTER_COMBAT
                | sc::EXIT_COMBAT
                | sc::CHANGE_UP
                | sc::CHANGE_DEAD
                | sc::CHANGE_DOWN
                | sc::SPAWN
                | sc::DESPAWN
                | sc::HEALTH_UPDATE
                | sc::WEAPON_SWAP
                | sc::MAX_HEALTH_UPDATE
                | sc::POINT_OF_VIEW
                | sc::BUFF_INITIAL
                | sc::TEAM_CHANGE
                | sc::ATTACK_TARGET
                | sc::TARGETABLE
                | sc::STACK_ACTIVE
                | sc::STACK_DEACTIVE
                | sc::GUILD
                | sc::BREAKBAR_STATE
                | sc::BREAKBAR_PERCENT
                | sc::MARKER
                | sc::BARRIER_UPDATE
                | sc::LAST90_BEFORE_DOWN
                | sc::EFFECT_45
                | sc::EFFECT_51
                | sc::EFFECT_GROUND_CREATE
                | sc::EFFECT_AGENT_CREATE
                | sc::GLIDER
                | sc::STUN_BREAK
                | sc::MISSILE_CREATE
                | sc::MISSILE_LAUNCH
                | sc::MISSILE_REMOVE
                | sc::ANIMATION_START
                | sc::ANIMATION_STOP
                | sc::BUFF_REMOVE_ALL
                | sc::BUFF_REMOVE_SINGLE
                | sc::BUFF_APPLY
                | sc::TRANSFORMATION
                | sc::STEALTH_CHANGE
                | sc::GADGET_ANIMATION
                | sc::GADGET_NAME_VISIBLE
                | sc::EFFECT_MISSILE_CREATE
                | sc::GADGET_CAPTURE_OUTLINE_SHOW
                | sc::GADGET_CAPTURE_SPLIT_PERCENT
                | sc::GADGET_CAPTURE_OUTLINE_HIDE
                | sc::GADGET_CAPTURE_OUTLINE_POINT
                | sc::JUMP
                | sc::GADGET_MODEL_INFO
                | sc::FLY_TO
                | sc::AGENT_INFO
                | sc::ANIMATION_SPEED
        )
}

/// `true` if `dst_agent` holds an agent address for this state change.
#[must_use]
pub fn dst_is_agent(state_change: u8) -> bool {
    matches!(
        state_change,
        sc::COMBAT
            | sc::ATTACK_TARGET
            | sc::BUFF_INITIAL
            | sc::EFFECT_45
            | sc::LOG_NPC_UPDATE
            | sc::EFFECT_51
            | sc::EFFECT_AGENT_CREATE
            | sc::MISSILE_LAUNCH
            | sc::ANIMATION_START
            | sc::BUFF_REMOVE_ALL
            | sc::BUFF_REMOVE_SINGLE
            | sc::BUFF_APPLY
            | sc::BUFF_CHANGE
    )
}

/// `true` if the `time` field of the record is a timestamp. For all other state changes the
/// field carries other data and the record must not take part in time based queries.
#[must_use]
pub fn has_time(state_change: u8) -> bool {
    src_is_agent(state_change)
        || dst_is_agent(state_change)
        || matches!(
            state_change,
            sc::REWARD
                | sc::TICK_RATE
                | sc::SQUAD_MARKER
                | sc::SQUAD_COMBAT_START
                | sc::SQUAD_COMBAT_END
                | sc::EFFECT_AGENT_REMOVE
                | sc::EFFECT_GROUND_REMOVE
                | sc::MAP_ID
                | sc::MAP_CHANGE
                | sc::WV_W_OBJECTIVE_STATUS
                | sc::TICK
                | sc::PV_P_SCORE_UPDATE
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_events_carry_agents_and_time() {
        assert!(src_is_agent(sc::COMBAT) && dst_is_agent(sc::COMBAT) && has_time(sc::COMBAT));
    }

    #[test]
    fn metadata_events_carry_payload_instead_of_agents() {
        for s in [sc::LANGUAGE, sc::GW_BUILD, sc::SHARD_ID, sc::FRACTAL_SCALE] {
            assert!(!src_is_agent(s) && !dst_is_agent(s) && !has_time(s));
        }
        assert!(!src_is_agent(sc::MAP_ID) && has_time(sc::MAP_ID));
    }

    #[test]
    fn geographical_events_are_position_like() {
        assert!(is_geographical(sc::POSITION) && src_is_agent(sc::VELOCITY));
        assert!(!is_geographical(sc::COMBAT));
    }

    #[test]
    fn unknown_has_nothing() {
        assert!(!src_is_agent(sc::UNKNOWN) && !dst_is_agent(sc::UNKNOWN) && !has_time(sc::UNKNOWN));
    }
}
