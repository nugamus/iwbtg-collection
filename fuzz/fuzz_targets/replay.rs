#![no_main]

libfuzzer_sys::fuzz_target!(|_data: &[u8]| {
    // #994: `iwbtg_sim::replay::Replay::decode`.
});
