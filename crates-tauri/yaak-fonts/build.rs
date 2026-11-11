const COMMANDS: &[&str] = &["list", "faces"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();

    // Tauri's common-controls-v6 feature needs comctl32 v6, which a Windows executable only gets
    // from its manifest. The app embeds one; test executables don't, so they fail to start
    // (STATUS_ENTRYPOINT_NOT_FOUND) unless it is added here. A library crate is never linked itself
    // and link args do not pass to dependents, so this reaches only this crate's test executables.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' \
             publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
