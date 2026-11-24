// The boss stress script (#991): HP, two phases, a pattern table, a timeline and two alarms.
//
// The timeline (`timelines/boss_attack.toml`, started by the object's `timeline` key) sets the
// attack cadence; the phase picks the row of the pattern table each moment fires. Alarm 0 is the
// phase-two pulse, alarm 1 the grace window after a phase change, when hits do not count. Kid
// bullets hit through `collide(other)`. At 0 HP the boss puts a `game_clear` under the kid, whose
// touch raises `run_end`. No RNG: the same inputs give the same fight.

const HP_MAX = 12.0;
const PHASE_TWO_AT = 6.0;
const GRACE = 20;
const PULSE_EVERY = 9;
const PULSE_SHOTS = 6.0;

fn create() {
    this.hp = HP_MAX;
    this.phase = 1.0;
    this.grace = 0.0;
    this.volleys = 0.0;
    // One row per phase: [shots per fan, spread in degrees, shot speed]. Every fan opens to the
    // right (direction 0), away from the kid.
    this.pattern = [[5.0, 60.0, 3.0], [9.0, 100.0, 4.5]];
}

fn moment_intro() {
    emit("boss_cycle");
}

fn moment_fan() {
    fan(this, 0.0);
}

fn moment_fan_offset() {
    fan(this, 0.5);
}

fn moment_rest() {}

fn alarm(n) {
    if n == 0 {
        // Phase two only: a straight pulse to the right between the timeline's fans.
        let i = 0.0;
        while i < PULSE_SHOTS {
            shot(this, (i - (PULSE_SHOTS - 1.0) / 2.0) * 4.0, 6.0);
            i += 1.0;
        }
        this.set_alarm(0, PULSE_EVERY);
    } else if n == 1 {
        this.grace = 0.0;
    }
}

fn collide(other) {
    if other.kind != "kid_bullet" {
        return;
    }
    destroy(other);
    if gm_bool(this.grace) {
        return;
    }
    this.hp = this.hp - 1.0;
    if gm_le(this.hp, 0.0) {
        let k = kid();
        if k != () {
            // Without a save slot, `save_with_position = false` writes nothing.
            instance_create("game_clear", k.x, k.y, #{ save_with_position: false });
        }
        emit("boss_dead");
        destroy(this);
    } else if gm_eq(this.phase, 1.0) && gm_le(this.hp, PHASE_TWO_AT) {
        this.phase = 2.0;
        this.grace = 1.0;
        this.set_alarm(1, GRACE);
        this.set_alarm(0, GRACE + PULSE_EVERY);
        emit("boss_phase_two");
    }
}

// Fires the current phase's fan; `offset` shifts it by that fraction of one step.
fn fan(host, offset) {
    let row = host.pattern[to_int(host.phase) - 1];
    let shots = row[0];
    let spread = row[1];
    let step = spread / (shots - 1.0);
    let i = 0.0;
    while i < shots {
        shot(host, -spread / 2.0 + (i + offset) * step, row[2]);
        i += 1.0;
    }
    host.volleys = host.volleys + 1.0;
}

fn shot(host, d, speed) {
    let b = instance_create("@boss_shot", host.x + 16.0, host.y);
    b.speed = speed;
    b.direction = d;
}
