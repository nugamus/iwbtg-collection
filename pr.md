## Closes
Closes #451

## Docs updated
- docs/format/builtins/gravity_flip.md (the `yflag_with_view_rotation` rule, tests, OPEN note)

## Follow-up issues
- nugamus/iwbtg-collection-research#92: the converter does not write `transition = "yflag_with_view_rotation"` on the kaenbyou arrows, so famb-gravflip still DIFFs on the real package. On a scratch copy of the package with the param added it is OK (touch f39, angle 180 on f54, flip f55).

## User check
Run in `client/`:

    cargo test -p iwbtg-sim --test wp6b_drop_flip_edge_blocker_slope kaenbyou
    cargo test -p iwbtg-format --test wp7a_load view_rotation

Output (run in the worktree): `test wp6b::kaenbyou_arrow_rotates_the_view_then_flips_the_kid ... ok`, `test result: ok. 1 passed`. Broken: any FAILED line.
