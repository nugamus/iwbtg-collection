// Spawn/destroy churn (#1024): every tick it creates `rate` dots that destroy themselves after
// `LIFE` ticks, so the population settles at `rate * LIFE` with a constant spawn/destroy rate.
fn step() {
    let i = 0.0;
    while i < this.rate {
        instance_create(this.bullet, 400.0, 300.0);
        i += 1.0;
    }
}
