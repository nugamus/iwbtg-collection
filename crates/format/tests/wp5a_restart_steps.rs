//! #224: `restart = { kid_steps = { ... } }` in `game.toml` becomes `RestartModel::kid_steps`.

use iwbtg_format::manifest::RestartSection;
use iwbtg_sim::def::RetryKidSteps;

fn model(text: &str) -> Result<iwbtg_sim::def::RestartModel, toml::de::Error> {
    Ok(toml::from_str::<RestartSection>(text)?.model())
}

#[test]
fn kid_steps_default_to_off() {
    assert_eq!(model("").unwrap().kid_steps, RetryKidSteps::default());
}

#[test]
fn kid_steps_are_read() {
    let m = model("kind = \"room_goto_saved\"\nkid_steps = { add_time = 2, save_death_time = true, clear_frozen = true }\n").unwrap();
    assert_eq!(
        m.kid_steps,
        RetryKidSteps {
            add_time: 2,
            save_death_time: true,
            clear_frozen: true,
            reset_jump_values: false
        }
    );
    let m = model("kid_steps = { reset_jump_values = true }\n").unwrap();
    assert!(m.kid_steps.reset_jump_values && m.kid_steps.add_time == 0);
}

#[test]
fn an_unknown_kid_step_is_an_error() {
    assert!(model("kid_steps = { add_tiem = 2 }\n").is_err());
}

#[test]
fn death_on_restart_per_kid_is_read() {
    use iwbtg_sim::def::DeathOnRestart;
    let m = model("death_on_restart = \"save_counters_per_kid\"\n").unwrap();
    assert_eq!(m.death_on_restart, DeathOnRestart::SaveCountersPerKid);
}
