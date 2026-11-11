// The boss intro: it lasts 30 ticks, then goes (a `boss_intro` cutscene region holds meanwhile).
fn create() {
    this.set_alarm(0, 30);
}

fn alarm(n) {
    destroy(this);
}
