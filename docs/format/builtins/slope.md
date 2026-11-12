# `slope`

Solid with a precise mask. No events: the climb and snap rules are the kid's (the `[slope]` model of the physics profile; KID-A1 POR and RBF, KID-A4 NY yaneH and yaneM, KID-B obj_wall). Source: `crates/sim/src/builtins/slope.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `shape` | `rise_right`, `rise_left`, `precise_sprite` | `rise_right` | Which way the mask rises, which picks the class: `rise_left` is `slope_mirror`, the other two `slope`. The mask is always the kind's sprite mask (`precise_sprite` only says so). |
| `scale_x` | real | 1 | A negative value mirrors the shape's class (`slope` and `slope_mirror` swap). It does not flip the mask or the sprite. |

**Class.** The kid tells the two directions apart by class: `slope` rises to the right, `slope_mirror` to the left (`[slope]` in `docs/format/physics.md`). In every original with a mirrored slope, the mirror is its own object with its own sprite already drawn rising left, at `image_xscale` 1: eat-the-new-year yaneM (kid/A4.md: "yaneM rises to the left (from the sprite images)") and nanameblockR in rainbow, rori and fairy-tale (kid/A1.md: "nanameblockR (rises to the left) ... precise 45 degree 32x32 masks"). So the loader picks the class and keeps the sprite's mask; flipping the mask too would turn yaneM's mask back to rising right. The converter writes yaneM as `scale_x = -1` (CR10). `objects::param_classes` applies both params on the kind; tests `objects::tests::slope_shape_and_scale_x_pick_the_class` and `wp7a_load::slope_shape_and_scale_x_pick_the_kind_class`.

**Not covered.**
- Poronkusema's nanameblock is one object whose instances get `image_xscale` -2, -1 or 2 from creation code (kid/A1.md POR); that is the instance's own scale (it does scale the mask, GM rule), its kid has one event for all of them, so all stay class `slope`. Write it as the room instance `xscale`, not `scale_x`.
- Family B `obj_wall` (kid/B.md) is one class for both directions, `slope_wall`, read by the End Step `slope_b` code; `shape` does not pick it. A kind gets it with `counts_as = ["slope_wall"]` (it keeps class `slope` too, which no B row reads).
- **OPEN:** `shape` and `scale_x` on a placed instance load and are checked, but the class is the kind's, like water `variant` (instance classes come from the kind; #659). Give the kind (object file) the param.

**Loading.** Both params load on an object file or a placed instance and are checked (`params::check`; tests `wp6b3_params_*` in `crates/format/tests/wp7a_load.rs`). Converter request CR17.
