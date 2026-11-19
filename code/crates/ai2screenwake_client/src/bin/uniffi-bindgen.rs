// uniffi-bindgen.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Tooling-only entry into the pinned, actual UniFFI generator.

fn main() {
    if std::env::args().skip(1).eq(["--build-info"]) {
        println!(
            "{}",
            serde_json::json!({
                "version": ai2screenwake_client::build_info::VERSION,
                "stamp": ai2screenwake_client::build_info::STAMP,
            })
        );
        return;
    }
    uniffi::uniffi_bindgen_main();
}
// end uniffi-bindgen.rs
