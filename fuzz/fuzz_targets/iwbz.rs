#![no_main]

//! `.iwbz` bytes from a stranger (#992): `iwbtg_format::iwbz::unpack` reads the zip's directory
//! and headers, enforces the archive limits, inflates each entry to its stated size and passes it
//! to `check_entry`. Both must answer `Ok` or an `IwbzError`, never panic, and the limits keep the
//! work bounded (docs/format/iwbz.md, Limits).

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = iwbtg_format::iwbz::unpack(data);
});
