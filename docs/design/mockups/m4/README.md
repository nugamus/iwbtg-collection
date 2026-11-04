# M4 mockups: game select, loading, HUD, pause, results (draft)

Draft for #478, second round, built from the user's feedback on PR #480 (2026-11-04). Live canvas: https://claude.ai/artifact/QbvtFDFHrx3HhRmuTGTM9w. The source files are in this folder: `canvas.json` places the artboards, each `.dc.html` is one artboard.

Direction: modernise IWBTG without losing its identity. Structure, motion and behaviour follow osu!lazer (MIT); look comes from IWBTG and fangame art. No pixel fonts for UI text (only inside the game-space pause box), no bordered boxes; spacing, type and a restrained accent carry the hierarchy.

| File | Artboard |
|---|---|
| `Main.dc.html` | Game select 16:9, Kaenbyou expanded to its entries |
| `SelectNoClears.dc.html` | Game select, an entry with no clears |
| `Loading.dc.html`, `LoadingUnranked.dc.html` | Loading card, ranked and unranked |
| `Hud169.dc.html`, `Hud169Local.dc.html` | HUD 16:9, Global and Local leaderboard |
| `Hud43.dc.html`, `Hud219.dc.html`, `HudSmall.dc.html` | HUD at 4:3, 21:9 and a 960x540 window |
| `LbStates.dc.html` | Live leaderboard at #1 / around #25 with a pace flash / below the top 100, no clears, offline |
| `Pause169.dc.html`, `Pause43.dc.html` | Pause B (the in-game PAUSE box) at 16:9 and 4:3 |
| `Results169.dc.html`, `Results169Slower.dc.html` | Results 16:9: new PB, and slower than PB |
| `Results43.dc.html`, `Results43Slower.dc.html` | Results 4:3, same two states |
| `Backdrop.dc.html` | The one background system (art, blur slider, dim) every screen uses |
| `Leaderboard.dc.html`, `RunInfo.dc.html`, `PauseBox.dc.html`, `GameScene.dc.html` | Shared parts the screens import |

## Shared design language

- Rows, panels, pills and buttons: 0.2 horizontal shear (`skewX(-11.31deg)`), text kept upright, 10 px corner radius.
- Motion: OutQuint over 300–500 ms for most transitions, OutElastic for small pops. Selected list panel slides 25 px left (400 ms); header slides in from the left (600 ms); leaderboard rows reorder with a 450 ms slide; your row widens 20 px with a pop; pace flash holds 3 s.
- Type: Barlow and Barlow Condensed, tabular numerals everywhere. Pixelify Sans only in the pause box.
- Colour: near-black ground; cherry red accent (`#ff3d55`) for the primary action and selection marks; green `−` ahead, red `+` behind, gold `★` best segment (signs and marks as well as colour); amber for Unranked and Offline.
- Game viewport: never stretched. At 16:9 it fills the window height; at 4:3 it shrinks so both panels keep their room; past 21:9 the panels stop at 360 px and the rest is background art.

## Data and art

Titles and creators come from our port list; every other number, player name and avatar is sample data. All art is placeholder except the delicious fruit, which is the real sprite, uploaded to the canvas as an asset (`/_blob/…` links) and not committed here.
