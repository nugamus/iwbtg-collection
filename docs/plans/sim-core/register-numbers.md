# Sim core plan: 15. register, Numbers

Part of [the sim core plan](../sim-core.md), section 15 (the intro and the column meanings are there).

### Numbers

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| N1 | `==`, `<` exact or epsilon | Confirmed (recorder): tolerance t with 9e-14 < t <= 1e-13; Confirmed (real runner, research#8): the real runner uses a tolerance too (rely's alpha stays 0.9999999999999999), its exact t is unmeasured and stays OPEN; code value from OpenGMK | `NumRules::compare` | `Epsilon(1e-13)` | num-eps-slip-b/-c (rec), num-eps-alpha (rec), rely real-alpha-eps (real runner, presence only) |
| N2 | Truthiness `> 0.5` vs `>= 0.5` | Spec infers `> 0.5`; code value from OpenGMK | `NumRules::truth` | `GeHalf` | num-eps-* |
| N3 | FPU precision (53, 64 or 24 bit) | Confirmed (real runner, research#8): 53-bit, round to nearest (control word 0x1272), so binary64; a jump's 42 y values equal the recorder's (CLONE_SPEC, Number handling) | `NumRules::float_model` from `[engine]` | f64 (no x87 fallback needed) | num-fpu-jump, rely real-key-repeat (real runner) |
| N4 | round tie rule; round(−x) to −0 or +0 | Inferred ties-even; −0 rule from OpenGMK | `NumRules::round_neg_zero` | ties even, `+0` | num traces |
| N5 | file_bin_write_byte of a fraction or out-of-range value | Fractions Confirmed (recorder): round to nearest, ties to even (48.5 to 48); out-of-range UNKNOWN (not traced) | `NumRules::byte_from_real` | `ByteConv::RoundWrap`: round half even, wrap mod 256 | num-byte-save (rec), tr-room-05-e (rec) |
| N6 | `string()` two-decimal rounding | UNKNOWN: exact halves go to even, but half even on the decimal value does not fit every value (183.005 gives 183.00, 0.015 gives 0.01) | `NumRules::string_decimals` | round half even on the decimal value (kept until research#100 finds the rule) | num-string-save-b (rec), machinary tr-room-07-b/-c (rec), research#100 |
| N7 | ds_list_write keeps doubles exactly | Confirmed (recorder) | `save::quantize(XyPrevious)` | exact | kaenbyou tr-room-08 rows 51-52 (rec) |
| N8 | div truncates (Confirmed, code fix); div/mod on fractional operands | Confirmed; fractional positive operands Confirmed (recorder); negative operands not traced | `num::gm_div`, `num::gm_mod` | `trunc(a/b)`; `a - (a div b)*b` on reals | little-runner num-divmod-save (rec) |
| N9 | RNG algorithm, scaling sign, start seed, draws per call | Inferred / UNKNOWN | `rng.rs` | Delphi LCG, unsigned, seed from replay, one draw | num-rng |
| N10 | Who draws from the RNG (blood, playerface, particles) | Confirmed consumers; particles Confirmed (recorder, research#158): CLONE_SPEC numbers "Particle draw rules" (slot after End Step, order, streams, region and type draws, step and death particles), modelled in `sim::particles` (#556); real runner unchecked (research#164); untraced settings `Unsupported` (research#163, #167, #168) | each built-in; `sim::particles`; `NumRules::particles_draw_rng` | draws as specs list; particles `false` until packages and built-ins create systems (#765, #766); state in `World::particles`, update at tick row 11b (#764); room-request tick and room change OPEN (research#170) | num-rng-a (rec, replayed bit for bit), num-part-rng-* (rec), BLOOD-COUNT |
| N11 | `sign` exact or with epsilon | OpenGMK provenance; slip creep depends on it | `NumRules::sign` | `Epsilon` (follows N1) | num-eps-slip (rec) |
| N12 | `min`/`max` tie and signed-zero rules | OpenGMK provenance | `NumRules::min_max_ties` | current code | num traces |
| N13 | `floor`/`ceil` keep signed zero | OpenGMK provenance (round.rs comments) | `NumRules::floor_ceil_zero` | keep sign | num traces |
| N14 | GML literal parse and converter-computed constants equal GM's values | Confirmed (recorder): digits as one integer times the double nearest 10^-k (research#104); 0.4, 0.45, 0.017, 1/15 are nearest doubles, 31 literals (0.3, 0.95, 0.7, ...) one ulp above; Confirmed (real runner, research#8): the real runner parses them the same way | converter writes the recorder double in shortest round-trip form; built-in constants; `num::gml_literal`; WP7b check | recorder model in the converter; hand-written decimals are the nearest double; computed values computed at run time | 172 release cuts in 14 games bit-exact; showdown a3-t19 tempHspeed 3.0 x 0.95 = 2.85 every step |
| N15 | Non-finite values | Design (spec: division by zero out of scope) | every setter | `SimError::NonFinite` | — |
