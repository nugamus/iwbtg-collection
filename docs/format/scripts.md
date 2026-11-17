# Scripts: `scripts/<id>.rhai`

Part of [format v0](v0.md).

Scripts are Rhai under the IWS profile ([0001](../decisions/0001-script-language.md)). One file per scripted object (or shared by several objects of one game). Only `fn` and `const` items at top level. A `const` whose value is a literal (a number, a string, `true` or `false`, a number with a minus sign) is visible inside every function below it, as if the literal were written there; a const whose value is an expression works at the top level only, a const must be declared above the function that uses it, and its name cannot be reused for a variable or a parameter. The API is in [architecture](../architecture/sim/scripts.md#script-api-first-version); the generated reference will be `docs/format/script-api.md`.

```rhai
// scripts/boss1.rhai
const SHOT_EVERY = 25;

fn create() {
    this.hp = 20.0;
    this.set_alarm(0, 50);
}

fn alarm(n) {
    if n == 0 {
        for i in 0..12 {
            instance_create("@orb", this.x, this.y, #{ speed: 4.0, direction: i * 30.0 });
        }
        this.set_alarm(0, SHOT_EVERY);
    }
}

fn collide(other) {
    if other.kind == "kid_bullet" {
        this.hp -= 1.0;
        destroy(other);
        if gm_le(this.hp, 0.0) {
            emit("boss1_dead");
            destroy(this);
        }
    }
}
```

`draw(d)` is a valid event function (client only), but the draw builder `d` has no methods yet, so the example does not use it.

Limits: at most 256 KB per script file.
