// The boss stress package (#991): HP with a phase change, a kid-bullet collider, alarms and a
// repeating timeline pattern.
//
// Original and deterministic: no RNG. A volley aims at the kid's position on the tick it runs,
// and the inputs decide that position.

const RING = 10;
const SHOT_SPEED = 2.0;
const AIM_EVERY = 20;
const PHASE_HP = 4.0;

fn create() {
    this.hp = PHASE_HP;
    this.phase = 1.0;
    this.set_alarm(0, AIM_EVERY);
}

// Timeline moments. `wind_up` opens the fight; `ring` and `spiral` fire a fan of native
// killer shots, so the timeline costs no per-step script.
fn moment_wind_up() {
    this.phase = 1.0;
}

fn moment_ring() {
    volley(this, 0.0);
}

fn moment_spiral() {
    volley(this, 45.0);
}

// One fan of RING shots. A shot aims at the kid when there is one (the fan spreads around that
// bearing), else it falls in a plain arc.
fn volley(host, turn) {
    let k = kid();
    let i = 0.0;
    while i < RING {
        let d = 90.0 + turn + i * (180.0 / RING);
        if k != () {
            d = point_direction(host.x, host.y, k.x, k.y) + (i - RING / 2.0) * 6.0 * host.phase;
        }
        let b = instance_create("@boss_shot", host.x, host.y);
        b.speed = SHOT_SPEED;
        b.direction = d;
        i += 1.0;
    }
}

// A single aimed shot every AIM_EVERY ticks: the alarm re-arms itself, the pattern the
// timeline drives runs alongside it.
fn alarm(n) {
    if n == 0 {
        let k = kid();
        if k != () {
            let b = instance_create("@boss_shot", this.x, this.y);
            b.speed = SHOT_SPEED;
            b.direction = point_direction(this.x, this.y, k.x, k.y);
        }
        this.set_alarm(0, AIM_EVERY);
    }
}

// Kid bullets are the boss's damage. A hit spends one HP and destroys the bullet; when the bar
// empties the fight moves to its second room, which the timing chain records as a split.
fn collide(other) {
    if other.kind == "kid_bullet" {
        this.hp -= 1.0;
        destroy(other);
        if gm_le(this.hp, 0.0) {
            emit("phase_one_clear");
            room_goto("phase_two");
        }
    }
}
