// Variant B's bullet: one script call per bullet per tick, the worst case the script layer must
// carry. The turn is fixed (never zero), so it is a real per-step state change and stays
// deterministic; native motion moves the bullet after Step.
const TURN = 0.5;

fn step() {
    this.direction = this.direction + TURN;
}
