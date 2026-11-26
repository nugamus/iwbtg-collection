# `bullet_blocker`

Destroys kid bullets that touch it. Source: `crates/sim/src/builtins/bullet_blocker.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `except_when_overlapping_kid` | bool | false | Does nothing while it overlaps a kid (WAN bulletBlocker). Format-0 legacy alias (`except_when_overlapping_kid`), moves in [#('wanna', 1269)](https://github.com/nugamus/iwbtg-collection/issues/('wanna', 1269)). |

**Loading.** The param loads on an object file or a placed instance and must be true or false (tests `wp6b3_params_*` in `crates/format/tests/load_objects.rs`). Converter request CR17.
