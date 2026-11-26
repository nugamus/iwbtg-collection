#![no_main]

libfuzzer_sys::fuzz_target!(|_data: &[u8]| {
    // #992: `iwbtg_format::iwbz::{check_entry, unpack}`.
});
