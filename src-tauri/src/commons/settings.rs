//! Settings of the Commons export, kept in their own file (`commons.json` in
//! the app data folder) rather than in RapidRAW's settings, so that the
//! settings screen of the app cannot overwrite them with a stale copy and
//! upstream merges do not touch them.

use crate::mediawiki::model::MediaWikiFields;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CommonsSettings {
    /// Public id of the OAuth 2.0 client registered on meta.wikimedia.org
    /// (non-confidential, so there is no secret to keep).
    pub client_id: String,
    pub edit_summary: String,
    /// Category added to every upload; empty for none. Must exist on Commons.
    pub tracking_category: String,
    /// Upload even if Commons warns (for instance that the file exists).
    pub ignore_warnings: bool,
    /// Statements derived from the file: inception date, media type, and
    /// "source of file" for own work (Cammello's "capture settings" switch).
    pub structured_capture: bool,
    /// Values for the whole upload set; an image's own values win.
    pub defaults: MediaWikiFields,
}

impl Default for CommonsSettings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            edit_summary: "Uploaded with RapidRAW".to_string(),
            tracking_category: String::new(),
            ignore_warnings: false,
            structured_capture: true,
            defaults: MediaWikiFields::default(),
        }
    }
}

impl CommonsSettings {
    /// Trim and check; every problem is reported.
    pub fn normalize(&mut self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        self.client_id = self.client_id.trim().to_string();
        if !self.client_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            errors.push(
                "clientId: only letters and digits (copy it from the OAuth registration)"
                    .to_string(),
            );
        }
        self.edit_summary = self.edit_summary.trim().to_string();
        if self.edit_summary.is_empty() {
            self.edit_summary = Self::default().edit_summary;
        }
        let category = self.tracking_category.trim();
        self.tracking_category = category
            .strip_prefix("Category:")
            .unwrap_or(category)
            .trim()
            .to_string();
        if let Err(mut problems) = self.defaults.normalize() {
            errors.append(&mut problems);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// The settings in `file`; defaults if it is missing or unreadable (a broken
/// file must not stop an export, and is reported in the log).
pub fn load_from(file: &Path) -> CommonsSettings {
    match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            log::warn!("Could not read {}: {e}", file.display());
            CommonsSettings::default()
        }),
        Err(_) => CommonsSettings::default(),
    }
}

/// Write through a temporary file, so a crash cannot leave half a file.
pub fn save_to(file: &Path, settings: &CommonsSettings) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let temporary = file.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_broken_files_give_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("commons.json");
        assert_eq!(load_from(&file), CommonsSettings::default());
        std::fs::write(&file, "{ nope").unwrap();
        assert_eq!(load_from(&file), CommonsSettings::default());
    }

    #[test]
    fn round_trip_and_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub").join("commons.json");
        let mut settings = CommonsSettings {
            client_id: "abc123".into(),
            tracking_category: "Uploaded with RapidRAW".into(),
            ..Default::default()
        };
        settings.defaults.author = "Harald".into();
        save_to(&file, &settings).unwrap();
        assert_eq!(load_from(&file), settings);
        assert!(!file.with_extension("json.tmp").exists());

        std::fs::write(&file, r#"{"clientId":"x1"}"#).unwrap();
        let partial = load_from(&file);
        assert_eq!(partial.client_id, "x1");
        assert_eq!(partial.edit_summary, "Uploaded with RapidRAW");
        assert!(partial.structured_capture);
    }

    #[test]
    fn normalizing() {
        let mut settings = CommonsSettings {
            client_id: "  abc123 ".into(),
            edit_summary: "  ".into(),
            tracking_category: "Category: Uploaded with RapidRAW ".into(),
            ..Default::default()
        };
        settings.defaults.depicts = vec!["q5".into()];
        settings.normalize().unwrap();
        assert_eq!(settings.client_id, "abc123");
        assert_eq!(settings.edit_summary, "Uploaded with RapidRAW");
        assert_eq!(settings.tracking_category, "Uploaded with RapidRAW");
        assert_eq!(settings.defaults.depicts, vec!["Q5"]);

        let mut bad = CommonsSettings {
            client_id: "not valid!".into(),
            ..Default::default()
        };
        bad.defaults.creator = "Harald".into();
        assert_eq!(
            bad.normalize().unwrap_err().len(),
            2,
            "both problems reported"
        );
    }
}
