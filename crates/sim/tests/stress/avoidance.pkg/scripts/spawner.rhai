// The avoidance stress spawner (#990): rings, spirals and aimed shots, all three every batch.
//
// Each tick it creates at most BATCH bullets: the host caps one event call at 256 spawns, so a
// longer batch would halt the sim. Waves are deterministic: no RNG, only the kid's position at
// the tick a shot is aimed, which the inputs decide.

const BATCH = 250.0;
const SPEED = 2.0;
const GOLDEN = 137.508;
const SPIN = 7.0;

fn create() {
    this.spawned = 0.0;
    this.wave = 0.0;
    this.spin = 0.0;
}

fn step() {
    let left = this.count - this.spawned;
    if left <= 0.0 {
        return;
    }
    let n = min(BATCH, left);
    let third = floor(n / 3.0);
    let k = kid();
    let i = 0.0;
    // Ring: an even spread around the spawner, rotated a little every wave.
    while i < third {
        launch(this, this.spin + i * (360.0 / third));
        i += 1.0;
    }
    // Spiral: the golden angle per bullet, advancing every wave.
    i = 0.0;
    while i < third {
        launch(this, this.spin * 0.5 + i * GOLDEN);
        i += 1.0;
    }
    // Aimed: a fan at the kid, or a straight line when there is none.
    i = 0.0;
    while i < n - third * 2.0 {
        let d = this.spin;
        if k != () {
            d = point_direction(this.x, this.y, k.x, k.y) + (i - (n - third * 2.0) / 2.0) * 0.5;
        }
        launch(this, d);
        i += 1.0;
    }
    this.spawned = this.spawned + n;
    this.wave = this.wave + 1.0;
    this.spin = this.spin + SPIN;
}

fn launch(host, d) {
    let b = instance_create(host.bullet, host.x, host.y);
    b.speed = SPEED;
    b.direction = d;
}
