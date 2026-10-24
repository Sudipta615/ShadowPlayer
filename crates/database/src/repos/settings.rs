//! Key/value settings repository.

use crate::db::Database;
use domain::models::AppSettings;
use rusqlite::{params, OptionalExtension, Result};

pub struct SettingsRepo {
    db: Database,
}

impl SettingsRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn load(&self) -> Result<AppSettings> {
        self.db.with_conn(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'app_settings';",
                    [],
                    |r| r.get(0),
                )
                .optional()?;

            match json {
                Some(s) => match serde_json::from_str(&s) {
                    Ok(settings) => Ok(settings),
                    Err(e) => {
                        // Do not fail the launch over unreadable settings, but
                        // make the loss visible: `AppSettings` carries
                        // `#[serde(default)]`, so a payload written by an older
                        // build with fewer fields still loads.
                        log::warn!("Ignoring unreadable app settings, using defaults: {e}");
                        Ok(AppSettings::default())
                    }
                },
                None => Ok(AppSettings::default()),
            }
        })
    }

    pub fn save(&self, settings: &AppSettings) -> Result<()> {
        // `unwrap_or_default()` here used to write an *empty string* when
        // serialization failed, which the next load then treated as "no
        // settings" — silently wiping the user's configuration.
        let json = serde_json::to_string(settings).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("could not serialize app settings: {e}"),
            )))
        })?;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('app_settings', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
                params![json],
            )?;
            Ok(())
        })
    }

    pub fn get_value(&self, key: &str) -> Result<Option<String>> {
        self.db.with_conn(|conn| {
            let val: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1;",
                    params![key],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(val)
        })
    }

    pub fn set_value(&self, key: &str, value: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
                params![key, value],
            )?;
            Ok(())
        })
    }
}
