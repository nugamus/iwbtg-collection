#![no_main]

libfuzzer_sys::fuzz_target!(|_data: &[u8]| {
    // #995: `Iws::compile`, then a bounded run with the input as one object's `step`.
});
