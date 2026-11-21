# Built-in objects

Part of [format v0](v0.md).

The 50 built-in kinds of v0, grouped as in the research list (`research/specs/mechanics.md`, "Final kind list (research#12)"). Each is documented once in `docs/format/builtins/<kind>.md`: parameters, defaults, events and behavior. The pages are authoritative; v0.md does not copy them. Each kind is one built-in: the differences between the original objects become parameters, and engine families never get separate kinds. The cherry is `killer`, and the original's player_start is `kid_start`. `moving_block`, `roof`, `music_zone` and `projectile` are not kinds (`projectile`: client#775).

The parameter survey ([builtins-survey.md](builtins-survey.md)) lists every parameter, its games and its keep/move verdict under decision 0004's promotion and demotion rule (#1076). A parameter used by one game and not a general concept is a move candidate and is not added to another kind.

## Standard behaviours

Decision [0005](../decisions/0005-standard-mechanics.md) groups the kinds into twelve native behaviours: **spawn** (`kid_start`), **checkpoint** (`save`), **portal** (`warp`, `edge_warp`, `difficulty_warp`), **trigger** (`trigger`), **hazard** (`spike`, `killer`, `orbit`), **pickup** (`boss_item`, `secret_item`, `item_viewer`), **overlay** (`game_over`, `playerface`, `bow`, `sign`, `music`, `death_sound`, `camera`), **emitter** (`blood`, `blood_emitter`, `particle_system`), **solid/surface** (`block`, `platform`, `slope`, `ladder`, `vine`, `fake_block`, `hidden_block`, `breakable_block`, `conveyor`, `bullet_blocker`, `trap_stopper`, `boss_gate`), **mover/projectile** (`moving_platform`, `drop_platform`, `path_platform`, `trap`, `kid_bullet`), **kid zone** (`water`, `gravity_flip`, `kid_param_zone`, `jump_refresher`, `spring`) and **goal** (`game_clear`, `time_limit`), plus the controller (`world`), the kid and the data bases (`decor`, `scripted`). The 50 kind names above stay the creator vocabulary; `base = "<behaviour>"` is additive and lands in T2 (#1277). The architecture page describes the trait table that backs both ([builtins](../architecture/sim/builtins.md#behaviours-and-the-trait-table)).

**Engine kinds (spawned by the sim or placed once per game).**

- [`kid`](builtins/kid.md): The player character; its events run the action lists of `physics.toml`.
- [`kid_start`](builtins/kid_start.md): The player start; creates the kid at room start (the original's player_start).
- [`kid_bullet`](builtins/kid_bullet.md): The kid's bullet.
- [`blood`](builtins/blood.md): One blood drop from the kid's death.
- [`blood_emitter`](builtins/blood_emitter.md): Makes the blood when the kid dies.
- [`bow`](builtins/bow.md): The Medium-mode ribbon that follows the kid; cosmetic.
- [`game_over`](builtins/game_over.md): The GAME OVER display the kid's death creates.
- [`playerface`](builtins/playerface.md): The face on the death screen (one RNG draw on Create).
- [`death_sound`](builtins/death_sound.md): Family B's death sound object.
- [`world`](builtins/world.md): The engine controller placed in `[engine] boot`: pause, mute and the play timer.
- [`difficulty_warp`](builtins/difficulty_warp.md): The new-game warp that sets the difficulty and clears the save.
- [`camera`](builtins/camera.md): Writes view 0: screen snap or smooth follow.
- [`music`](builtins/music.md): Plays a track.
- [`particle_system`](builtins/particle_system.md): Builds a runner particle system from `particles/<id>.toml`.
- [`scripted`](builtins/scripted.md): Base of an object that has a `script` and no other `base`; no native behavior.
- [`decor`](builtins/decor.md): Sprite-only object with no behavior.

**Solids and platforms.**

- [`block`](builtins/block.md): Solid wall; the kid's collision handlers do the work.
- [`platform`](builtins/platform.md): The one-way platform.
- [`moving_platform`](builtins/moving_platform.md): A platform that moves by itself, with the carry rules of each original family.
- [`drop_platform`](builtins/drop_platform.md): A platform that falls or rises when the kid stands on it.
- [`path_platform`](builtins/path_platform.md): A platform that follows a path.
- [`slope`](builtins/slope.md): Solid with a precise mask; the kid's `[slope]` model does the climbing (the draft `roof` is a slope).
- [`fake_block`](builtins/fake_block.md): Looks like a block but is not solid, or vanishes.
- [`hidden_block`](builtins/hidden_block.md): A block that is invisible until a kid comes near.
- [`breakable_block`](builtins/breakable_block.md): A solid block that breaks.
- [`boss_gate`](builtins/boss_gate.md): A solid block that destroys itself once its boss flag is set.
- [`conveyor`](builtins/conveyor.md): A solid block that pushes the kid sideways.
- [`trap_stopper`](builtins/trap_stopper.md): Stops or destroys a trap that touches it.

**Killers.**

- [`spike`](builtins/spike.md): Static killer; `dir` picks the sprite.
- [`killer`](builtins/killer.md): A generic killer, and the cherry (the draft `projectile`'s role is client#775).
- [`orbit`](builtins/orbit.md): Killers that move on a circle.

**Triggers and traps.**

- [`trigger`](builtins/trigger.md): Touch zone that fires a trap id under one of five models.
- [`trap`](builtins/trap.md): A mover that launches when its trigger fired (the draft `moving_block` is a `trap` with `kind = block`).

**Kid zones.**

- [`water`](builtins/water.md): Water: fall cap, refreshed jumps and air jump, by `variant`.
- [`vine`](builtins/vine.md): Wall-jump surface.
- [`ladder`](builtins/ladder.md): A ladder; the climbing is the kid's `ladder` action.
- [`gravity_flip`](builtins/gravity_flip.md): Flips the kid's gravity.
- [`kid_param_zone`](builtins/kid_param_zone.md): Writes kid numbers (speeds, jumps) while a kid overlaps it.
- [`jump_refresher`](builtins/jump_refresher.md): Gives the air jump back, then hides for a while.
- [`spring`](builtins/spring.md): Throws the kid upward.
- [`bullet_blocker`](builtins/bullet_blocker.md): Destroys kid bullets that touch it.

**Progress, rooms and items.**

- [`save`](builtins/save.md): A save point.
- [`warp`](builtins/warp.md): Room travel.
- [`edge_warp`](builtins/edge_warp.md): Room change when the kid leaves through an edge.
- [`game_clear`](builtins/game_clear.md): Ends the run.
- [`boss_item`](builtins/boss_item.md): The reward of a boss.
- [`secret_item`](builtins/secret_item.md): A collectible secret item.
- [`item_viewer`](builtins/item_viewer.md): Shows a collected boss reward or item.
- [`time_limit`](builtins/time_limit.md): A countdown (or its start zone) that does something when it runs out.
- [`sign`](builtins/sign.md): A sign that raises a text box.

Event phases follow the original events: a kind runs its behavior in the event where the original object did, and when games differ a parameter picks the event (`moving_platform` `carry_rule`, `camera` `on_*`). Each page names its events.

There is no trigger action vocabulary in v0: the `trigger` built-in fires a trap id, and anything else is a script.
