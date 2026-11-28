//! Live parts: build once, update in place (UI foundation, #1681).
//!
//! A screen builds its tree once, when it opens or when what it lists changes (a search, a new
//! language). After that each part that shows data keeps that data in a [`Live`] component; when
//! the data changes, [`refresh`] rebuilds that part's children and nothing else. A value step on
//! one settings row rebuilds that row's control, not the panel; the rest of the tree, its scroll
//! position and its focus stay as they are.

use bevy::prelude::*;

/// The data a part's children were built from.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Live<T: Clone + PartialEq + Send + Sync + 'static>(pub T);

/// Rebuilds `entity`'s children with `build` when `value` differs from what they were built
/// from (or they were never built). Returns whether it rebuilt.
pub fn refresh<T: Clone + PartialEq + Send + Sync + 'static>(
    world: &mut World,
    entity: Entity,
    value: &T,
    build: impl FnOnce(&mut ChildSpawner, &T),
) -> bool {
    let Ok(mut e) = world.get_entity_mut(entity) else {
        return false;
    };
    if e.get::<Live<T>>().is_some_and(|l| &l.0 == value) {
        return false;
    }
    e.despawn_related::<Children>();
    e.insert(Live(value.clone()));
    e.with_children(|c| build(c, value));
    true
}

/// [`refresh`] from a system with `Commands`: queued, so it runs when the commands apply.
pub fn queue_refresh<T: Clone + PartialEq + Send + Sync + 'static>(
    commands: &mut Commands,
    entity: Entity,
    value: T,
    build: impl FnOnce(&mut ChildSpawner, &T) + Send + 'static,
) {
    commands.queue(move |world: &mut World| {
        refresh(world, entity, &value, build);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component)]
    struct Leaf;

    #[test]
    fn rebuilds_only_on_a_change() {
        let mut world = World::new();
        let part = world.spawn(Node::default()).id();
        let build = |c: &mut ChildSpawner, n: &u32| {
            for _ in 0..*n {
                c.spawn(Leaf);
            }
        };
        assert!(refresh(&mut world, part, &2u32, build));
        let first: Vec<Entity> = world.get::<Children>(part).unwrap().to_vec();
        assert!(!refresh(&mut world, part, &2u32, build), "same data: nothing to do");
        assert_eq!(world.get::<Children>(part).unwrap().to_vec(), first, "the children stay");
        assert!(refresh(&mut world, part, &3u32, build));
        assert_eq!(world.get::<Children>(part).unwrap().len(), 3);
        assert!(first.iter().all(|e| world.get_entity(*e).is_err()), "the old children are gone");
    }
}
