## Closes
Part of #141 (does not close it: kaenbyou is not all OK yet; the remaining causes are filed below).

## Docs updated
- docs/architecture.md (the case `latch` key); the key is also documented in the header of crates/tools/src/case.rs.

## Follow-up issues
- research#86 converter views (famb-water-a/b/c, trick-water-jump-b), research#88 save point `saved_position`, research#89 game_over preset, research#90 bloodEmitter lifetime
- #450 restart row sampling, #451 gravity_flip view rotation, #452 boss sample and god_mode, #453 tr-room-12 boot start, #454 deathfade-a stub volume

Changes: `latch` case key (recorder `--gml` reads of the decoded save file) with a unit test; cases fixed for the current converter output (famb-save-restart-a/b, tr-room-08, trick-restart-glitch-b-a/b map the save point kinds; famb-death-timing-b maps blood and GAMEOVER columns and is OPEN until research#89 and #90). Checked on a scratch package with the converter fixes applied: 32 -> 36+ OK, with famb-save-restart-a and famb-death-timing-b OK, water cases OK, the rest differ only on the causes above.

## User check
Run in `client/`:

    cargo test -p iwbtg-tools latch
    IWBTG_RESEARCH=<research> cargo run -q -p iwbtg-tools --bin iwbtg -- trace check <research>/build/packages/kaenbyou

Expected: `test trace_cmd::bundle_tests::latch_holds_the_value_before_the_last_read ... ok`. Against the unfixed converter output the check ends `32 OK, 4 OPEN, 11 failed, of 47`; each failure is one of the issues above. Broken: the latch test FAILED.
