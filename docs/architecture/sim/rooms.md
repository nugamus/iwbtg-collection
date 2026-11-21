# The simulation: Rooms

Part of [the simulation](../sim.md).

## Rooms

Room data, the room change sequence, restart and reset, saves and views. This is the live text of the [sim core plan](../../plans/sim-core.md) sections 4.1-4.5, which stay only as history. Spec: CLONE_SPEC "rooms" (Confirmed change requests, room start sequence, persistence, views, game_restart, saves; Inferred items; UNKNOWN list), mechanics.md "camera", "save", "warp", "edge_warp".

### Room data and loading

```rust
// def/room.rs
pub struct RoomDef {
    pub name: String, pub width: u32, pub height: u32,
    pub role: RoomRole,                     // Boot | Title | Menu | Select | Play
    pub file_speed: u32,                    // room_speed on entry, before world Room Start (Inferred, R14); legacy clock data under ClockPolicy::RoomSpeed (R30)
    pub persistent: bool,
    pub views_enabled: bool, pub views: [ViewDef; 8],
    pub instances: Vec<PlacedInstance>,     // file order = creation order (Inferred, rooms: "created in the order they are listed")
    pub on_create: Vec<RoomAction>,         // room creation code, as data
}
pub struct PlacedInstance { pub kind: KindId, pub rank: u16, pub x: Real, pub y: Real,
    pub xscale: Real, pub yscale: Real, pub angle: Real, pub params: Params,
    pub name: Option<NameId>, pub group: Option<GroupId> }
```

`RoomRole` replaces every "not rInit/rMenu/rTitle/rSelectStage" test in the engines: kid Alarm 0 play timer, bow self-destroy, the R handler's "unless menu room", B Create save, spook saveExe skip, rely's killPlayer special case in rSelectStage. The loader never sorts `instances`.

### Room change sequence

```rust
pub enum RoomRequest { Goto(RoomId), Next, Previous, Restart, GameRestart, GameEnd }
pub enum StartPoint {
    NewGame { difficulty: u8 },           // what warpStart does: delete save, set difficulty, go to GameDef::new_game_room
    Room { room: RoomId, difficulty: u8 }, // trace cases: the recorder's --room jump
    FromSave(SaveSlot),
}
pub fn start_game(ctx: &mut Ctx, start: &StartPoint) -> Result<(), SimError>;  // boot instances, first room, Game Start
pub fn perform_change(ctx: &mut Ctx, req: RoomRequest) -> Result<(), SimError>;
```

- `start_game` first creates the persistent instances listed in `GameDef::boot` (the `world` built-in, Design: it stands in for rInit), then enters the start room. Title, menu and select rooms are client UI for now (Design, CR7); a run enters gameplay through `StartPoint::NewGame`.
- `perform_change` does the manual's 7 steps (Confirmed):
  1. Room End for every instance (dispatch order).
  2. Remove non-persistent instances, no Destroy event. Deactivated persistent instances count as non-persistent here: they do not move to the next room (Confirmed, manual 403_06). If the old room is persistent, store its instances in `persistent_rooms[old]` first.
  3. Keep persistent instances (ids unchanged).
  4. If the new room is not persistent or not visited: create placed instances in file order; for each, apply its params (instance creation code) then run Create. A placed instance whose GM8 id is already alive (a carried persistent one, R29) is skipped, and so is one destroyed earlier in the same tick, which is still pending removal (#670): the retry records the ids it destroyed on the R tick (`RoomRequest::GotoAfterRemovals`) so the room it re-enters does not create those placed kids again. If it is a restored persistent room, put its saved instances back without Create (Inferred, R18).
  5. First room only: Game Start for every instance (no game in the 20 uses it, Confirmed).
  6. Room creation actions (`on_create`).
  7. Room Start for every instance in dispatch order (snapshot; R10).
- **One engine-owned transaction (#1174 A2/A2b).** The steps run as one `room::RoomTransaction`, in the stage order `EnterRoomPolicy::stages` declares (`def/game.rs`). `gm8`'s order is `GM8_ROOM_STAGES`: `RoomEnd`, `Prepare`, `Commit`, `Placed`, `GameStart`, `CreationCode`, `RoomStart`. `Prepare` only reads the old and new room and stages the carried instance set; `Commit` is the transaction's single commit point and the only stage that changes the instance store and the room state (it also emits `RoomOutput::Entered`); the build stages then create and start the new room. The old steps 2 and 3 are split across `Prepare` and `Commit` without changing what a handler observes: Room End still runs first on the old room, Create still sees the new room, and Room Start still takes its snapshot after the build. A request raised by an event or creation action is taken out of `World::pending` right after that event. `EnterRoomPolicy::nested_requests` names the rule (R13/R23): `LastWins` is the gm8 behavior (one slot, the last request, chained or deferred by `after_sequence`); `Queue` appends each request and `after_sequence` runs them in raise order. The queue lives for the tick's chain only, so the `NextTick` rule keeps the one-slot behavior (the world store is identity-save's).
- Also: `room_speed = file_speed` at entry (Inferred, R14), views reset from `ViewDef`, transition reset (visual only, `RoomOutput::Transition`; transitions spend no ticks, Inferred R17b). `Restart` and `Goto(current)` rebuild a non-persistent room (Inferred). Persistent room `Restart` is out of scope (Confirmed unused).
- **Requests raised during the sequence** (R23, rooms UNKNOWN). kaenbyou and eat-the-lemon call `loadGame` (room_goto) from rLoad's room creation code (Confirmed that they do; what follows is UNKNOWN: do Room Start events still run, is the room drawn, is a tick spent). Switch `RoomRules::nested_request`: `ChainAfterSequence` (Design default: finish all 7 steps, then run `perform_change` again before the next tick) or `NextTick` (rLoad gets one tick). A loop bound of 8 chained changes per tick, then `SimError`. `kaenbyou/tr-room-12` is recorded and settles it.
- **`instance_destroy()` in room creation code** (R24, rooms UNKNOWN: which instance is "self"). Showdown's stage rooms call it. Confirmed (recorder): it destroys nothing and the rest of the code runs (showdown `a3-t18-a` rows 42-43, `a3-t20`, `tr-room-14`), so `RoomAction::DestroySelf` is a no-op. The transaction gives the action an explicit context (the room and the placed instances it built), because room creation code belongs to the room, not to an instance: there is no calling instance to destroy. A standard model that replaces the bare `instance_destroy()` with an explicit target uses that context once A1's identity predicate lands (#1174).

### Restart and game_restart

- `GameRestart` (Inferred): remove all instances (persistent too), forget persistent room states, reset globals per `RoomRules::globals_survive_restart` (UNKNOWN, R16; `little-runner/tr-room-04` recorded), start again. The room it enters is the **Boot-role room** (`room::restart_room`), the loader's engine-init marker; a profile with no Boot room keeps the gm8 shape where that is room order index 0. The destination is never a save identity, and `room_goto_next`/`previous` skip the Boot room. Sounds keep playing (Inferred; S4).
- **One lifecycle.** The sim has one lifecycle (decision 0005): `new_game`, `enter_room`, `death`, `retry` and `quit`, plus the F2 `reset` policy. `GameDef::lifecycle()` is the engine's view; the retry and reset policies are the stored profile data (`[engine] restart`/`[engine] reset`; `RestartModel`/`ResetModel` remain their format names, aliases of `RetryPolicy`/`ResetPolicy`). The `world` built-in runs the retry and reset **at the world's rank**, in the phase the policy's trigger names:

  ```rust
  // def/game.rs
  pub struct Lifecycle { pub new_game: NewGamePolicy, pub enter_room: EnterRoomPolicy,
      pub death: DeathPolicy, pub retry: RetryPolicy, pub quit: QuitPolicy, pub reset: ResetPolicy }
  pub struct RetryPolicy { pub trigger: RestartTrigger, pub kind: RestartKind, pub excluded_roles: RoleSet,
      pub boot_ticks: u8 /* retry_delay_ticks */, pub flipped_load_dy: Real /* +4 */,
      pub death_on_restart: DeathOnRestart,
      pub kid_steps: RetryKidSteps /* B: add_time 2, save_death_time, clear_frozen; showdown: clear_frozen, reset_jump_values (#224) */,
      pub music: Persistence, pub effects: Persistence }
  pub enum RestartTrigger { KeyPressEvent, StepHeld, StepPressed { skip_while_paused: bool } }
  pub enum RestartKind { GameRestartViaBoot, RoomGotoSaved, ShowdownLoad, RecreateInPlace }
  pub enum Spawn { RoomStart, Checkpoint, KidStart }
  pub enum ResetScope { Room, Save, Run }
  ```

  A retry's player-observable policy is: reset scope `save` (return to the checkpoint's room, position and counters), spawn `checkpoint` when a checkpoint exists (else `room_start`), control after `retry_delay_ticks`, music and effects kept, and the checkpoint write `death_on_restart` declares. A new game resets scope `run`, spawns `room_start` and deletes the save. Death sets the dead state and counts; it is not a transition. `quit` is client-only and has no sim state.

  | Kind | Trigger | Games | Steps (spec source) |
  |---|---|---|---|
  | `GameRestartViaBoot` | `KeyPressEvent` (keyboard phase); wanna `StepHeld` (world Step, every tick while held, A3 I7) | Groups A (minus showdown), B, C: 15 games | Optional death-only save (little-runner, wanna saveDeathTime); create kid at (0,0) in the old room if none; set the requested room and position (saveExe); game_restart (last request wins, R13); `boot_ticks` ticks in a boot room with no gameplay instances; then the saved room at the saved position, plus `flipped_load_dy` when flipped (10 Group A games: rely and 9 others; not rori, fairy-tale, showdown). Rooms "game_restart" Confirmed list; timeline N, N+1, N+2 Inferred (TR-ROOM-01; `rely-the-needle-4-2/tr-room-01-a` recorded). |
  | `RoomGotoSaved` | kaenbyou, eat-the-lemon `KeyPressEvent`; go-the-spook-house `StepPressed` (world Step, rooms Confirmed) | 3 | Destroy kid and bow, create kid, `Goto(saved room)`, set position (rooms Confirmed). B adds: +2 to the play timer, saveDeathTime, clear frozen; the new kid's Create sees the stale `reverse` (kid/B.md R and Death). Spook's saveExe skips the goto in Title/Menu roles. |
  | `ShowdownLoad` | `KeyPressEvent` | showdown | kid/A3.md showdown "Restart": clear frozen on both kid kinds, add a death and saveDeathTime and destroy each, loadGame (create kid at (0,0), reset jump values and the listed globals), write temp, saveExe, loadDeathTime; **no game_restart**. |
  | `RecreateInPlace` | `StepPressed { skip_while_paused: true }` | keep-celebrating-for-xia | Destroy kid, create kid at the saved spot in the current room, then `Goto(saved room)` (rooms Confirmed). |

- **R26 (settled).** Showdown's R has no game_restart (research PR #98; recorder `traces/showdown/a3-t18-a.csv` f41-43). The rooms section counts 15 of the 20 games with a full game restart.
- **The init phase is engine state.** A retry's delay is `RetryPolicy::boot_ticks` (the serialized format-0 name; `retry_delay_ticks` in decision 0005) and is run by `restart::InitPhase`: the R tick begins it, `boot_step` counts it down, and the tick that reaches zero loads the checkpoint. It is not a package-authored room and not a temp file. The gm8 compatibility profile still enters the package Boot room while the phase runs because an old replay's state hash observes that room; the phase itself does not depend on it. `standard` removes the package room once the stable room key lands (#1153, follow-up of #1149). A delay of 0 is permanently `Unsupported`: no game in the 20 sets it, so whether the init tick exists at all is UNKNOWN (R17; #225).
- `room_restart` (kaenbyou warpStart, xia death in difficulty room) is `RoomRequest::Restart`.
- `Reset` (F2): Groups A, B, C (and go-the-spook-house) delete the save's temp marker, stop every sound and game_restart (CLONE_SPEC rooms; kid/A4.md "F2 pressed"); kaenbyou/lemon gameReset resets globals and goes to the Boot-role room (rooms Confirmed), with no recorded sound stop and an OPEN global list (research trace request nugamus/iwbtg-collection-research#270, #225). The policy is `Lifecycle::reset` (`ResetPolicy`, format name `ResetModel`); its reset scope is `room` for the temp-marker restart and `run` for gameReset, and `ResetPolicy::sounds()` is `stop` for the former and `keep` for the latter.

### Saves

The sim never touches files. The save is sim state; the client stores its bytes ([client storage](../client/storage.md)).

```rust
// `flipped` is `global.reverse` for a `yflag` game (family B), the kid's flip state otherwise (#267).
pub struct SaveSlot { pub room: RoomId, pub x: Real, pub y: Real, pub flipped: bool, pub facing: i8,
    pub difficulty: u8, pub deaths: u32, pub time: u32, pub bosses: u16, pub items: u16, pub clear: bool }
pub enum SavedPosition { XyBase100Bytes, XyBase256Bytes, XyText2Dec, XyPrevious, FixedPoint, FloorNudge }
pub fn save_now(ctx: &mut Ctx, kid: InstId, model: SavedPosition, fixed: Option<(Real, Real)>) -> Result<(), SimError>;
pub fn save_counters(ctx: &mut Ctx) -> Result<(), SimError>;        // death-only save
pub fn quantize(x: Real, y: Real, model: SavedPosition, r: &NumRules) -> (Real, Real);
pub fn load_into(ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError>;
```

- `quantize` reproduces what the original's save format does to a position (rooms Confirmed group table): base-100 bytes (13 games; last byte through `byte_from_real`, N5), base-256 div/mod bytes (little-runner, N8), `string()` two decimals (machinary, eat-the-new-year, N6), xprevious/yprevious exact (kaenbyou, eat-the-lemon, N7), fixed point (poronkusema), floor with the three-step +1 nudge using `place_free` (xia). This decides the respawn v-align (tricks `trick.respawn_valign`). Recorded and waiting for the spec: `kaenbyou/tr-room-08` (N7), `little-runner/num-divmod-save-*`, `eat-the-new-year/num-string-save-*`, `machinary/tr-room-07-a`, `little-runner/tr-room-06`.
- Death-only saves update counters only. `deaths` and `time` use `checked_add` (10.3).
- `World.save = None` means "save file missing"; the kid Create's first-play save (rely Create step 4, B Create step 4) tests it.
- The client persists the slot inside its versioned envelope (package id, built-in revision, saved room name; [client storage](../client/storage.md)); `StartPoint::FromSave` restores it.

### Views and camera

- `View { enabled, visible, x, y, w, h, port, follow: Option<Target>, hborder, vborder, hspeed, vspeed, angle }` per room from the room file. `view_xview` without index is view 0 (Inferred).
- Runner follow (eat-the-lemon, animus): `view::follow_update`, once per tick before the Draw slot; follows `World::first_of` of the target (R21) (Inferred algorithm; check order and phase UNKNOWN, R19; `animus/tr-room-09-*`, `eat-the-lemon/tr-room-09-*` recorded). Camera objects are the `camera` built-in with a `mode` parameter; their Step placement gives the 1-tick lag the spec infers (`little-runner/tr-room-11` recorded).
- Views affect gameplay through `view_xview` reads (GAMEOVER position, camera-relative spawns), so they are hashed sim state.
