//! Headless document model. No UI types allowed here.
//!
//! The model is plain data. It changes only through [`Change`] records applied
//! with [`Project::apply`], so every edit is invertible and replayable. Time is
//! integer ticks ([`TICKS_PER_QUARTER`]); there are no floats in the time domain.

mod automation;
mod change;
mod effects;
mod hash;
mod hash_cache;
mod id;
mod project;
mod rules;
mod time;
mod validate;
#[cfg(test)]
mod validate_tests;

pub use automation::{
    AutomationCurve, AutomationLane, AutomationPoint, AutomationTarget, MAX_AUTOMATION_POINTS,
};
pub use change::{Change, ChangeError, FieldKey, FieldValue};
pub use composer_theory::{Key, Scale};
pub use effects::{
    Effect, EffectChain, EffectKind, EffectOwner, ParamSpec, MAX_CHAIN_EFFECTS, MAX_EFFECT_PARAMS,
};
pub use hash_cache::HashCache;
pub use id::{Id, IdGen, ParseIdError};
pub use project::{
    ChannelMix, MasterMix, Note, Project, Region, RegionContent, RegionKind, TempoMap, Track,
    TrackKind, DEFAULT_MBPM, MAX_CLIP_OFFSET, MAX_MBPM, MAX_NAME_LEN, MAX_PAN, MAX_REVERB,
    MAX_VOLUME, MIN_MBPM, UNITY_VOLUME,
};
pub use rules::{Rule, MAX_MARKERS, MAX_SOUND_ID_LEN};
pub use time::{Tick, TimeSignature, MAX_TICK, TICKS_PER_QUARTER};
pub use validate::{Locator, ValidationError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_json_is_hex_string_and_round_trips() {
        let id = Id(0xabc);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"00000000000000000000000000000abc\"");
        assert_eq!(serde_json::from_str::<Id>(&json).unwrap(), id);
        assert!("xyz".parse::<Id>().is_err());
    }

    #[test]
    fn project_json_round_trip_keeps_hash() {
        let mut p = Project::new(Id(1), "demo");
        let track = Track {
            id: Id(2),
            name: "lead".into(),
            kind: TrackKind::Instrument,
            instrument: Some(Id(9)),
            mix: Default::default(),
            sound: None,
            reverb_send: 0,
            regions: Default::default(),
        };
        p.apply(&Change::NewTrack { index: 0, track }).unwrap();
        let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.state_hash(), p.state_hash());
    }

    #[test]
    fn bar_length_is_exact() {
        assert_eq!(TimeSignature::COMMON.ticks_per_bar(), 3840);
        let six_eight = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        assert_eq!(six_eight.ticks_per_bar(), 2880);
    }

    #[test]
    fn stale_set_is_rejected_and_inverse_restores() {
        let mut p = Project::new(Id(1), "a");
        let c = Change::Set {
            key: FieldKey::ProjectName,
            old: FieldValue::Text("a".into()),
            new: FieldValue::Text("b".into()),
        };
        let h = p.state_hash();
        p.apply(&c).unwrap();
        assert_eq!(p.apply(&c), Err(ChangeError::Stale("field")));
        p.apply(&c.inverse()).unwrap();
        assert_eq!(p.state_hash(), h);
    }

    #[test]
    fn default_mix_is_invisible_and_non_default_mix_round_trips() {
        let track = Track {
            id: Id(2),
            name: "t".into(),
            kind: TrackKind::Instrument,
            instrument: None,
            mix: Default::default(),
            sound: None,
            reverb_send: 0,
            regions: Default::default(),
        };
        let mut p = Project::new(Id(1), "m");
        p.apply(&Change::NewTrack { index: 0, track }).unwrap();
        // A default mix adds nothing to the JSON or the hash of older projects.
        assert!(!serde_json::to_string(&p).unwrap().contains("mix"));
        let plain = p.state_hash();
        let set = |old: ChannelMix, new: ChannelMix| Change::Set {
            key: FieldKey::TrackMix { track: Id(2) },
            old: FieldValue::Mix(old),
            new: FieldValue::Mix(new),
        };
        let loud = ChannelMix {
            volume: 1500,
            pan: -250,
            mute: true,
            solo: false,
        };
        p.apply(&set(ChannelMix::default(), loud)).unwrap();
        assert_ne!(p.state_hash(), plain);
        let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.track(Id(2)).unwrap().mix, loud);
        assert_eq!(back.state_hash(), p.state_hash());
        let bad = ChannelMix {
            volume: MAX_VOLUME + 1,
            ..loud
        };
        assert!(p.apply(&set(loud, bad)).is_err());
        p.apply(&set(loud, ChannelMix::default())).unwrap();
        assert_eq!(p.state_hash(), plain);
        assert_eq!(MAX_PAN, 1000);
        assert_eq!(UNITY_VOLUME, 1000);
    }

    #[test]
    fn effect_chains_hash_validate_and_invert() {
        let mut p = Project::new(Id(1), "fx");
        let plain = p.state_hash();
        let ins = |index, n| Change::InsertEffect {
            owner: EffectOwner::Master,
            index,
            effect: Effect::new(Id(n), EffectKind::Gate),
        };
        p.apply(&ins(0, 7)).unwrap();
        assert_ne!(p.state_hash(), plain);
        p.validate().unwrap();
        // Same id twice, bad position, bad parameter and bad track are all refused.
        assert!(p.apply(&ins(1, 7)).is_err());
        assert!(p.apply(&ins(5, 8)).is_err());
        let bad = Change::SetEffectParam {
            effect: Id(7),
            param: 0,
            old: 100,
            new: 5000,
        };
        assert_eq!(
            p.apply(&bad),
            Err(ChangeError::OutOfRange("effect parameter"))
        );
        let stale = Change::SetEffectParam {
            effect: Id(7),
            param: 0,
            old: 1,
            new: 2,
        };
        assert_eq!(p.apply(&stale), Err(ChangeError::Stale("effect parameter")));
        assert!(p
            .apply(&Change::InsertEffect {
                owner: EffectOwner::Track { track: Id(99) },
                index: 0,
                effect: Effect::new(Id(9), EffectKind::Delay),
            })
            .is_err());
        // A hand edited project with a wrong parameter count fails validation.
        let mut q = p.clone();
        q.effects[0].effects[0].params.pop();
        assert_eq!(q.validate().unwrap_err().code(), "effect_invalid");
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("bypass"));
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        p.apply(&ins(0, 7).inverse()).unwrap();
        assert_eq!(p.state_hash(), plain);
        assert!(p.effects.is_empty() && p.index_is_consistent());
    }
}
