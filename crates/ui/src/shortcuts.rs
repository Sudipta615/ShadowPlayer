//! The single source of truth for the app's keyboard shortcuts.
//!
//! The shortcut list used to live twice — once in the Help view and once in a
//! second, unreachable help modal — so the two could drift apart silently and
//! neither was checked against the code that actually handles the keys. It
//! lives here now: [`SHORTCUTS`] is what the Help view renders, and
//! [`tests::readme_matches_the_shortcut_table`] fails the build if the README
//! stops describing the same set of keys.

/// One keyboard shortcut: the keys as the user presses them, and what they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    /// The key combination, as written in the Help view and the README.
    pub keys: &'static str,
    /// What the shortcut does.
    pub description: &'static str,
}

/// Every app-wide keyboard shortcut, in the order the Help view lists them.
///
/// Keep this in the same order as the README's "Keyboard shortcuts" table; the
/// consistency test checks the *set* of keys, and the order is a reading
/// convenience.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        keys: "Space / K",
        description: "Play / pause the active track",
    },
    Shortcut {
        keys: "J",
        description: "Skip to the previous track in the queue",
    },
    Shortcut {
        keys: "L",
        description: "Skip to the next track in the queue",
    },
    Shortcut {
        keys: "M",
        description: "Toggle mute / unmute on the master output",
    },
    Shortcut {
        keys: "S",
        description: "Toggle random shuffle",
    },
    Shortcut {
        keys: "R",
        description: "Cycle repeat mode (off → all → one)",
    },
    Shortcut {
        keys: "Left Arrow",
        description: "Seek backward by 5 seconds",
    },
    Shortcut {
        keys: "Right Arrow",
        description: "Seek forward by 5 seconds",
    },
    Shortcut {
        keys: "Up Arrow",
        description: "Raise the master volume by 5 %",
    },
    Shortcut {
        keys: "Down Arrow",
        description: "Lower the master volume by 5 %",
    },
    Shortcut {
        keys: "/",
        description: "Jump to the search box",
    },
    Shortcut {
        keys: "Ctrl + F",
        description: "Jump to the search box",
    },
    Shortcut {
        keys: "Ctrl + Space",
        description: "Open the command palette",
    },
    Shortcut {
        keys: "F",
        description: "Full-screen Now Playing",
    },
    Shortcut {
        keys: "Ctrl + B",
        description: "Toggle bit-perfect Pure Direct mode",
    },
    Shortcut {
        keys: "Ctrl + P",
        description: "Open Settings & DSP studio",
    },
    Shortcut {
        keys: "Ctrl + N",
        description: "Open the playlist editor (create or manage playlists)",
    },
    Shortcut {
        keys: "Ctrl + O",
        description: "Open Folders & library manager",
    },
    Shortcut {
        keys: "Ctrl + E",
        description: "Toggle the equalizer & DSP panel",
    },
    Shortcut {
        keys: "Ctrl + 1 to 5",
        description: "Rate the selected tracks or the playing track 1–5 stars",
    },
    Shortcut {
        keys: "Ctrl + 0",
        description: "Clear the rating on the selection or the playing track",
    },
    Shortcut {
        keys: "Ctrl + Alt + Up / Down",
        description: "Step the selection's rating up or down",
    },
    Shortcut {
        keys: "Escape",
        description: "Close the open dialog (one layer per press), or clear the search",
    },
];

#[cfg(test)]
mod tests {
    use super::SHORTCUTS;

    /// The README is the app's front door; if it stops mentioning a shortcut the
    /// Help view offers, one of them is lying to the user.
    #[test]
    fn readme_matches_the_shortcut_table() {
        let readme =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"))
                .expect("README.md should be readable");

        // Pull the key column out of the README's "Keyboard shortcuts" table.
        let mut documented: Vec<String> = Vec::new();
        let mut in_table = false;
        for line in readme.lines() {
            let line = line.trim();
            if line.starts_with("## ") {
                in_table = line.contains("Keyboard shortcuts");
                continue;
            }
            if !in_table || !line.starts_with('|') {
                continue;
            }
            let cells: Vec<&str> = line
                .split('|')
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .collect();
            let Some(key) = cells.first() else { continue };
            // Skip the header row ("Key") and the alignment row ("| :--- |").
            if *key == "Key" || key.starts_with(':') || key.starts_with('-') {
                continue;
            }
            documented.push((*key).to_string());
        }

        let expected: Vec<String> = SHORTCUTS.iter().map(|s| s.keys.to_string()).collect();

        assert_eq!(
            documented, expected,
            "README.md's keyboard shortcut table and the in-app list have drifted apart"
        );
    }
}
