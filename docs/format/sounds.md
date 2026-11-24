# Sounds and music sidecars

Part of [format v0](v0.md).

```toml
#:schema ../../../client/docs/format/schema/v0/sound.schema.json
length_ticks = 9792   # song length in seconds times the tick rate
volume = 1
pan = 0
```

`sounds/<id>.ogg` is a normal sound; `music/<id>.ogg` is MULTIMEDIA (at most one copy plays). A sound id and a music id may not be the same. The optional sidecar `<folder>/<id>.toml` holds `length_ticks` (the song length in ticks, which `sound_isplaying` needs; converter request CR9), `volume` (default 1), `pan` (default 0) and `kind` (`normal` or `multimedia`, to override the folder). Without `length_ticks` the length is 0 ("unknown") and the load warns (CR9): the sim then treats the sound as playing until it is stopped, so `sound_isplaying` stays true after a one-shot that the original would have finished (#1173). The client still plays the Ogg once to its end. Sound ids are numbered sounds first, then music, each in file path order.

**Files a game loads through a DLL** (SuperSound `SS_LoadSound`, GMFMODSimple `FMODSoundAdd`: kaenbyou, eat-the-lemon, go-the-spook-house, xia) get their length from the same sidecars. The DLLs never report a length, so when the script host calls an adapter's load function it passes `GameDef::dll_sound_length(file)`: the file name's stem (after the last `/` or `\`, without the extension, case ignored) names a `sounds/` or `music/` id, and that id's `length_ticks` is the length. A file that names no package sound gets 0, "unknown": a one-shot DLL sound then plays until stopped, which differs from the original for `SS_IsSoundPlaying` and for FMOD instance validity. So a converter puts every file a game loads by path into `sounds/` or `music/` (as Ogg) with a `length_ticks` sidecar, and the game's DLL calls name it by that stem (converter request CR9, which now covers DLL-loaded loose files too).
