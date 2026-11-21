# `sign`

A sign that raises a text box (JUN, LT, ANI, FT, XIA). Source: `crates/sim/src/builtins/sign.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `text` | text | required | The message. |
| `mode` | `on_up_press`, `while_overlapping` | `on_up_press` | `on_up_press`: each Step, Up pressed this tick while the sign overlaps a kid (FT object796, ANI kanbanED). `while_overlapping`: every tick a kid touches it (KID-C objSign). |

The sim emits `BuiltinOutput::TextBox { text }` on every tick the sign speaks; showing, wrapping and closing the box is the client's. Nothing reads it back. The client latches the text into a box that stays up for 150 sim ticks (3 s at 50 Hz) after the last tick that raised it: the original's box is a modal `show_message` and the spec does not say how it closes or how long it stays, so the short client-side duration is the declared default (#459).

**Loading.** Both params load on an object file or a placed instance (`text` is free text). Tests in `crates/sim/tests/builtins_sign.rs`.
