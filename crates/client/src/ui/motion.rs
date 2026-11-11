//! Motion: OutQuint for most transitions (300 to 500 ms), OutElastic for small pops, and a
//! reduced-motion switch that makes every tween land at once.
//!
//! A `Tween<P>` drives one property `P` of its entity: `OffsetX`/`OffsetY` (a `UiTransform`
//! translation, so layout does not move), `Scale`, `Width` (`Node::width`) or `Opacity`
//! (`UiOpacity`). It stays on the entity when finished, so `retarget` can send it somewhere else
//! from wherever it is now.

use std::marker::PhantomData;

use bevy::ecs::component::Mutable;
use bevy::prelude::*;

use super::shape::UiOpacity;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<ReducedMotion>().add_systems(
        Update,
        (
            run::<OffsetX>,
            run::<OffsetY>,
            run::<Scale>,
            run::<Width>,
            run::<Opacity>,
        ),
    );
}

/// When on, tweens jump to their end: no slides, fades or pops.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReducedMotion(pub bool);

/// An easing curve from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    /// Fast start, long soft landing: `1 - (1 - t)^5`.
    OutQuint,
    /// Overshoots and settles: the pop of a row that widens.
    OutElastic,
}

impl Ease {
    /// The eased value at `t` (clamped to 0..=1). Every curve gives 0 at 0 and 1 at 1.
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::OutQuint => 1.0 - (1.0 - t).powi(5),
            Ease::OutElastic => {
                if t <= 0.0 || t >= 1.0 {
                    return t;
                }
                let period = std::f32::consts::TAU / 3.0;
                2f32.powf(-10.0 * t) * ((10.0 * t - 0.75) * period).sin() + 1.0
            }
        }
    }
}

/// A property a tween can drive.
pub trait Prop: Send + Sync + 'static {
    type Target: Component<Mutability = Mutable>;
    fn set(target: &mut Self::Target, value: f32);
}

/// `UiTransform` translation x, logical px.
pub struct OffsetX;
/// `UiTransform` translation y, logical px.
pub struct OffsetY;
/// `UiTransform` scale, both axes.
pub struct Scale;
/// `Node::width`, logical px.
pub struct Width;
/// `UiOpacity`, 0 to 1.
pub struct Opacity;

impl Prop for OffsetX {
    type Target = UiTransform;
    fn set(target: &mut UiTransform, value: f32) {
        target.translation.x = Val::Px(value);
    }
}

impl Prop for OffsetY {
    type Target = UiTransform;
    fn set(target: &mut UiTransform, value: f32) {
        target.translation.y = Val::Px(value);
    }
}

impl Prop for Scale {
    type Target = UiTransform;
    fn set(target: &mut UiTransform, value: f32) {
        target.scale = Vec2::splat(value);
    }
}

impl Prop for Width {
    type Target = Node;
    fn set(target: &mut Node, value: f32) {
        target.width = Val::Px(value);
    }
}

impl Prop for Opacity {
    type Target = UiOpacity;
    fn set(target: &mut UiOpacity, value: f32) {
        target.0 = value;
    }
}

/// Moves property `P` from `from` to `to` over `duration` seconds along `ease`.
#[derive(Component, Debug)]
pub struct Tween<P: Prop> {
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub ease: Ease,
    elapsed: f32,
    /// The last value written, so a finished tween stops touching its target.
    written: Option<f32>,
    _prop: PhantomData<P>,
}

impl<P: Prop> Tween<P> {
    /// A tween that starts now.
    pub fn new(from: f32, to: f32, duration: f32, ease: Ease) -> Tween<P> {
        Tween {
            from,
            to,
            duration,
            ease,
            elapsed: 0.0,
            written: None,
            _prop: PhantomData,
        }
    }

    /// A tween resting at `value`, ready to be retargeted.
    pub fn at_rest(value: f32, duration: f32, ease: Ease) -> Tween<P> {
        let mut tween = Tween::new(value, value, duration, ease);
        tween.elapsed = duration;
        tween
    }

    /// The value now.
    pub fn value(&self) -> f32 {
        let t = if self.duration <= 0.0 {
            1.0
        } else {
            self.elapsed / self.duration
        };
        self.from + (self.to - self.from) * self.ease.at(t)
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Heads for `to` from the current value, restarting the clock. Same target: no change.
    pub fn retarget(&mut self, to: f32) {
        if to.to_bits() == self.to.to_bits() {
            return;
        }
        self.from = self.value();
        self.to = to;
        self.elapsed = 0.0;
    }

    /// Advances by `dt` seconds (to the end at once under reduced motion); the value to write,
    /// if it changed.
    fn step(&mut self, dt: f32, reduced: bool) -> Option<f32> {
        self.elapsed = if reduced {
            self.duration
        } else {
            (self.elapsed + dt).min(self.duration)
        };
        let value = self.value();
        (self.written != Some(value)).then(|| {
            self.written = Some(value);
            value
        })
    }
}

fn run<P: Prop>(
    time: Res<Time>,
    reduced: Res<ReducedMotion>,
    mut tweens: Query<(&mut Tween<P>, &mut P::Target)>,
) {
    let dt = time.delta_secs();
    for (mut tween, mut target) in &mut tweens {
        if tween.finished() && tween.written.is_some() && !reduced.is_changed() {
            continue;
        }
        if let Some(value) = tween.step(dt, reduced.0) {
            P::set(&mut target, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_start_at_zero_and_end_at_one() {
        for ease in [Ease::OutQuint, Ease::OutElastic] {
            assert_eq!(ease.at(0.0), 0.0, "{ease:?}");
            assert_eq!(ease.at(1.0), 1.0, "{ease:?}");
        }
        // OutQuint is past 96% at half time; OutElastic overshoots.
        assert!(Ease::OutQuint.at(0.5) > 0.96);
        assert!((0..100).any(|i| Ease::OutElastic.at(i as f32 / 100.0) > 1.0));
    }

    #[test]
    fn a_tween_eases_and_retargets_from_where_it_is() {
        let mut t = Tween::<OffsetX>::new(0.0, -25.0, 0.4, Ease::OutQuint);
        assert_eq!(t.step(0.2, false), Some(-25.0 * Ease::OutQuint.at(0.5)));
        let mid = t.value();
        t.retarget(0.0);
        assert_eq!(t.from, mid);
        t.step(0.4, false);
        assert_eq!(t.value(), 0.0);
        assert!(t.finished());
        assert_eq!(t.step(0.1, false), None, "a finished tween writes nothing new");
    }

    #[test]
    fn reduced_motion_lands_at_once() {
        let mut t = Tween::<Width>::new(820.0, 840.0, 0.5, Ease::OutElastic);
        assert_eq!(t.step(0.016, true), Some(840.0));
        assert!(t.finished());
    }
}
