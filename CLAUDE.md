# IWBTG Collection: client

@README.md

## Rules

- Never commit original game binaries, decompiled code or extracted files. Those stay in the research repo (`../research`). Ported games live in the games repo (`../games`), rebuilt in our format from clean-room specs.
- Clean room: GameMaker behavior comes only from `research/specs/` (CLONE_SPECs) and the traces there. Never read another runner's or tool's source (OpenGMK, Jtool, renex, decompiled games) while writing client code. If the spec does not answer a question, mark it OPEN and request a spec item or trace.
- The simulation is deterministic: fixed tick at the game's original rate, no frame time in `sim`, number handling matching the original as its CLONE_SPEC documents. Timing, splits and leaderboards count ticks, never rendered frames.
- `sim` must not depend on Bevy.
- Game packages are readable text and never contain compiled code. Custom behavior goes only through our sandboxed script language. See `docs/format/README.md`.
- Each game keeps its own physics profile. Never "fix" a quirk or bug players rely on.
- Input is a fixed set of client actions (left, right, jump, shoot, restart, ...). Games map actions; they never read raw keys.
- Design is a priority. No generic UI; match the IWBTG style and reuse art from ported games where it fits.
- The user does not write Rust and tests every change. End each change with: exact build/run commands, steps to perform, and what to look for (expected vs. broken).

## Maintaining this harness

Any agent may edit this file and `.claude/`. Keep it lean: add a line only when it would have prevented a real mistake, and delete lines that go stale.
