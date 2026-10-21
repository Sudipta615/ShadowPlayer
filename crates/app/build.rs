//! Build script for the ShadowPlayer player binary.
//!
//! Its only job is Windows: embed the application icon and the version
//! metadata into the `.exe`, so the file carries the right mark in Explorer,
//! in the taskbar and in the "Apps and Features" list.
//!
//! Everything else is a no-op, so the Linux and macOS builds do not pay for
//! it and do not need a resource compiler.
//!
//! The icon is embedded on a *best effort* basis. A missing or broken
//! resource compiler must not break the build — the window icon is also set
//! at runtime from the same PNG (see `window_icon` in `src/main.rs`), so the
//! app has its mark either way. What the embedded resource adds is the
//! Explorer's own preview of the file, which the runtime cannot reach.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let icon = std::path::Path::new("assets/icon.ico");
        if !icon.exists() {
            println!("cargo:warning=assets/icon.ico is missing; the executable will have no embedded icon");
            return;
        }

        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "ShadowPlayer");
        res.set("ProductName", "ShadowPlayer");
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("CompanyName", "ShadowPlayer");
        res.set("LegalCopyright", "Apache-2.0");

        match res.compile() {
            Ok(()) => println!("cargo:rustc-link-arg-bins=/SUBSYSTEM:WINDOWS"),
            Err(e) => {
                // A resource compiler that is missing or unhappy is a warning,
                // not a build failure: the app still runs, and still shows its
                // icon in the window.
                println!("cargo:warning=could not embed the Windows icon: {e}");
            }
        }
    }
}
