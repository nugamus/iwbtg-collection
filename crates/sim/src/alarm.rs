//! Alarms (docs/plans/sim-core.md 2.1 row 4, R3, R3b). WP0 declared; WP1 owns.
//!
//! CLONE_SPEC tick loop, Confirmed (recorder): in the alarm phase, for each instance, alarms 0 to
//! 11 in index order; an alarm whose object has that alarm event counts down by 1 and its event
//! runs when it reaches 0 (traces/rely-the-needle-4-2/t-evo-1.csv rows 3-5). A fired alarm that
//! is not re-armed reads 0 until the next alarm phase and -1 after it, without firing again
//! (traces/go-the-jun-g-world/num-eps-alpha.csv rows 45-46). An alarm with no event does not
//! count down (traces/kaenbyou/num-alarm-half-b.csv); the phase only calls [`tick`] for alarms
//! the kind handles. An alarm assigned exactly 0 is still open (U3); it is stored like a fired
//! alarm, so it goes to -1 without firing.

use crate::error::SimError;
use crate::num::{self, Real};
use crate::rules::NumRules;
use crate::world::Instance;

/// The idle value a fired alarm settles to.
const IDLE: Real = Real::new(-1.0);

/// One alarm's countdown in the alarm phase. A value `> 0` loses 1 and fires when that leaves it
/// at or below 0; a value of exactly 0 (fired last tick, not re-armed) becomes -1 and does not
/// fire; a negative value is idle and stays.
pub fn tick(alarm: &mut Real) -> bool {
    let v = alarm.get();
    if v > 0.0 {
        *alarm = Real::new(v - 1.0);
        alarm.get() <= 0.0
    } else {
        if v == 0.0 {
            *alarm = IDLE;
        }
        false
    }
}

/// GML `alarm[n] = v`: stored through `num::alarm_from_real` (R3b; Confirmed (recorder): round
/// to nearest, ties to even, traces/kaenbyou/num-alarm-half-b.csv rows 10-31). Rejects `n >= 12`
/// (a script error) and non-finite values (N15). A whole value other than zero is stored as is,
/// which every `AlarmConv` mode gives for it.
pub fn set(inst: &mut Instance, n: u8, v: Real, rules: &NumRules) -> Result<(), SimError> {
    let slot = inst
        .alarms
        .get_mut(usize::from(n))
        .ok_or_else(|| SimError::Script {
            message: format!("alarm[{n}] does not exist (0-11)"),
        })?;
    if !v.is_finite() {
        return Err(SimError::non_finite(format!("alarm[{n}]")));
    }
    let x = v.get();
    *slot = if x.fract() == 0.0 && x != 0.0 {
        v
    } else {
        num::alarm_from_real(v, rules)
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::KindId;
    use crate::world::InstId;

    fn run(start: f64, ticks: usize) -> (Vec<f64>, Vec<bool>) {
        let mut a = Real::new(start);
        let mut values = Vec::new();
        let mut fired = Vec::new();
        for _ in 0..ticks {
            fired.push(tick(&mut a));
            values.push(a.get());
        }
        (values, fired)
    }

    #[test]
    fn three_counts_down_and_fires_on_the_third_tick_then_idles() {
        let (values, fired) = run(3.0, 5);
        assert_eq!(values, vec![2.0, 1.0, 0.0, -1.0, -1.0]);
        assert_eq!(fired, vec![false, false, true, false, false]);
    }

    #[test]
    fn idle_values_stay() {
        assert_eq!(run(-1.0, 3), (vec![-1.0; 3], vec![false; 3]));
        assert_eq!(run(-5.0, 1), (vec![-5.0], vec![false]));
    }

    #[test]
    fn set_rejects_bad_slots_and_values() {
        let mut inst = Instance::new(InstId::FIRST, KindId(0), 0, Real::ZERO, Real::ZERO);
        let rules = NumRules::default();
        assert!(matches!(
            set(&mut inst, 12, Real::ONE, &rules),
            Err(SimError::Script { .. })
        ));
        assert!(matches!(
            set(&mut inst, 0, Real::new(f64::NAN), &rules),
            Err(SimError::NonFinite { .. })
        ));
        set(&mut inst, 11, Real::new(40.0), &rules).unwrap();
        assert_eq!(inst.alarms[11], Real::new(40.0));
    }
}
