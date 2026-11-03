// The golden corpus walker (WP9): create, step, alarm and collide.
fn create() {
    this.hp = 3.0;
    this.hspeed = 2.0;
    this.set_alarm(0, 20);
}

fn step() {
    if this.x > 700.0 { this.hspeed = -2.0; }
    if this.x < 100.0 { this.hspeed = 2.0; }
}

fn alarm(n) {
    if n == 0 {
        if random(1.0) < 0.5 {
            let s = instance_create("spark", this.x, this.y - 8.0);
            s.vspeed = -1.5;
        }
        this.set_alarm(0, 15.0 + irandom(10.0));
    }
}

fn collide(other) {
    this.hspeed = -this.hspeed;
    this.x += this.hspeed;
    this.hp -= 1.0;
    if gm_le(this.hp, 0.0) {
        this.hp = 3.0;
        emit("walker_reset");
    }
}
