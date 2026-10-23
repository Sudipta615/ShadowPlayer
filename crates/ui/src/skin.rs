//! Backward compatibility shim for the legacy skin module.
//!
//! All theming functionality now lives in [`crate::theme`].

pub use crate::theme::{
    LoadedTheme as LoadedSkin,
    ThemeError as SkinError,
    ThemeFile as SkinFile,
    ThemeManager as SkinManager,
    REQUIRED_TOKENS,
};
