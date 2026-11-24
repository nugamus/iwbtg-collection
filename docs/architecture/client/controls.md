# The client: controls panel

Part of [the client](../client.md).

`controls.rs` (#52; docs/content.md, Controls): a searchable picker of the games in a folder, the
picked game's actions with the labels its package gives the four extra actions, and rebinding of
the global bindings. `iwbtg-client --controls <folder> [--screenshot <png>]` opens it alone; a
settings screen or the pause menu will host the same module later (the entry point is a one-line
call, and until then the flags are how it is reached).

## The list

- The games are the packages [`shell::select::packages`](shell.md) finds in the folder (folders
  with a `game.toml`, and `.iwbz` files), read for `game.toml` only, in title order. A package
  whose `game.toml` does not read is left out: the client cannot play it, so there is nothing to
  bind for it.
- The search field is the kit's (`ui/widgets.rs::search_field`, focused when the panel opens).
  Every word typed must appear in the game's title or its creator, any case, as in game select.
- Up and Down walk the list; the mouse wheel scrolls it; a row is clipped by the list's box, so a
  long library scrolls inside the column and the pick stays visible. A row shows the title, the
  creator and, for a game with `[controls]` labels, how many extra actions it uses.

## The picked game

The right side names the game, then the line `extra_actions_notice` builds: each `[controls]`
label of `extra1` to `extra4` with the key bound to it now, or `unbound` (docs/format/game.md,
Controls). A game without labels says that the standard set covers it.

Two columns of rows follow:

- **GAME ACTIONS**: the engine actions every game reads (`left` through `reset`), then one row per
  extra action the game labels, tagged `EXTRA` and named with the game's own words ("Teleport").
  An extra action the game does not label is not shown, as in the HUD's input display (#831): the
  panel never invents a meaning for `extra2`.
- **CLIENT KEYS**: `quick-retry`, `skip-cutscene` and `frame-advance`, the keys the client handles
  itself (they never enter an `InputFrame`, [view](view.md)).

A row lists the keys and controller inputs bound to its action as pills, `unbound` when there are
none, and a `CHANGED` tag when the binding differs from the default. The panel reads bindings
through `bindings.rs`'s accessors (`keys_of`, `pads_of`, `client_keys_of`, `client_pads_of`,
`is_default_action`, `is_default_client`).

## Rebinding

A click, Enter or Space on a row starts listening for it; the next key press binds that key to
the action and Escape cancels. A press on the row itself (Enter, Space) does not count, so the
activation never binds the key that started it. Only keys are rebound here, not controller
inputs: a pad button cannot be held down and read as a binding the way a key press can, and the
defaults cover the pads (a `--bind action=pad:<name>` still reaches them).

The rebind goes through `bindings::Config::set`, the same path as `--bind`: an unknown key, the
pause key or button, or a key already bound to something else is refused. A refusal keeps the old
binding and its message is shown above the columns; a success replaces that action's keys and
writes the settings' `[bindings]` table ([Settings](settings.md)), so the change is the player's
from the next run on. A `--screenshot` run starts from the default bindings, saves nothing, and
is the same picture on every machine.

## Checks

`--screenshot` works as for the gallery; the panel lays out at 1400×900 design px and is scaled
to the window. Two hidden flags exist for screenshot checks, in the style of the HUD's
`--hud-preview-*`: `--controls-pick <N>` picks a row of the list at start (0 is the first title)
and `--controls-search <TEXT>` starts with that search text in the field. `controls::tests`
covers the search, the label and notice text, a fixture package's labels, and that a refused
rebind keeps the old binding.

The notice itself, posted before play, is not wired yet: it needs a notice kind for extra actions
(`ui/notice.rs`) and a call where a run starts (`view.rs` or `screen.rs`), neither of which this
change owns. `extra_actions_notice` is public for that call.
