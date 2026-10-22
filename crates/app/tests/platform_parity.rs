//! Cross-platform packaging claims, checked against the files that ship.
//!
//! Windows and macOS artefacts cannot be built on a Linux CI runner without a
//! cross linker and an SDK, but the parts that make a bundle look like a real
//! application — the icon containers, the property list and the packaging
//! script — are plain files. These tests keep those inputs honest instead of
//! letting a platform claim outlive the asset that backed it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn u32_be(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// The value of the first `<string>` after a plist key, or the next boolean
/// element. The plist is committed by this repository, so a small parser is
/// preferable to a build dependency for six assertions.
fn plist_after_key<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("<key>{key}</key>");
    let start = plist.find(&needle)? + needle.len();
    Some(&plist[start..])
}

fn plist_string<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let rest = plist_after_key(plist, key)?;
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")? + start;
    Some(rest[start..end].trim())
}

fn plist_bool(plist: &str, key: &str) -> Option<bool> {
    let rest = plist_after_key(plist, key)?;
    let false_at = rest.find("<false/>");
    let true_at = rest.find("<true/>");
    match (false_at, true_at) {
        // Whichever element comes first is the value.
        (Some(f), Some(t)) => Some(t < f),
        (Some(_), None) => Some(false),
        (None, Some(_)) => Some(true),
        (None, None) => None,
    }
}

fn assert_png(bytes: &[u8], context: &str) {
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "{context} is not a PNG payload: {:02x?}",
        &bytes[..bytes.len().min(8)]
    );
}

#[test]
fn the_windows_icon_has_the_sizes_explorer_and_the_taskbar_ask_for() {
    let bytes = read(&app_dir().join("assets/icon.ico"));
    assert!(bytes.len() >= 6, "ICO is too short");
    assert_eq!(u16_le(&bytes, 0), 0, "reserved field must be zero");
    assert_eq!(u16_le(&bytes, 2), 1, "type must be icon (1)");
    let count = u16_le(&bytes, 4) as usize;
    assert!(count >= 4, "only {count} icon directory entries");

    let directory_end = 6 + count * 16;
    assert!(bytes.len() >= directory_end, "truncated icon directory");

    let mut sizes = Vec::new();
    for index in 0..count {
        let entry = 6 + index * 16;
        let width = if bytes[entry] == 0 {
            256
        } else {
            bytes[entry] as usize
        };
        let height = if bytes[entry + 1] == 0 {
            256
        } else {
            bytes[entry + 1] as usize
        };
        let size = u32_le(&bytes, entry + 8) as usize;
        let offset = u32_le(&bytes, entry + 12) as usize;
        assert_eq!(width, height, "icon entry {index} is not square");
        assert!(size > 0, "icon entry {index} has no payload");
        assert!(
            offset >= directory_end && offset + size <= bytes.len(),
            "icon entry {index} points outside the file"
        );
        sizes.push(width);
    }

    for required in [16, 32, 48, 256] {
        assert!(
            sizes.contains(&required),
            "the .ico is missing {required}px; found {sizes:?}"
        );
    }
}

#[test]
fn the_macos_icon_is_a_well_formed_icns_container() {
    let bytes = read(&app_dir().join("assets/icon.icns"));
    assert!(bytes.len() > 8, "ICNS is too short");
    assert_eq!(&bytes[..4], b"icns", "wrong ICNS magic");
    assert_eq!(
        u32_be(&bytes, 4) as usize,
        bytes.len(),
        "the ICNS length field disagrees with the file"
    );

    let mut position = 8;
    let mut kinds = Vec::new();
    while position < bytes.len() {
        assert!(
            position + 8 <= bytes.len(),
            "truncated ICNS entry header at {position}"
        );
        let kind = &bytes[position..position + 4];
        let size = u32_be(&bytes, position + 4) as usize;
        assert!(size >= 8, "ICNS entry {kind:?} is smaller than its header");
        assert!(
            position + size <= bytes.len(),
            "ICNS entry {kind:?} runs past the end of the file"
        );
        kinds.push(kind.to_vec());
        if kind == b"icp4" || kind == b"icp5" || kind == b"ic07" || kind == b"ic08" {
            assert_png(&bytes[position + 8..position + size], "ICNS PNG entry");
        }
        position += size;
    }
    assert_eq!(position, bytes.len(), "ICNS entries do not fill the file");

    for required in [b"icp4", b"icp5", b"ic07", b"ic08", b"ic09"] {
        assert!(
            kinds
                .iter()
                .any(|kind| kind.as_slice() == required.as_slice()),
            "the .icns is missing {:?}; found {:?}",
            std::str::from_utf8(required).unwrap(),
            kinds
                .iter()
                .map(|kind| String::from_utf8_lossy(kind).into_owned())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn the_macos_property_list_describes_a_foreground_music_player() {
    let plist = std::fs::read_to_string(app_dir().join("macos/Info.plist"))
        .expect("read the committed Info.plist");
    assert_eq!(
        plist_string(&plist, "CFBundleShortVersionString"),
        Some(env!("CARGO_PKG_VERSION")),
        "the bundle version must move with the workspace manifest"
    );
    assert_eq!(
        plist_string(&plist, "CFBundleExecutable"),
        Some("shadowplayer")
    );
    assert_eq!(plist_string(&plist, "CFBundlePackageType"), Some("APPL"));
    assert_eq!(plist_string(&plist, "CFBundleIconFile"), Some("icon.icns"));
    assert_eq!(
        plist_string(&plist, "CFBundleIdentifier"),
        Some("org.shadowplayer.ShadowPlayer")
    );
    assert_eq!(
        plist_bool(&plist, "LSUIElement"),
        Some(false),
        "the app must keep its Dock icon and normal window behaviour"
    );
    assert_eq!(plist_bool(&plist, "NSHighResolutionCapable"), Some(true));
    assert!(
        !plist.contains("<key>NSMicrophoneUsageDescription</key>"),
        "the player has no capture feature and must never ask for a microphone"
    );
    assert!(
        app_dir().join("assets/icon.icns").exists(),
        "CFBundleIconFile names a file that is not committed"
    );
}

#[test]
fn the_windows_build_embeds_the_same_icon_and_version() {
    let build = std::fs::read_to_string(app_dir().join("build.rs")).expect("read build.rs");
    assert!(
        build.contains("winresource"),
        "Windows resources are not built"
    );
    assert!(
        build.contains("assets/icon.ico"),
        "the build does not use the icon"
    );
    assert!(
        build.contains("FileVersion"),
        "the executable has no file version"
    );
    assert!(build.contains("env!(\"CARGO_PKG_VERSION\")"));

    let main = std::fs::read_to_string(app_dir().join("src/main.rs")).expect("read main.rs");
    assert!(
        main.contains("#![cfg_attr(windows, windows_subsystem = \"windows\")]"),
        "a Windows release would open a console window"
    );
}

#[test]
fn the_packaging_script_is_executable_and_produces_the_expected_bundle() {
    let script = app_dir().join("macos/package-app.sh");
    let metadata = std::fs::metadata(&script).expect("the packaging script must exist");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            metadata.permissions().mode() & 0o111,
            0,
            "package-app.sh is not executable"
        );
    }
    let script_text = std::fs::read_to_string(&script).expect("read package-app.sh");
    assert!(script_text.starts_with("#!/usr/bin/env bash"));
    assert!(script_text.contains("Info.plist"));
    assert!(script_text.contains("icon.icns"));
    assert!(script_text.contains("Contents/MacOS/shadowplayer"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "sp_bundle_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let binary = root.join("shadowplayer");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new("bash")
            .arg(&script)
            .arg(&binary)
            .arg(root.join("dist"))
            .output()
            .expect("run package-app.sh");
        assert!(
            output.status.success(),
            "package-app.sh failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let bundle = root.join("dist/ShadowPlayer.app");
        let executable = bundle.join("Contents/MacOS/shadowplayer");
        assert!(executable.is_file(), "bundle has no executable");
        assert_ne!(
            std::fs::metadata(&executable).unwrap().permissions().mode() & 0o111,
            0,
            "bundled executable is not executable"
        );
        let bundled_plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
        assert_eq!(
            plist_string(&bundled_plist, "CFBundleShortVersionString"),
            Some(env!("CARGO_PKG_VERSION")),
            "the packaging step did not rewrite the version"
        );
        let bundled_icon = read(&bundle.join("Contents/Resources/icon.icns"));
        assert_eq!(&bundled_icon[..4], b"icns");

        let _ = std::fs::remove_dir_all(root);
    }
}
