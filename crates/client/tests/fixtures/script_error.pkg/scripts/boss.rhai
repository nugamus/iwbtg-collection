// A boss with a bug (the script error fixture, #101): half a second in, it reads a pattern
// that is not there, and the script stops with an error.
fn create() {
    this.set_alarm(0, 25);
}

fn alarm(n) {
    let patterns = [1.0, 2.0];
    this.hspeed = patterns[patterns.len() + 1];
}
