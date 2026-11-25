// The golden corpus script API case (#786): a controller that calls every function and
// property of the script API at least once (docs/format/script-api.md). The coverage test in
// golden_corpus.rs fails when a registered name is missing from the corpus scripts.
// It sits at x 400 in the course and at x 401 in the annex, so `xstart` says which room it is.

fn create() {
    this.hp = 3.0;
    this.phase = 0.0;
    // The read-only room-file name and group: strings, or `()` when the room file gives none.
    // `&` evaluates both operands (this controller is unnamed, so the body does not run).
    if (this.name == ()) & (this.group == ()) { this.phase = 0.0; }
    this.set_alarm(0, 10);
    this.set_alarm(1, 25.6);
}

fn room_start() {
    play_music("theme");
}

fn room_end() {
    stop_music();
}

fn begin_step() {
    // Numbers: every math function, each result folded into `phase`.
    let a = sin(0.5) + cos(0.25) + tan(0.125) + arctan2(1.0, 2.0) + exp(0.5) + ln(3.0);
    let b = dsin(30.0) + dcos(60.0) + darctan2(1.0, 1.0) + degtorad(90.0) + radtodeg(1.0);
    let c = sqrt(2.0) + sqr(1.5) + pow(2.0, 0.5) + abs(-1.25) + abs(-3) + sign(-2.0) + sign(4) + frac(-2.75);
    let d = floor(2.5) + ceil(2.5) + round(2.5) + clamp(9.0, 0.0, 4.0) + lerp(0.0, 10.0, 0.3);
    let e = max(1.0, 2.0) + min(1.0, 2.0) + max(3, 4) + min(3, 4) + div(7.0, 2.0) + mod(-7.0, 3.0);
    let f = point_distance(0.0, 0.0, 3.0, 4.0) + point_direction(0.0, 0.0, 1.0, 1.0);
    let g = lengthdir_x(10.0, 30.0) + lengthdir_y(10.0, 30.0) + to_float(to_int(2.9)) + 1 / 2;
    this.phase = frac((a + b + c + d + e + f + g + this.phase) / 1000.0) * 1000.0;
}

fn step() {
    let t = room_tick();
    let course = gm_eq(this.xstart, 400.0);
    // Stored arrays (#1383): iterate and push/pop once, on the variable and on a local copy.
    if t == 1.0 {
        this.arr = [1.0, 2.0];
        this.arr.push(3.0);
        this.arr.pop();
        let b = this.arr;
        b.push(4.0);
        b.pop();
        let sum = 0.0;
        for v in this.arr { sum += v; }
        this.phase = this.phase + sum;
    }
    if course {
        if t == 100.0 && tick() < 200.0 { room_restart(); }
        if t == 250.0 && gm_gt(tick(), 1200.0) { kill_kid(); }
        if t == 300.0 { room_goto_next(); }
    } else if t == 120.0 {
        if gm_bool(irandom(1.0)) { room_goto_previous(); } else { room_goto("course"); }
    }
}

fn end_step() {
    // Comparisons with the run's epsilon.
    let p = this.phase;
    if gm_lt(p, 0.0) || gm_le(p, -1.0) || gm_ge(p, 1e9) || gm_ne(p, p) {
        emit("phase_out_of_range");
    }
    // Collision queries (#777): each result folded into `phase`.
    let hits = 0.0;
    if place_meeting(this, this.x, this.y + 32.0, "block") { hits += 1.0; }
    if this.place_meeting(this.x, this.y, "mover") { hits += 2.0; }
    if place_free(this, this.x, this.y - 32.0) { hits += 4.0; }
    if instance_place(this, this.x, this.y, "all") != () { hits += 8.0; }
    if collision_point(this.x, this.y + 40.0, "block") != () { hits += 16.0; }
    if collision_point(this.x, this.y, "all", false, this) != () { hits += 32.0; }
    if collision_rect(this.x - 64.0, this.y, this.x + 64.0, this.y + 64.0, "mover") != () { hits += 64.0; }
    if collision_rect(0.0, 0.0, 800.0, 608.0, "all", true, this) != () { hits += 128.0; }
    if collision_line(this.x, this.y, this.x, this.y + 200.0, "block") != () { hits += 256.0; }
    if collision_line(this.x - 300.0, this.y, this.x + 300.0, this.y, "mover", false, ()) != () { hits += 512.0; }
    this.phase = this.phase + hits / 4096.0;
    for i in 0..gm_repeat(2.4) {
        this.hp = this.hp + to_float(i) * 0.0;
    }
}

fn alarm(n) {
    if n == 0 {
        // Instances: create, read back, steer, destroy the oldest beyond three.
        let m = instance_create("mover", this.x + random_range(-64.0, 64.0), this.y);
        m.direction = choose([0.0, 45.0, 135.0, 180.0]);
        m.speed = 1.0 + random(2.0);
        let s = instance_create("spike", 400.0, -64.0, #{ dir: "down" });
        if exists(s) { destroy(s); }
        let all = instances("mover");
        if all.len() > 3 { destroy(all[0]); }
        play_sound("blip");
        this.set_alarm(0, 20 + irandom_range(0.0, 15.0));
    } else {
        let k = kid();
        if k != () {
            this.hp = this.hp - 1.0;
            if gm_le(this.hp, 0.0) {
                this.hp = 3.0;
                shake(10.0, 2.0);
                emit("api_tick");
            }
            this.x = clamp(k.x, 100.0, 700.0);
            // The kid field API (decision 0004): every getter reads, and every writable field
            // writes its current value back so the corpus covers the registration without
            // changing the run.
            k.djump = k.djump;
            k.tjump = k.tjump;
            k.onPlatform = k.onPlatform;
            k.ladder = k.ladder;
            k.frozen = k.frozen;
            k.frozen2 = k.frozen2;
            k.muteki = k.muteki;
            k.curJumps = k.curJumps;
            k.tempHspeed = k.tempHspeed;
            k.maxSpeed = k.maxSpeed;
            k.maxVspeed = k.maxVspeed;
            k.maxFallSpeed = k.maxFallSpeed;
            k.scale = k.scale;
            // The read-only kid fields.
            let flags = k.ladderud || k.ladderjump || k.flipped || k.on_slope_last || k.onVineL || k.onVineR;
            let nums = k.yflag + k.facing + k.walk_h + k.jump + k.jump2 + k.jump3;
            if flags && gm_lt(nums, -1e9) { emit("kid_fields"); }
        }
        this.set_alarm(1, 30.0 + this.get_alarm(0) * 0.0);
    }
}
