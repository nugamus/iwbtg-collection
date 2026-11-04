//! Trace setup actions, behind the `trace-setup` feature (docs/plans/sim-core.md 9.2, 9.4, 10.4).
//!
//! A fixed vocabulary that replaces the recorder's `--gml` assignments, `--x` and `--y`.
//! Applying one sets `World::setup_applied`, which is hashed, so a run that used setup can never
//! match a clean verification. The server refuses to start when this feature is on. WP0
//! declares; WP8a owns.
//!
//! Setup only assigns: it runs no event, creates no instance and reads no file. A value a
//! setter rejects (non-finite, N15) or a field setup cannot write is a [`SimError`], and the
//! caller halts the run there.

use std::sync::Arc;

use crate::def::{GameDef, RoomId, SpriteId, VarId};
use crate::error::SimError;
use crate::num::{self, Real};
use crate::probe::{FieldRef, FixedColumn, ImageField, ProbeTarget, SaveField, first_of_target};
use crate::room;
use crate::rules::RunnerRules;
use crate::save::SaveSlot;
use crate::script::ScriptRuntime;
use crate::sim::{Sim, SimOptions};
use crate::world::{Ctx, InstId, Instance, Value, World};

/// One setup action.
#[derive(Clone, PartialEq, Debug)]
pub enum SetupAction {
    /// Set a field of an instance (the kid, the first of a kind, a named instance).
    SetField {
        /// Whose field.
        target: ProbeTarget,
        /// Which field.
        field: FieldRef,
        /// The value.
        value: Value,
    },
    /// Set a global.
    SetGlobal {
        /// The global.
        var: VarId,
        /// The value.
        value: Value,
    },
    /// Set a save slot field (creates the slot if missing).
    SetSave {
        /// The field.
        field: SaveField,
        /// The value.
        value: Value,
    },
}

impl Sim {
    /// Applies one setup action and sets `World::setup_applied`.
    ///
    /// `SetField` writes through the instance's setters (`Motion`, `Image`), so a non-finite
    /// value is `SimError::NonFinite`; alarms store `num::alarm_from_real` of the value (R3b) and
    /// `visible`/`solid` take `num::gm_bool` of it. `ProbeTarget::Row` names no instance here and
    /// is an error, as is a target with no instance (setup never creates one) and a field setup
    /// cannot write (`id`, `room`, kid fields: `kid::api` has no setter). `SetSave` on a missing
    /// slot first creates one with every field zero (facing 1, room 0).
    pub fn apply_setup(&mut self, s: &SetupAction) -> Result<(), SimError> {
        self.world.setup_applied = true;
        apply(&self.def, &self.rules, &mut self.world, s)
    }

    /// Like [`Sim::new`], but applies `pre_entry` to the world after the rules, the script host
    /// and the empty world exist and before `room::start_game` creates the boot instances and
    /// enters the first room: the recorder's `--gml` code that ran before its `--room` jump
    /// (plan 9.2). Only globals and the save slot exist at that point, so a `SetField` here fails
    /// with "no instance".
    ///
    /// The steps are the ones `Sim::new` documents (rules from the game's runner and engine_rev,
    /// `ScriptRuntime::new`, `World::new` in the start room with the seed, a `Ctx` carrying
    /// `options.dll_mode`, `room::start_game`); a run built here and one built by `Sim::new` with
    /// no setup must stay identical apart from `setup_applied`.
    pub fn new_with_setup(
        def: Arc<GameDef>,
        options: SimOptions,
        pre_entry: &[SetupAction],
    ) -> Result<Sim, SimError> {
        let rules = RunnerRules::for_runner(def.runner, def.engine_rev)?;
        let scripts = ScriptRuntime::new(&def, &rules)?;
        let first_room = match &options.start {
            room::StartPoint::NewGame { .. } => def.new_game_room,
            room::StartPoint::Room { room, .. } => *room,
            room::StartPoint::FromSave(slot) => slot.room,
        };
        let mut world = World::new(first_room, options.seed);
        for s in pre_entry {
            world.setup_applied = true;
            apply(&def, &rules, &mut world, s)?;
        }
        {
            let mut ctx = Ctx {
                def: &def,
                rules: &rules,
                w: &mut world,
                scripts: &scripts,
                dll_mode: options.dll_mode,
            };
            room::start_game(&mut ctx, &options.start)?;
        }
        Ok(Sim {
            def,
            rules,
            world,
            options,
            scripts,
            rows_done: false,
        })
    }
}

/// Applies one action to a world.
fn apply(def: &GameDef, rules: &RunnerRules, w: &mut World, s: &SetupAction) -> Result<(), SimError> {
    match s {
        SetupAction::SetField { target, field, value } => {
            let id = target_instance(def, rules, w, target)?;
            let inst = w
                .get_mut(id)
                .ok_or_else(|| SimError::invariant(format!("setup target {} not stored", id.0)))?;
            set_field(inst, field, value, rules)
        }
        SetupAction::SetGlobal { var, value } => {
            w.globals.insert(*var, value.clone());
            Ok(())
        }
        SetupAction::SetSave { field, value } => set_save(def, w, *field, value),
    }
}

/// The instance a `SetField` writes.
fn target_instance(
    def: &GameDef,
    rules: &RunnerRules,
    w: &World,
    t: &ProbeTarget,
) -> Result<InstId, SimError> {
    let found = match t {
        ProbeTarget::Row => {
            return Err(SimError::unsupported("setup target `Row` names no instance"));
        }
        ProbeTarget::Kid => w.kid(&rules.tick),
        ProbeTarget::First(crate::collide::Target::Class(c)) => w.first_of(*c, &rules.tick),
        ProbeTarget::First(t) => first_of_target(def, w, &rules.tick, *t),
        ProbeTarget::Named(n) => w
            .iter_alive()
            .find(|i| i.active && i.name == Some(*n))
            .map(|i| i.id),
    };
    found
        .ok_or_else(|| SimError::unsupported(format!("setup target {t:?}: no instance (setup creates none)")))
}

/// A real from a setup value.
fn real(v: &Value, what: &str) -> Result<Real, SimError> {
    match v {
        Value::Real(r) if r.is_finite() => Ok(*r),
        Value::Real(_) => Err(SimError::non_finite(what)),
        other => Err(SimError::unsupported(format!(
            "setup {what}: expects a real, got {other:?}"
        ))),
    }
}

/// An integer from a setup value: a finite real with no fraction.
fn int(v: &Value, what: &str) -> Result<i64, SimError> {
    let r = real(v, what)?;
    let i = num::round::to_int_trunc(r).map_err(|e| SimError::from_num(what, e))?;
    #[allow(
        clippy::cast_precision_loss,
        reason = "exactness is checked by comparing the bits"
    )]
    let back = Real::new(i as f64);
    // `+ 0.0` turns -0 into +0, which is the integer 0.
    if back.to_bits() != Real::new(r.get() + 0.0).to_bits() {
        return Err(SimError::unsupported(format!(
            "setup {what}: {} is not an integer",
            r.get()
        )));
    }
    Ok(i)
}

/// An integer that fits `T`.
fn int_as<T: TryFrom<i64>>(v: &Value, what: &str) -> Result<T, SimError> {
    let i = int(v, what)?;
    T::try_from(i).map_err(|_| SimError::unsupported(format!("setup {what}: {i} is out of range")))
}

/// Writes one field of an instance.
fn set_field(
    inst: &mut Instance,
    field: &FieldRef,
    value: &Value,
    rules: &RunnerRules,
) -> Result<(), SimError> {
    match field {
        FieldRef::Fixed(c) => {
            let what = c.name();
            match c {
                FixedColumn::X => inst.m.set_x(real(value, what)?),
                FixedColumn::Y => inst.m.set_y(real(value, what)?),
                FixedColumn::Xprevious => inst.m.set_xprevious(real(value, what)?),
                FixedColumn::Yprevious => inst.m.set_yprevious(real(value, what)?),
                FixedColumn::Hspeed => inst.m.set_hspeed(real(value, what)?),
                FixedColumn::Vspeed => inst.m.set_vspeed(real(value, what)?),
                FixedColumn::Speed => inst.m.set_speed(real(value, what)?),
                FixedColumn::Direction => inst.m.set_direction(real(value, what)?),
                FixedColumn::Gravity => inst.m.set_gravity(real(value, what)?),
                FixedColumn::GravityDirection => inst.m.set_gravity_direction(real(value, what)?),
                FixedColumn::Friction => inst.m.set_friction(real(value, what)?),
                FixedColumn::ImageIndex => inst.img.set_index(real(value, what)?),
                FixedColumn::SpriteIndex => {
                    let i: i64 = int(value, what)?;
                    let sprite = if i < 0 {
                        None
                    } else {
                        Some(SpriteId(int_as::<u32>(value, what)?))
                    };
                    inst.img.set_sprite(sprite);
                    Ok(())
                }
                FixedColumn::Frame
                | FixedColumn::Room
                | FixedColumn::Object
                | FixedColumn::Id
                | FixedColumn::SpriteName => {
                    Err(SimError::unsupported(format!("setup cannot write `{what}`")))
                }
            }
        }
        FieldRef::Alarm(n) => {
            let what = format!("alarm[{n}]");
            let v = real(value, &what)?;
            let slot = inst
                .alarms
                .get_mut(usize::from(*n))
                .ok_or_else(|| SimError::unsupported(format!("setup {what}: no such alarm")))?;
            *slot = num::alarm_from_real(v, &rules.num);
            Ok(())
        }
        FieldRef::Image(f) => match f {
            ImageField::Speed => inst.img.set_speed(real(value, "image_speed")?),
            ImageField::Xscale => inst.img.set_xscale(real(value, "image_xscale")?),
            ImageField::Yscale => inst.img.set_yscale(real(value, "image_yscale")?),
            ImageField::Angle => inst.img.set_angle(real(value, "image_angle")?),
            ImageField::Alpha => inst.img.set_alpha(real(value, "image_alpha")?),
            ImageField::Visible => {
                inst.visible = num::gm_bool(real(value, "visible")?, &rules.num);
                Ok(())
            }
            ImageField::Solid => {
                inst.solid = num::gm_bool(real(value, "solid")?, &rules.num);
                Ok(())
            }
            ImageField::Depth => {
                inst.depth = real(value, "depth")?;
                Ok(())
            }
        },
        FieldRef::Var(v) => {
            if let Value::Real(r) = value
                && !r.is_finite()
            {
                return Err(SimError::non_finite("custom variable"));
            }
            inst.vars.insert(*v, value.clone());
            Ok(())
        }
        FieldRef::Kid(f) => Err(SimError::unsupported(format!(
            "setup of kid field `{}`: kid::api has no setter (OPEN, WP5a)",
            f.name
        ))),
    }
}

/// Writes one save slot field, creating an all-zero slot first when none exists.
fn set_save(def: &GameDef, w: &mut World, field: SaveField, value: &Value) -> Result<(), SimError> {
    if field == SaveField::Exists {
        if int(value, "save.exists")? == 0 {
            w.save = None;
        } else if w.save.is_none() {
            w.save = Some(empty_slot());
        }
        return Ok(());
    }
    let slot = w.save.get_or_insert_with(empty_slot);
    match field {
        SaveField::Exists => {}
        SaveField::Room => {
            let r = RoomId(int_as::<u32>(value, "save.room")?);
            if def.room(r).is_none() {
                return Err(SimError::unsupported(format!("setup save.room: no room {}", r.0)));
            }
            slot.room = r;
        }
        SaveField::X => slot.x = real(value, "save.x")?,
        SaveField::Y => slot.y = real(value, "save.y")?,
        SaveField::Flipped => slot.flipped = int(value, "save.flipped")? != 0,
        SaveField::Facing => slot.facing = int_as::<i8>(value, "save.facing")?,
        SaveField::Difficulty => slot.difficulty = int_as::<u8>(value, "save.difficulty")?,
        SaveField::Deaths => slot.deaths = int_as::<u32>(value, "save.deaths")?,
        SaveField::Time => slot.time = int_as::<u32>(value, "save.time")?,
        SaveField::Bosses => slot.bosses = int_as::<u16>(value, "save.bosses")?,
        SaveField::Items => slot.items = int_as::<u16>(value, "save.items")?,
        SaveField::Clear => slot.clear = int(value, "save.clear")? != 0,
    }
    Ok(())
}

/// A save slot with every field zero, facing right, in room 0.
fn empty_slot() -> SaveSlot {
    SaveSlot {
        room: RoomId(0),
        x: Real::ZERO,
        y: Real::ZERO,
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 0,
        time: 0,
        bosses: 0,
        items: 0,
        clear: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::testing::GameDefBuilder;
    use crate::def::{Builtin, KindDef, RoomDef};
    use crate::room::StartPoint;
    use crate::sound::DllMode;

    fn sim() -> Sim {
        let mut b = GameDefBuilder::new("setup-test");
        b.kind(KindDef::new("block", Builtin::Block));
        b.room(RoomDef::new("room_1", 800, 608));
        b.def_mut().var_names.insert("grav".into(), VarId(0));
        let def = b.build().expect("def");
        Sim {
            def: Arc::new(def),
            rules: RunnerRules::default(),
            world: World::new(RoomId(0), 0),
            options: SimOptions {
                start: StartPoint::Room {
                    room: RoomId(0),
                    difficulty: 0,
                },
                seed: 0,
                dll_mode: DllMode::Stub,
            },
            scripts: ScriptRuntime::default(),
            rows_done: false,
        }
    }

    #[test]
    fn globals_and_save_fields_set_the_flag() {
        let mut s = sim();
        assert!(!s.world.setup_applied);
        s.apply_setup(&SetupAction::SetGlobal {
            var: VarId(0),
            value: Value::Real(Real::ONE),
        })
        .expect("global");
        assert!(s.world.setup_applied, "setup is visible to the state hash");
        assert_eq!(s.world.globals.get(&VarId(0)), Some(&Value::Real(Real::ONE)));
        s.apply_setup(&SetupAction::SetSave {
            field: SaveField::Deaths,
            value: Value::Real(Real::new(12.0)),
        })
        .expect("save");
        let slot = s.world.save.clone().expect("slot created");
        assert_eq!((slot.deaths, slot.facing, slot.room), (12, 1, RoomId(0)));
        for (field, v) in [
            (SaveField::Deaths, 1.5),
            (SaveField::Facing, 300.0),
            (SaveField::Room, 4.0),
            (SaveField::X, f64::INFINITY),
        ] {
            assert!(
                s.apply_setup(&SetupAction::SetSave {
                    field,
                    value: Value::Real(Real::new(v)),
                })
                .is_err(),
                "{field:?} = {v}"
            );
        }
        s.apply_setup(&SetupAction::SetSave {
            field: SaveField::Exists,
            value: Value::Real(Real::ZERO),
        })
        .expect("delete");
        assert!(s.world.save.is_none());
    }

    #[test]
    fn field_setup_needs_an_instance_and_a_writable_field() {
        let mut s = sim();
        let row = SetupAction::SetField {
            target: ProbeTarget::Row,
            field: FieldRef::Fixed(FixedColumn::X),
            value: Value::Real(Real::ONE),
        };
        assert!(matches!(s.apply_setup(&row), Err(SimError::Unsupported { .. })));
        let id = s.world.alloc_id().expect("id");
        s.world
            .insert(Instance::new(
                id,
                crate::def::KindId(0),
                0,
                Real::ZERO,
                Real::ZERO,
            ))
            .expect("insert");
        let mut inst = s.world.get(id).expect("inst").clone();
        assert!(
            set_field(
                &mut inst,
                &FieldRef::Fixed(FixedColumn::Id),
                &Value::Real(Real::ONE),
                &s.rules
            )
            .is_err()
        );
        let kid_field = FieldRef::Kid(crate::kid::api::KidField {
            name: "djump".into(),
            index: None,
        });
        assert!(set_field(&mut inst, &kid_field, &Value::Real(Real::ONE), &s.rules).is_err());
        set_field(
            &mut inst,
            &FieldRef::Image(ImageField::Depth),
            &Value::Real(Real::new(-5.0)),
            &s.rules,
        )
        .expect("depth");
        assert_eq!(inst.depth, Real::new(-5.0));
        assert!(
            set_field(
                &mut inst,
                &FieldRef::Var(VarId(0)),
                &Value::Real(Real::new(f64::NAN)),
                &s.rules
            )
            .is_err()
        );
    }

    #[test]
    fn integers_must_be_exact() {
        assert_eq!(int(&Value::Real(Real::new(3.0)), "t"), Ok(3));
        assert_eq!(int(&Value::Real(Real::new(-0.0)), "t"), Ok(0));
        assert!(int(&Value::Real(Real::new(2.5)), "t").is_err());
        assert!(int(&Value::Str("1".into()), "t").is_err());
        assert!(int_as::<u8>(&Value::Real(Real::new(256.0)), "t").is_err());
    }
}
