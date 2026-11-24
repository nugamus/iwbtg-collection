# The simulation: The kid

Part of [the simulation](../sim.md).

## The kid

The kid built-in: its profile model, the events it runs, and the spec rules behind each field or action. This is the live text of the [sim core plan](../../plans/sim-core.md) sections 5.1-5.4, which stay only as history. Spec: `research/specs/engines/gm8/kid/A1.md` .. `kid/C.md` (claims `A1-NNN`..`C-NNN`; index `research/specs/INDEX.md`), `mechanics.md` "Physics profile fields needed by the kid families" (claims `MECH-NNN`), `tricks.md`.

### Approach

The kid is one built-in whose events run an **ordered list of named actions** read from `physics.toml`. Each family's events are written as such lists. Every spec rule maps to either a field value or a named action (a Rust function in `kid/actions/`). Nothing family-specific hides in an `if family == B`. A `preset` key fills defaults per family (`kid/presets.rs`), but every field can be overridden.

```rust
// def/physics.rs (serde Deserialize, deny_unknown_fields)
pub struct PhysicsProfile {
    pub preset: KidFamily,                 // A1 A2 A3 A4 A5 B C: documentation and defaults
    pub numbers: KidNumbers,
    pub input: KidInput,
    pub events: KidEvents,                 // ordered action lists per event
    pub jump: JumpRules, pub fall_cap: FallCap, pub gravity: GravityRules,
    pub platform: PlatformRules, pub vine: VineRules, pub water: WaterRules,
    pub ice: IceRules, pub slope: SlopeModel, pub ladder: LadderRules,
    pub flip: FlipModel, pub death: DeathRules, pub bullet: BulletRules,
    pub save: SaveRules, pub misc: KidMisc,
}
// float_model lives in game.toml [engine] -> NumRules::float_model (N3): it is runner FPU state, not a kid field.
// kid/mod.rs
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow;
pub fn event_set(p: &PhysicsProfile) -> EventSet;
pub mod presets { pub fn apply(p: &mut PhysicsProfileDraft) -> PhysicsProfile; }
```

`KidState` (in `kid/state.rs`, with the family extras in sub-structs `KidStateB`, `KidStateExt`) holds: `djump, cur_jumps, tjump, on_platform, ladder, ladder_ud, ladder_jump, frozen, frozen2, yflag, flipped, facing, temp_hspeed, run_speed, jump_speeds, max_vspeed, max_fall_speed, fade_alpha, invincible, scale, on_slope_last`. Game-side latched key flags (wanna, rori, fairy-tale, showdown) live in `KidGlobals` because they persist while no kid exists (A3 Inferred, I6). Other built-ins reach the kid only through `kid::api` ([world model](world.md), Kid access).

### physics.toml (v0.1)

```toml
#:schema ../../client/docs/format/schema/v0/physics.schema.json
preset = "A1"                 # A1 | A2 | A3 | A4 | A5 | B | C

[numbers]
jumps = [8.5, 7.0]            # ground jump, then air jumps in order (B: jump[1], jump[2]; showdown adds 6.0)
gravity = 0.4
run_speed = 3.0
max_vspeed = 9.0
max_fall_speed = 8.0          # read only by fall_cap = "shift_then_clamp"
release_mult = 0.45           # 0.5 in machinary, eat-the-new-year
sprite_threshold = 0.05       # 0.5 in go-the-spook-house
spawn_offset = [17, 23]       # machinary [16, 23]
image_speed_idle = 0.2

[input]
walk = "direct_latch"         # direct_latch | sampled; see tricks trick.restart_glitch
tap_fix = false               # xia directionalTapFix; research gap, no field yet (#1155)
vine_keys = "edges"           # edges | latched_flags (wanna, rori, fairy-tale, showdown)

[events]
create = ["create_defaults", "ensure_world", "create_bow", "first_play_save", "arm_play_timer"]
room_start = []                # B: ["create_bow", "set_view_angle"]
step = ["gravity", "walk", "air_sprite", "fall_cap", "shoot", "jump", "release",
        "ad_nudge", "vines", "flip_swap", "platform_gravity_zero", "fade_in"]   # `gravity` stands for the ladder action's step 4 where the profile lists `ladder` (see below)
end_step = ["ladder_sprites"]
alarms = [{ n = 0, run = ["play_timer"] }]   # B: cancel poll (unused while cancels are on)
outside_room = []              # B: ["kill_unless_room_changer"]
keys = [{ action = "suicide", on = "press", run = ["kill"] }]   # machinary H/T/W, xia C: more rows
collisions = [
  { with = "block", run = "block_standard" },
  { with = "solidblock", run = "block_standard" },
  { with = "platform", run = "platform_standard" },
  { with = "killer", run = "kill" },
]
boundary = "intersect_origin"  # intersect_origin | outside_room_kill | end_step_origin | none

[jump]
ground_classes = ["block", "solidblock", "water1"]   # 1 px in gravity direction
ground_also = ["platform_flag", "ladder_jump"]
ground_sets_djump = true        # false in A4
air = "djump"                   # djump | cur_jumps (B) | djump_then_tjump (showdown)
water_air = "water2_without_flag"  # none | water2_without_flag | water_unlimited (B) | water3_keeps (C)
slope_force_within = 0          # rainbow, rori, fairy-tale: 2
release_twice_while_tjump = false
blocked_by_pause_press = false  # kaenbyou: true (Action::Pause)

[fall_cap]
rule = "clamp_abs"              # clamp_abs | clamp_against_gravity (B) | clamp_down_only (spook) | shift_then_clamp (A4)

[gravity]
reapply = "step"                # step | step_copy_grav (wanna) | end_step_slope (B) | end_step (little-runner) | create_only (showdown, xia)
block_landing_zeroes = true
platform_zero_in_step = true    # rely only (Step action 6)

[platform]
rule = "half_vspeed"            # y - vspeed/2 <= platform.y
snap = 9
take = "vspeed"                 # vspeed | delta_y (B: other.y - other.yprevious) | yspeed_flipped (C)
hold_probe = 4

[vine]
model = "distance_asym"         # distance_asym (A, B) | distance_sym_early (C) | none
left_below = 2.0
right_equal = 1.0
slide = 2.0
kick = [15.0, -9.0]
drop = 3.0

# [water], [ice], [slope], [ladder], [flip], [death], [bullet], [save], [misc]: see the rule table below
```

The standard names for the shared concerns are accepted alongside the current spellings (`docs/format/physics.md` "Standard names and aliases"): `[vines]`/`[vine]`, `[ladders]`/`[ladder]`, `[slopes]`/`[slope]`, `[platforms]`/`[platform]`, `max_fall`/`max_fall_speed`, and the `model =` selectors `[gravity] model` (`constant`, `step`, `end_step_slope`, `create_only`), `[vines] model` (`latched_keys`, `symmetric_early`), `[slopes] model` (`poronkusema`, `rainbow`, `fairy_tale`) and `[water] model` (`jump_refresh`, `full_jump`, `water3_keeps`). Each selector maps onto an existing field (`[gravity] reapply`, `[input] vine_keys`, `[jump] water_air`, `[slope] downhill`) in `presets::apply`; a field written explicitly wins. The aliases change no preset value and no `gameplay_hash` of an unchanged file.

Event lists name actions; a family's lists come from its spec's event sections in order. The `create` list must cover the spec's Create steps including the first-play save (`first_play_save`: when `World.save` is `None`, call `save::save_now`; rely Create step 4, B Create step 4) and "create world if missing" (`ensure_world`, via `EngineKind::World`).

### Spec rule to field or code path

Each row is one spec rule; the last column is the field or action that implements it. "A1 Step 3.5" means kid/A1.md "Step", action 3, step 5.

| Spec rule | Families | Field / action |
|---|---|---|
| Kid mask 11x21, origin (17,23); player2 mask origin (17,11); xia flip mask origin (17,8) | all | sprite `mask` of the kid kind; `[flip] mask_flipped` |
| Create defaults (jump 8.5, jump2 7, maxSpeed 3, gravity 0.4, maxVspeed 9, image_speed 0.2, djump true / curJumps 1) | all | `numbers.*`, `jump.air`, action `create_defaults` |
| Create: world if missing, bow, first-play save when no save exists, alarm[0] = room_speed | A1, A5, B | actions `ensure_world`, `create_bow`, `first_play_save`, `arm_play_timer` |
| B Create does not set gravity (0 until End Step) | B | `gravity.reapply = "end_step_slope"` and create gravity 0 |
| B Room Start: create bow, set view angle | B | `events.room_start` actions `create_bow`, `set_view_angle` |
| Create alpha 0 and +0.1 per Step (rely) | A1 rely | action `fade_in`, `misc.fade_in = true` |
| Gravity back to its constant every Step unless on a ladder (A1 Step action 2 step 4; the rely profile has no ladder) | A1 (rely) | action `gravity` (`gravity.reapply = "step"`) |
| Ladder: up/down while overlapping ladder grabs, moves 3 px, gravity/vspeed 0, jump leaves, else gravity 0.4 | A1 (little-trap, rainbow) | action `ladder` (two `direct` reads where the original reads twice), `[ladder] entry = "up_or_down" | "up_only"`. A profile that has ladders lists `ladder` instead of `gravity`: `ladder` includes the gravity reset (step 4), so the two are never listed together |
| Walk: right wins, frozen forces 0, facing unless near vine, vine-blocked direction keeps old hspeed | A, B, C | action `walk` (`InputState::direct` under `input.walk = "direct_latch"`) |
| Slip blocks: probe 4 px, read slip 10 px below, add/subtract `slip*sign(hspeed)`, no clamp | A1, A2, B | `[ice] model = "slip"`; uses `num::sign(.., &NumRules)` (N11) |
| Showdown ice: tempHspeed, decay 0.95, zero below 0.2 | A3 showdown | action `ice_temp_hspeed` |
| C slide block adds its h; slip clamped to 3 | C | action `slide_block`, `[ice] clamp = true` |
| Air sprite thresholds ±0.05 unless onPlatform; onPlatform cleared when no platform 4 px below | all | action `air_sprite`, `numbers.sprite_threshold`, `platform.hold_probe` |
| Fall cap variants (a)-(d) | per game | `fall_cap.rule` |
| Shoot, jump, release in that order in one Step | all | list order |
| Ground jump classes and extras | per game | `jump.ground_classes`, `jump.ground_also` |
| A4 ground jump does not set djump | A4 | `jump.ground_sets_djump = false` |
| B jump counter (first jump sets 2, walking off spends first, air jump uses jump[curJumps], water unlimited) | B | `jump.air = "cur_jumps"`, `jump.water_air = "water_unlimited"` |
| B jump ignores press when P pressed this tick (kaenbyou) | B kaenbyou | `jump.blocked_by_pause_press = true` (reads `pressed(Pause)`) |
| B disable-cancel switch with polling Alarm 0 | B | `jump.cancel = "edge" | "poll_alarm"` |
| Showdown triple jump | A3 showdown | `jump.air = "djump_then_tjump"`, `jump.release_twice_while_tjump`, `numbers.jumps[2]` |
| Release: `vspeed < 0` (against gravity in B, C) times release_mult | all | action `release` |
| A/D nudge ±1 while block 1 px below | animus, easy-miku-2 | action `ad_nudge`, `misc.ad_nudge` |
| Vines A/B (asym distances, slide, kick, drop) | A1, A2, B | action `vines`, `vine.model = "distance_asym"`; uses C13 |
| Vines C (both sides <= 1, flags before jump code) | C | actions `vine_probe` + `vines` |
| Vine keys as game-side latched flags | wanna, rori, fairy-tale, showdown | `input.vine_keys = "latched_flags"`, `KidGlobals` flags |
| Flip by object swap at y-4 | A (11 games) | action `flip_swap`, `[flip] model = "swap_object"`, rank via selector `{kind="kid", flipped=true}` |
| Flip by yflag (B) | B | `[flip] model = "yflag"` |
| Flip by grav sign (C) | C | `[flip] model = "grav_sign"` |
| Gravity re-set variants | per game | `gravity.reapply` |
| Rely Step action 6: gravity 0 while onPlatform | A1 rely | action `platform_gravity_zero` |
| Block collision 3 steps (h contact, v contact + landing refresh, diagonal) | all | collision run `block_standard` |
| Block landing gravity 0 | per game | `gravity.block_landing_zeroes`, per collision entry `zero_gravity = true` |
| Flipped refresh on ceiling contact; xia treats hspeed 0 as right when flipped | B, C | `block_standard` reads `flip.model` |
| Slope step-up, slope events, downhill snaps | rainbow, rori, fairy-tale, fed-up, poronkusema | `block_slope_step`, `slope_por_rise`, `downhill_snap_*`, `[slope] model` |
| B End Step playerSlope (always runs; restores gravity even without slopes, Confirmed) | B | end-step action `slope_b` |
| C slope code in Step (unused in xia rooms) | C | action `slope_c` (tier 3) |
| Little-runner End Step: gravity 0.4 every tick, unstuck | A5 | end-step action `gravity_and_unstuck` |
| Platform collision | all | collision run `platform_standard`, `[platform]` |
| poronkusema movingBlock collision | A1 poronkusema | collision run `moving_block_por` (tier 3) |
| Killer: collision event / Step before motion (B) / End Step (wanna, spook, xia) | per game | `events.collisions` entry `kill`, or step/end-step action `killer_check` |
| Edge handling per family; B Outside Room kill unless roomChanger | per game | `events.boundary`, `events.outside_room`, `[death] edge_*` |
| RORI/FT boundary looks up roomTo at the origin point first | rori, fairy-tale | `[death] edge_lookup_point_first` (K2) |
| Suicide: Key Q pressed / held in Step (wanna) / press in Step (xia) / none | per game | `events.keys` or step action `suicide_check` |
| Other kid key events (machinary H, T, W; xia C teleport) | machinary, xia | `events.keys` rows |
| Death: per-family order of sound, blood emitter, destroy kid, deaths +1, death-only save, GAMEOVER; rely kill in Select role (goto rSelectStage + destroy) | all | `kid::api::kill` + `[death]` (sound ids, gameover offset, `stop_music`, `select_room_reset`) |
| Frozen: blocks input / skips Step (spook) / countdown (wanna) | per game | `misc.frozen` |
| Bullet params (speed 16, life, max, spawn offset, block checks) | per game | `[bullet]` fields, built-in `kid_bullet` |
| Play timer in kid Alarm 0 every room_speed ticks outside menu roles | A | `events.alarms` action `play_timer` (counts into `RunGlobals::time`) |
| B play timer +2 per tick in world Step, +2 on R | B | world built-in param `timer = "b_centis"` |
| Machinary depth layers and scale | A4 machinary | `misc.scale_layers` (tier 3) |
| Multi-kid rooms (eat-the-lemon) | B lemon | several kids; "the kid" = `World::first_of` (R21); killPlayer destroys all; saveGame stores the first |
| Param zones | rainbow, showdown | `kid::api::apply_param_zone` called by built-in `kid_param_zone` |

### Tricks switches

Only SWITCHABLE tricks get switches; EMERGENT ones must fall out of the physics and become test cases.

| Trick id | Switch | Default for the 20 games |
|---|---|---|
| `trick.ad_align` | `misc.ad_nudge` | on in animus, easy-miku-2; off elsewhere |
| `input.dual_shift` | `sim::input` per-key edges (built) | on |
| `input.cactus_phantom_release` | `.inputs` and replay allow a release without press | off until UNKNOWN resolves |
| `input.numpad_cancel` | none: no OS key layer in the client (INP-063, Inferred) | off |
| `trick.disable_cancels` | `jump.cancel` | edge (both B games) |
| `trick.coyote_time`, `trick.jump_buffer` | none (no game in the set has them) | off |
| `bug.right_vine_miss` | `vine.model` | asym in A and B, sym in C |
| `bug.vine_on_ground` | none until specified | — |
| `bug.water_full_jump_1f`, `bug.water1_low_check` | not modeled (not seen in A, C) | off |
| `trick.restart_glitch` | `input.walk = "direct_latch"`, world param `latch_cleared_by_world` | latch on; cleared in kaenbyou, lemon, xia (Inferred) |
| `input.sub_frame_tap` | same latch plus `input.tap_fix` | latch on; xia's `tap_fix` is **research only**: it has no physics field and the loader rejects it, so it is not a promised profile value (#1155) |
| `trick.edge_kill_timing` | `events.boundary` | per family |
| `trick.killer_check_timing` | killer action placement | per family |
| `trick.vdiet` | `NumRules::float_model` (no binary32) | off |

`game.toml [tricks]` keeps the labels for game select; the sim reads only the fields above. The loader checks that a trick marked `required` has its switch on.
