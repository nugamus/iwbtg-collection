// The golden corpus spark: a short-lived object another script creates.
fn create() { this.set_alarm(0, 12); }

fn alarm(n) { destroy(this); }
