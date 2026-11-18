// A boss with a bug in its Create (the start error fixture, #946): it stands in the start room,
// so the script stops with an error in the run's very first tick, while the sim is built.
fn create() {
    let patterns = [1.0, 2.0];
    this.hspeed = patterns[patterns.len() + 1];
}
