# `bullet_blocker`

Destroys kid bullets that touch it. Source: `crates/sim/src/builtins/bullet_blocker.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `except_when_overlapping_kid` | bool | false | Does nothing while it overlaps a kid (WAN bulletBlocker). |

**Loading.** The param loads on an object file or a placed instance and must be true or false (tests `wp6b3_params_*` in `crates/format/tests/wp7a_load.rs`). Converter request CR17.
