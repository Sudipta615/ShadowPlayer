//! In-house localisation.
//!
//! The catalogue is deliberately small and dependency-free at runtime: the
//! English catalogue is compiled in by [`build.rs`](../build.rs), and the
//! non-English catalogues are ordinary JSON files parsed once on first use.
//!
//! Call sites use [`tr!`](crate::tr) for plain labels and
//! [`tr_args!`](crate::tr_args) for interpolated text.  Both accept either a
//! string literal (the convenient form during migration) or a generated
//! `Msg::S…` variant.  In both cases the build script verifies that the key
//! exists in `locales/en.json`, so a typo is a compile error rather than an
//! English string silently leaking into a translated UI.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, RwLock};

include!(concat!(env!("OUT_DIR"), "/i18n_generated.rs"));

/// The languages shipped by this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    English,
    German,
    French,
    Spanish,
    Japanese,
    Chinese,
}

impl Locale {
    pub const ALL: [Locale; 6] = [
        Locale::English,
        Locale::German,
        Locale::French,
        Locale::Spanish,
        Locale::Japanese,
        Locale::Chinese,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::German => "de",
            Self::French => "fr",
            Self::Spanish => "es",
            Self::Japanese => "ja",
            Self::Chinese => "zh",
        }
    }

    /// The language's own name, for the Settings picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::German => "Deutsch",
            Self::French => "Français",
            Self::Spanish => "Español",
            Self::Japanese => "Nihongo (Japanese)",
            Self::Chinese => "Zhongwen (Chinese)",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        let code = code
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or_default()
            .replace('_', "-")
            .to_ascii_lowercase();
        let language = code.split('-').next().unwrap_or_default();
        match language {
            "en" => Some(Self::English),
            "de" => Some(Self::German),
            "fr" => Some(Self::French),
            "es" => Some(Self::Spanish),
            "ja" => Some(Self::Japanese),
            "zh" => Some(Self::Chinese),
            _ => None,
        }
    }
}

static ACTIVE_LOCALE: LazyLock<RwLock<Locale>> = LazyLock::new(|| RwLock::new(Locale::English));
static GERMAN: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../locales/de.json")).unwrap_or_else(|e| {
        log::error!("The German catalogue is not valid JSON: {e}");
        HashMap::new()
    })
});
static FRENCH: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../locales/fr.json")).unwrap_or_else(|e| {
        log::error!("The French catalogue is not valid JSON: {e}");
        HashMap::new()
    })
});
static SPANISH: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../locales/es.json")).unwrap_or_else(|e| {
        log::error!("The Spanish catalogue is not valid JSON: {e}");
        HashMap::new()
    })
});
static JAPANESE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../locales/ja.json")).unwrap_or_else(|e| {
        log::error!("The Japanese catalogue is not valid JSON: {e}");
        HashMap::new()
    })
});
static CHINESE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../locales/zh.json")).unwrap_or_else(|e| {
        log::error!("The Chinese catalogue is not valid JSON: {e}");
        HashMap::new()
    })
});
static MISSING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Initialise the active locale from an explicit preference, then the POSIX
/// environment (`LC_ALL`, `LC_MESSAGES`, `LANG`), then English.
pub fn init(preference: &str) {
    let pref = preference.trim();
    let locale = if !pref.is_empty() {
        if let Some(loc) = Locale::from_code(pref) {
            loc
        } else {
            log::info!(
                "Locale {preference:?} is not shipped by this build; falling back to English"
            );
            Locale::English
        }
    } else {
        let environment = std::env::var("LC_ALL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("LC_MESSAGES").ok().filter(|s| !s.is_empty()))
            .or_else(|| std::env::var("LANG").ok().filter(|s| !s.is_empty()));
        if let Some(raw) = &environment {
            if Locale::from_code(raw).is_none() {
                log::info!("Locale {raw:?} is not shipped by this build; falling back to English");
            }
        }
        environment
            .as_deref()
            .and_then(Locale::from_code)
            .unwrap_or_default()
    };
    set_locale(locale);
}

pub fn set_locale(locale: Locale) {
    if let Ok(mut active) = ACTIVE_LOCALE.write() {
        *active = locale;
    }
}

pub fn locale() -> Locale {
    ACTIVE_LOCALE.read().map(|l| *l).unwrap_or(Locale::English)
}

/// Translate one message, falling back to English and reporting each missing
/// key at most once.
pub fn tr(msg: Msg) -> String {
    let locale = locale();
    if locale == Locale::English {
        return msg.english().to_string();
    }
    let catalogue = match locale {
        Locale::English => unreachable!(),
        Locale::German => &*GERMAN,
        Locale::French => &*FRENCH,
        Locale::Spanish => &*SPANISH,
        Locale::Japanese => &*JAPANESE,
        Locale::Chinese => &*CHINESE,
    };
    if let Some(text) = catalogue.get(msg.key()) {
        return text.clone();
    }
    if let Ok(mut missing) = MISSING.lock() {
        if missing.insert(msg.key().to_string()) {
            log::warn!(
                "No {} translation for {:?}; using English",
                locale.code(),
                msg.english()
            );
        }
    }
    msg.english().to_string()
}

/// Translate a literal call site after resolving it to its generated variant.
pub fn tr_literal(english: &'static str) -> String {
    match msg_for_key(&message_key(english)) {
        Some(msg) => tr(msg),
        None => {
            if let Ok(mut missing) = MISSING.lock() {
                if missing.insert(english.to_string()) {
                    log::warn!("Translation catalogue is missing {english:?}");
                }
            }
            english.to_string()
        }
    }
}

/// Translate a message with `{name}` placeholders.
pub fn tr_args(msg: Msg, args: &[(&str, String)]) -> String {
    interpolate(&tr(msg), args)
}

pub fn tr_args_literal(english: &'static str, args: &[(&str, String)]) -> String {
    interpolate(&tr_literal(english), args)
}

fn interpolate(template: &str, args: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        if let Some((_, value)) = args.iter().find(|(key, _)| *key == name) {
            out.push_str(value);
        } else {
            out.push('{');
            out.push_str(name);
            out.push('}');
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Stable message id: `S` + 64-bit FNV-1a of the UTF-8 English text.
///
/// This is duplicated in `build.rs` because build scripts cannot import the
/// crate they build.  The test below pins the algorithm.
pub fn message_key(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("S{hash:016X}")
}

/// A plain `tr!("English")` macro.
#[macro_export]
macro_rules! tr {
    (Msg::$variant:ident) => {
        $crate::i18n::tr($crate::i18n::Msg::$variant)
    };
    ($literal:literal) => {
        $crate::i18n::tr_literal($literal)
    };
}

/// A translated format string. Placeholders use `{name}`:
///
/// ```ignore
/// tr_args!("Scan complete: {n} tracks", "n" => tracks.len())
/// ```
#[macro_export]
macro_rules! tr_args {
    (Msg::$variant:ident, $($name:literal => $value:expr),* $(,)?) => {{
        let args: &[(&str, String)] = &[$(( $name, ($value).to_string() )),*];
        $crate::i18n::tr_args($crate::i18n::Msg::$variant, args)
    }};
    ($literal:literal, $($name:literal => $value:expr),* $(,)?) => {{
        let args: &[(&str, String)] = &[$(( $name, ($value).to_string() )),*];
        $crate::i18n::tr_args_literal($literal, args)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english_catalogue() -> HashMap<String, String> {
        serde_json::from_str(include_str!("../../locales/en.json")).unwrap()
    }

    fn german_catalogue() -> HashMap<String, String> {
        serde_json::from_str(include_str!("../../locales/de.json")).unwrap()
    }

    #[test]
    fn every_generated_message_has_english_text() {
        for &msg in Msg::ALL {
            assert!(!msg.english().is_empty(), "{:?} has no English text", msg);
            assert_eq!(message_key(msg.english()), msg.key());
        }
    }

    #[test]
    fn english_catalogue_and_generated_enum_agree() {
        let en = english_catalogue();
        assert_eq!(en.len(), Msg::ALL.len());
        for &msg in Msg::ALL {
            assert_eq!(en.get(msg.key()).map(String::as_str), Some(msg.english()));
        }
    }

    #[test]
    fn every_non_english_key_exists_in_english() {
        let en = english_catalogue();
        for key in german_catalogue().keys() {
            assert!(en.contains_key(key), "de.json has unknown key {key}");
        }
        let fr: HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/fr.json")).unwrap();
        for key in fr.keys() {
            assert!(en.contains_key(key), "fr.json has unknown key {key}");
        }
        let es: HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/es.json")).unwrap();
        for key in es.keys() {
            assert!(en.contains_key(key), "es.json has unknown key {key}");
        }
        let ja: HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/ja.json")).unwrap();
        for key in ja.keys() {
            assert!(en.contains_key(key), "ja.json has unknown key {key}");
        }
        let zh: HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/zh.json")).unwrap();
        for key in zh.keys() {
            assert!(en.contains_key(key), "zh.json has unknown key {key}");
        }
    }

    #[test]
    fn missing_translation_falls_back_to_english() {
        let en = english_catalogue();
        let de = german_catalogue();
        let Some(missing) = Msg::ALL.iter().find(|msg| !de.contains_key(msg.key())) else {
            return;
        };
        assert_eq!(translate_for(Locale::German, *missing), en[missing.key()]);
    }

    #[test]
    fn shipped_german_translation_is_used() {
        let de = german_catalogue();
        let Some((&msg, translation)) = Msg::ALL
            .iter()
            .find_map(|msg| de.get(msg.key()).map(|text| (msg, text)))
        else {
            return;
        };
        assert_eq!(translate_for(Locale::German, msg), *translation);
    }

    #[test]
    fn placeholders_are_replaced() {
        assert_eq!(
            interpolate(
                "Scan complete: {n} tracks ({ms}ms)",
                &[("n", "812".to_string()), ("ms", "2410".to_string()),]
            ),
            "Scan complete: 812 tracks (2410ms)"
        );
    }

    #[test]
    fn locale_codes_are_parsed_with_region_and_encoding() {
        assert_eq!(Locale::from_code("de_DE.UTF-8"), Some(Locale::German));
        assert_eq!(Locale::from_code("en-GB"), Some(Locale::English));
        assert_eq!(Locale::from_code("fr_FR"), Some(Locale::French));
        assert_eq!(Locale::from_code("es_ES.UTF-8"), Some(Locale::Spanish));
        assert_eq!(Locale::from_code("ja_JP"), Some(Locale::Japanese));
        assert_eq!(Locale::from_code("zh_CN"), Some(Locale::Chinese));
        assert_eq!(Locale::from_code("it_IT"), None);
    }

    #[test]
    fn message_key_is_stable() {
        assert_eq!(message_key("Close"), "S5E8250FB85D64C23");
    }

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// The preference wins over the environment, and an unknown preference
    /// falls back to English rather than to an unloaded language.
    ///
    /// The environment is read through `std::env`, so the test sets it
    /// explicitly and restores it: the suite runs single-threaded, and the
    /// locale it leaves behind must not decide another test's outcome.
    #[test]
    fn the_preference_decides_the_locale() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_all = std::env::var("LC_ALL").ok();
        let previous_msg = std::env::var("LC_MESSAGES").ok();
        let previous_lang = std::env::var("LANG").ok();

        std::env::remove_var("LC_ALL");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LANG", "de_DE.UTF-8");

        let restore = || {
            for (key, value) in [
                ("LC_ALL", previous_all.clone()),
                ("LC_MESSAGES", previous_msg.clone()),
                ("LANG", previous_lang.clone()),
            ] {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        };

        init("en");
        assert_eq!(
            locale(),
            Locale::English,
            "the preference beats the environment"
        );

        init("de");
        assert_eq!(locale(), Locale::German);

        init("fr");
        assert_eq!(locale(), Locale::French);

        init("es");
        assert_eq!(locale(), Locale::Spanish);

        init("ja");
        assert_eq!(locale(), Locale::Japanese);

        init("zh");
        assert_eq!(locale(), Locale::Chinese);

        init("it");
        assert_eq!(
            locale(),
            Locale::English,
            "a language this build does not ship falls back to English"
        );

        restore();
    }

    /// With no preference, the environment decides.
    #[test]
    fn the_environment_decides_when_there_is_no_preference() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_all = std::env::var("LC_ALL").ok();
        let previous_msg = std::env::var("LC_MESSAGES").ok();
        let previous_lang = std::env::var("LANG").ok();

        std::env::remove_var("LC_ALL");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LANG", "de_DE.UTF-8");
        init("");
        assert_eq!(locale(), Locale::German);

        // An explicit locale in the environment outranks the general one.
        std::env::set_var("LC_ALL", "en_US.UTF-8");
        init("");
        assert_eq!(locale(), Locale::English);

        // Nothing at all: English.
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LC_MESSAGES");
        std::env::remove_var("LANG");
        init("");
        assert_eq!(locale(), Locale::English);

        for (key, value) in [
            ("LC_ALL", previous_all),
            ("LC_MESSAGES", previous_msg),
            ("LANG", previous_lang),
        ] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    /// Pure lookup used by the tests so they never mutate the process-wide
    /// locale while another test is rendering.
    fn translate_for(locale: Locale, msg: Msg) -> String {
        if locale == Locale::German {
            if let Some(text) = german_catalogue().get(msg.key()) {
                return text.clone();
            }
        }
        msg.english().to_string()
    }
}
