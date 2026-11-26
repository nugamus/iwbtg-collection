// A dot that lives `LIFE` ticks and then destroys itself; with `churner` this keeps the population
// at `rate * LIFE` while the client creates and destroys sprites every tick.
const LIFE = 100.0;

fn create() {
    this.life = 0.0;
}

fn step() {
    this.life = this.life + 1.0;
    if (this.life >= LIFE) {
        destroy(this);
    }
}
