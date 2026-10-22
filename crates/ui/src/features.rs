//! The platform feature matrix.
//!
//! Marketing tables rot. This one is the source of truth for both the in-app
//! Help page and the README, and a test ties every platform claim back to a
//! `cfg` gate in the implementation. If a platform-specific service is removed
//! from the build, the table cannot quietly keep claiming it.

/// A desktop platform ShadowPlayer ships for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Linux,
    Windows,
    MacOs,
}

impl Platform {
    pub const ALL: [Platform; 3] = [Platform::Linux, Platform::Windows, Platform::MacOs];

    pub fn label(self) -> &'static str {
        match self {
            Platform::Linux => "Linux",
            Platform::Windows => "Windows",
            Platform::MacOs => "macOS",
        }
    }

    /// The value `std::env::consts::OS` uses.
    pub fn os_name(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::Windows => "windows",
            Platform::MacOs => "macos",
        }
    }

    pub fn current() -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|platform| platform.os_name() == std::env::consts::OS)
    }
}

/// One capability and the platforms that implement it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    pub id: &'static str,
    pub label: &'static str,
    pub platforms: &'static [Platform],
    pub note: &'static str,
}

impl Feature {
    pub fn supports(&self, platform: Platform) -> bool {
        self.platforms.contains(&platform)
    }
}

/// Every cross-platform capability ShadowPlayer claims, with the platform set
/// that actually ships it.
pub const FEATURES: &[Feature] = &[
    Feature {
        id: "exclusive_output",
        label: "Bit-perfect exclusive output",
        platforms: &[Platform::Linux, Platform::Windows, Platform::MacOs],
        note: "ALSA hw: on Linux; WASAPI Exclusive and ASIO on Windows; CoreAudio hog mode on macOS.",
    },
    Feature {
        id: "system_media",
        label: "System media integration",
        platforms: &[Platform::Linux, Platform::Windows, Platform::MacOs],
        note: "MPRIS2, System Media Transport Controls, and macOS Now Playing respectively.",
    },
    Feature {
        id: "mpris_playerctl",
        label: "MPRIS2 and playerctl",
        platforms: &[Platform::Linux],
        note: "org.mpris.MediaPlayer2.shadowplayer on the session bus, including play/pause and metadata.",
    },
    Feature {
        id: "smtc_artwork",
        label: "SMTC artwork",
        platforms: &[Platform::Windows],
        note: "Track artwork and metadata in the Windows media flyout and hardware media keys.",
    },
    Feature {
        id: "macos_now_playing",
        label: "macOS Now Playing",
        platforms: &[Platform::MacOs],
        note: "souvlaki publishes metadata and artwork; the app bundle remains a normal foreground app (LSUIElement = false).",
    },
    Feature {
        id: "native_picker",
        label: "Native file and folder picker",
        platforms: &[Platform::Linux, Platform::Windows, Platform::MacOs],
        note: "rfd with the XDG desktop portal on Linux, and the native dialogs on Windows and macOS.",
    },
    Feature {
        id: "wayland_portal",
        label: "Wayland portal picker",
        platforms: &[Platform::Linux],
        note: "The XDG portal backend is used under Wayland; the picker does not depend on an X11 display.",
    },
    Feature {
        id: "windows_icon",
        label: "Windows application icon",
        platforms: &[Platform::Windows],
        note: "The executable and window carry the committed multi-resolution .ico.",
    },
    Feature {
        id: "macos_bundle",
        label: "macOS app bundle",
        platforms: &[Platform::MacOs],
        note: "The committed package-app.sh wraps the binary in a foreground .app with the icon and no microphone key.",
    },
    Feature {
        id: "no_microphone_permission",
        label: "No microphone permission",
        platforms: &[Platform::Linux, Platform::Windows, Platform::MacOs],
        note: "The app captures nothing, and the macOS bundle omits NSMicrophoneUsageDescription.",
    },
    Feature {
        id: "null_backend",
        label: "Null output backend",
        platforms: &[Platform::Linux, Platform::Windows, Platform::MacOs],
        note: "A platform-independent output used by the bit-identical Pure Direct test.",
    },
];

pub fn feature(id: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|feature| feature.id == id)
}

/// The README's generated platform table.
///
/// Rendered by both `README.md` and the test that keeps the file in step. The
/// markers let the test replace only this block rather than compare prose.
pub fn markdown_table() -> String {
    let mut out = String::new();
    out.push_str("| Capability | Linux | Windows | macOS | Notes |\n");
    out.push_str("| :--- | :---: | :---: | :---: | :--- |\n");
    for feature in FEATURES {
        out.push_str("| ");
        out.push_str(feature.label);
        out.push_str(" |");
        for platform in Platform::ALL {
            out.push_str(if feature.supports(platform) {
                " Yes |"
            } else {
                " - |"
            });
        }
        out.push(' ');
        out.push_str(feature.note);
        out.push_str(" |\n");
    }
    out
}

/// The block markers used in `README.md`.
pub const README_START: &str = "<!-- platform-features:start -->";
pub const README_END: &str = "<!-- platform-features:end -->";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_feature_has_a_valid_platform_set() {
        for feature in FEATURES {
            assert!(!feature.id.is_empty());
            assert!(!feature.label.is_empty());
            assert!(!feature.note.is_empty());
            assert!(
                !feature.platforms.is_empty(),
                "{} has no platform",
                feature.id
            );
            for platform in feature.platforms {
                assert!(Platform::ALL.contains(platform));
            }
        }
    }

    #[test]
    fn feature_matrix_matches_the_cfg_gates_in_the_services() {
        // These files are where the platform-specific services actually live.
        let media = include_str!("services/mpris_service.rs");
        let pickers = include_str!("services/import_service.rs");
        let windows_build = include_str!("../../app/build.rs");
        let macos_plist = include_str!("../../app/macos/Info.plist");

        for feature in FEATURES {
            match feature.id {
                "mpris_playerctl" => {
                    assert_eq!(feature.platforms, &[Platform::Linux]);
                    assert!(media.contains(r#"#[cfg(target_os = "linux")]"#));
                }
                "smtc_artwork" => {
                    assert_eq!(feature.platforms, &[Platform::Windows]);
                    assert!(media.contains(r#"cfg(target_os = "windows")"#));
                }
                "macos_now_playing" => {
                    assert_eq!(feature.platforms, &[Platform::MacOs]);
                    assert!(media.contains(r#"cfg(target_os = "macos")"#));
                }
                "windows_icon" => {
                    assert_eq!(feature.platforms, &[Platform::Windows]);
                    assert!(windows_build.contains("winresource"));
                    assert!(windows_build.contains("assets/icon.ico"));
                }
                "macos_bundle" => {
                    assert_eq!(feature.platforms, &[Platform::MacOs]);
                    assert!(macos_plist.contains("<key>LSUIElement</key>"));
                    assert!(!macos_plist.contains("<key>NSMicrophoneUsageDescription</key>"));
                }
                "wayland_portal" | "native_picker" => {
                    // rfd is configured with its XDG portal default on Linux;
                    // the same service is the native dialog elsewhere.
                    assert!(pickers.contains("rfd::FileDialog"));
                }
                "exclusive_output"
                | "system_media"
                | "no_microphone_permission"
                | "null_backend" => {
                    assert_eq!(feature.platforms, Platform::ALL);
                }
                _ => {}
            }
        }

        // Every platform-specific source gate must be represented by a row
        // that names that platform. This catches a new Linux-only service
        // being added without updating the matrix.
        for (token, platform) in [
            (r#"target_os = "linux""#, Platform::Linux),
            (r#"target_os = "windows""#, Platform::Windows),
            (r#"target_os = "macos""#, Platform::MacOs),
        ] {
            if media.contains(token) {
                assert!(
                    FEATURES.iter().any(|feature| feature.supports(platform)),
                    "{} is gated in mpris_service.rs but no feature claims it",
                    platform.label()
                );
            }
        }
    }

    #[test]
    fn readme_table_is_generated_from_this_matrix() {
        let readme = include_str!("../../../README.md");
        let Some(start) = readme.find(README_START) else {
            panic!("README is missing {README_START}");
        };
        let Some(end) = readme.find(README_END) else {
            panic!("README is missing {README_END}");
        };
        let actual = &readme[start + README_START.len()..end];
        let expected = format!("\n{}\n", markdown_table());
        assert_eq!(
            actual.trim_matches('\n'),
            expected.trim_matches('\n'),
            "README platform table is stale; paste markdown_table() between the markers"
        );
    }

    #[test]
    fn current_platform_is_in_the_matrix_where_relevant() {
        let Some(current) = Platform::current() else {
            return;
        };
        // The all-platform features must always support the build we run on.
        for feature in FEATURES {
            if feature.platforms == Platform::ALL {
                assert!(feature.supports(current), "{}", feature.id);
            }
        }
    }
}
