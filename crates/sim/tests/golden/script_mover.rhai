// The golden corpus script API case: a mover the controller creates. It reads and writes
// every instance property and bounces off blocks, by its collision event and by the bounce
// built-ins.

fn create() {
    this.gravity = 0.05;
    this.gravity_direction = 270.0;
    this.friction = 0.01;
    this.image_speed = 0.5;
    this.image_index = 0.0;
    this.image_alpha = 0.75;
    this.image_angle = 90.0;
    this.image_xscale = 1.5;
    this.image_yscale = 0.5;
    this.depth = -1.0;
    this.solid = false;
    this.visible = true;
    this.hp = 2.0;
    this.set_alarm(0, 200);
}

fn step() {
    let moved = point_distance(this.xprevious, this.yprevious, this.x, this.y);
    let from_start = point_distance(this.xstart, this.ystart, this.x, this.y);
    if gm_gt(moved, 8.0) || gm_gt(from_start, 900.0) {
        this.speed = 0.0;
    }
    if this.kind != "mover" || this.id < 0.0 {
        emit("mover_bad");
    }
    if gm_bool(this.image_index) {
        this.image_angle = this.image_angle + this.direction * 0.0 + 90.0;
    }
    if this.hspeed == 0.0 && this.vspeed == 0.0 && this.speed == 0.0 {
        this.vspeed = -1.0;
        this.hspeed = 0.5;
    }
    this.y = this.y + 0.0;
    // The bounce built-ins (#835), precise and not, alternating with the animation frame.
    if gm_bool(this.image_index) {
        this.move_bounce_solid(true);
        this.move_bounce_all(false);
    } else {
        this.move_bounce_solid(false);
        this.move_bounce_all(true);
    }
    this.visible = this.solid == false && this.image_alpha > 0.5 && this.image_xscale > 0.0
        && this.image_yscale > 0.0 && this.depth < 0.0 && this.friction > 0.0
        && this.gravity > 0.0 && this.gravity_direction > 0.0 && this.image_speed > 0.0;
}

fn collide(other) {
    this.hspeed = -this.hspeed;
    this.vspeed = -this.vspeed * 0.5;
    this.hp -= 1.0;
    if gm_le(this.hp, 0.0) { destroy(this); }
}

fn outside_room() {
    destroy(this);
}

fn alarm(n) {
    destroy(this);
}
