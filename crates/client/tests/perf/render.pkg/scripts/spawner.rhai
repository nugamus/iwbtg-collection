// The stable-population spawner (#1024): fills the room once with `count` dots, 250 a tick (the
// host caps one event call at 256 spawns), laid out in a grid, so every run starts with the same
// population and the same picture.
const BATCH = 250.0;
const ACROSS = 790.0;
const DOWN = 590.0;

fn create() {
    this.spawned = 0.0;
}

fn step() {
    let left = this.count - this.spawned;
    if left <= 0.0 {
        return;
    }
    let n = min(BATCH, left);
    let i = 0.0;
    while i < n {
        let at = this.spawned + i;
        let x = 4.0 + at % ACROSS;
        let y = 4.0 + (at / ACROSS) % DOWN;
        instance_create(this.bullet, x, y);
        i += 1.0;
    }
    this.spawned = this.spawned + n;
}
