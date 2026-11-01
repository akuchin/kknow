fn main() {
    delay_load_comctl32();
    tauri_build::build()
}

/// Lets the test binaries start on Windows.
///
/// tao imports `TaskDialogIndirect` from comctl32. That export exists only in
/// Common Controls v6, and a process reaches v6 through an embedded manifest.
/// `tauri_build` embeds one, but only into the app binary (`resource.lib` goes
/// in through `rustc-link-arg-bins`). A test binary gets none, so the loader
/// binds comctl32 v5 — 5.82 in System32, which has no `TaskDialogIndirect` —
/// and the process dies with STATUS_ENTRYPOINT_NOT_FOUND before the first test.
///
/// Giving the test binary a manifest of its own is not possible here: cargo has
/// no link-arg scope for the unit tests of a lib, and the one scope that covers
/// them also covers bins, where a second manifest is a hard error (CVT1100).
///
/// So nothing from comctl32 is resolved at load time. The app still gets its
/// manifest from `tauri_build` and binds v6 on the first call. The tests never
/// call these functions: they create no window.
///
/// The target decides, not the host: a cross build to Windows needs this too.
fn delay_load_comctl32() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return; // Only the MSVC linker takes these flags.
    }
    println!("cargo:rustc-link-arg=/DELAYLOAD:comctl32.dll");
    println!("cargo:rustc-link-arg=delayimp.lib");
}
