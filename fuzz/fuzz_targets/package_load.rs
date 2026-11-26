#![no_main]

libfuzzer_sys::fuzz_target!(|_data: &[u8]| {
    // #993: unpack, then `iwbtg_format::load_report` and the package check.
});
