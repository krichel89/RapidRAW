//! MediaWiki metadata per image: data model, validation, merging and patching.
//!
//! Deliberately pure (serde only, no Tauri) so it can be unit-tested on its own
//! and stays out of the way of upstream merges. The field set and the merge
//! rules mirror the MediaWiki tab of Cammello (`workflow_config.FIELDS`,
//! `sdc.merge_descriptions`), so values mean the same in both tools.
//!
//! One struct serves two levels, as in Cammello:
//!
//! * per image: stored in the sidecar (`ImageMetadata::media_wiki`)
//! * defaults: values for a whole upload set (app settings, later)
//!
//! [`MediaWikiFields::merge`] combines the two; the image wins.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Wikimedia Commons caption limit (structured data, plain text).
pub const MAX_CAPTION_CHARS: usize = 250;

/// Every serialized field name. A unit test keeps this list and the struct in
/// step, so a field added to one cannot be forgotten in the other.
pub const ALL_KEYS: &[&str] = &[
    "targetFilename",
    "date",
    "description",
    "captions",
    "templates",
    "baseDescription",
    "author",
    "photographerCategory",
    "creator",
    "source",
    "permission",
    "license",
    "copyright",
    "depicts",
    "depictsOverride",
    "ifNoDepicts",
    "categories",
    "createdDuring",
    "cameraPosition",
    "objectPosition",
    "galleryPage",
    "gallerySuffix",
    "extraWikitext",
    "statements",
    "composer",
    "composerDied",
    "compositionYear",
    "compositionLicense",
    "work",
    "recordedBy",
    "recordingTechnique",
    "sourceTemplate",
    "recordingLicense",
    "otherVersions",
    "instrument",
    "period",
    "country",
];

/// Fields that only ever come from the defaults; a per-image value is ignored
/// (Cammello: `_BASE_ONLY_KEYS`, plus the gallery page that belongs with it).
const BASE_ONLY: &[&str] = &["gallerySuffix", "galleryPage"];

// serde's `skip_serializing_if` passes `&String`, so the signature cannot be `&str`.
#[allow(clippy::ptr_arg)]
fn is_empty_text(s: &String) -> bool {
    s.is_empty()
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MediaWikiFields {
    // ── Per file ──────────────────────────────────────────────────────────
    /// Target filename on Commons.
    #[serde(skip_serializing_if = "is_empty_text")]
    pub target_filename: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub date: String,
    /// Description of this file (added after the base description).
    #[serde(skip_serializing_if = "is_empty_text")]
    pub description: String,
    /// Structured-data captions, language code -> plain text (max 250 chars).
    /// In the defaults these are the captions for all files.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub captions: BTreeMap<String, String>,

    // ── Description block ────────────────────────────────────────────────
    /// Other templates (wikitext).
    #[serde(skip_serializing_if = "is_empty_text")]
    pub templates: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub base_description: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub author: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub photographer_category: String,
    /// Creator (P170), one Q-number.
    #[serde(skip_serializing_if = "is_empty_text")]
    pub creator: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub source: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub permission: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub license: String,
    /// Copyright status (P6216), one Q-number.
    #[serde(skip_serializing_if = "is_empty_text")]
    pub copyright: String,

    // ── Structured data ─────────────────────────────────────────────────
    /// Depicts (P180), Q-numbers. Merged (union) with the defaults.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub depicts: Vec<String>,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub depicts_override: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub if_no_depicts: String,
    /// Commons categories (names without the `Category:` prefix).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// Created during (P10408), one Q-number.
    #[serde(skip_serializing_if = "is_empty_text")]
    pub created_during: String,
    /// Camera position, `lat, lon` in decimal degrees.
    #[serde(skip_serializing_if = "is_empty_text")]
    pub camera_position: String,
    /// Position of the depicted object, `lat, lon` in decimal degrees.
    #[serde(skip_serializing_if = "is_empty_text")]
    pub object_position: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub gallery_page: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub gallery_suffix: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub extra_wikitext: String,
    /// Fixed structured-data claims, property (`P123`) -> value.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub statements: BTreeMap<String, String>,

    // ── Music and audio ──────────────────────────────────────────────────
    #[serde(skip_serializing_if = "is_empty_text")]
    pub composer: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub composer_died: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub composition_year: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub composition_license: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub work: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub recorded_by: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub recording_technique: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub source_template: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub recording_license: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub other_versions: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub instrument: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub period: String,
    #[serde(skip_serializing_if = "is_empty_text")]
    pub country: String,
}

// ── Validation helpers ────────────────────────────────────────────────────

fn is_qid(s: &str) -> bool {
    let digits = match s.strip_prefix('Q') {
        Some(d) => d,
        None => return false,
    };
    !digits.is_empty()
        && !digits.starts_with('0')
        && digits.len() <= 13
        && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `q42`, ` Q42 # Berlin` -> `Q42`. Anything after `#` is a comment.
fn clean_qid(raw: &str) -> Option<String> {
    let token = raw.split('#').next().unwrap_or("").trim();
    let mut chars = token.chars();
    let first = chars.next()?;
    let candidate = format!("{}{}", first.to_ascii_uppercase(), chars.as_str());
    is_qid(&candidate).then_some(candidate)
}

fn is_property_id(s: &str) -> bool {
    match s.strip_prefix('P') {
        Some(d) => {
            !d.is_empty()
                && !d.starts_with('0')
                && d.len() <= 10
                && d.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// `de`, `pt-br`, `zh-hans`: lowercase language code, optional subtags.
fn is_language_code(s: &str) -> bool {
    let mut parts = s.split('-');
    let primary_ok = parts
        .next()
        .is_some_and(|p| (2..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_lowercase()));
    primary_ok
        && parts.all(|p| {
            (1..=8).contains(&p.len())
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// `52.52, 13.405` -> normalized `52.52, 13.405`, or `None` if malformed or
/// out of range.
fn normalize_position(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 2 {
        return None;
    }
    let lat: f64 = parts[0].parse().ok()?;
    let lon: f64 = parts[1].parse().ok()?;
    if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
        return None;
    }
    Some(format!("{lat}, {lon}"))
}

fn split_list(items: &[String]) -> Vec<String> {
    items
        .iter()
        .flat_map(|item| item.split([';', ',', '\n']))
        .map(|p| p.to_string())
        .collect()
}

impl MediaWikiFields {
    fn text_fields_mut(&mut self) -> Vec<&mut String> {
        vec![
            &mut self.target_filename,
            &mut self.date,
            &mut self.description,
            &mut self.templates,
            &mut self.base_description,
            &mut self.author,
            &mut self.photographer_category,
            &mut self.creator,
            &mut self.source,
            &mut self.permission,
            &mut self.license,
            &mut self.copyright,
            &mut self.depicts_override,
            &mut self.if_no_depicts,
            &mut self.created_during,
            &mut self.camera_position,
            &mut self.object_position,
            &mut self.gallery_page,
            &mut self.gallery_suffix,
            &mut self.extra_wikitext,
            &mut self.composer,
            &mut self.composer_died,
            &mut self.composition_year,
            &mut self.composition_license,
            &mut self.work,
            &mut self.recorded_by,
            &mut self.recording_technique,
            &mut self.source_template,
            &mut self.recording_license,
            &mut self.other_versions,
            &mut self.instrument,
            &mut self.period,
            &mut self.country,
        ]
    }

    /// Trim, drop empties and duplicates, check the values that Wikimedia
    /// would reject. On error `self` is left as it was; every problem found is
    /// reported, not just the first.
    pub fn normalize(&mut self) -> Result<(), Vec<String>> {
        let mut next = self.clone();
        let mut errors = Vec::new();

        for field in next.text_fields_mut() {
            let trimmed = field.trim();
            if trimmed.len() != field.len() {
                *field = trimmed.to_string();
            }
        }

        for (name, value) in [
            ("creator", &mut next.creator),
            ("copyright", &mut next.copyright),
            ("createdDuring", &mut next.created_during),
        ] {
            if value.is_empty() {
                continue;
            }
            match clean_qid(value) {
                Some(qid) => *value = qid,
                None => errors.push(format!("{name}: \"{value}\" is not a Q-number")),
            }
        }

        let mut depicts: Vec<String> = Vec::new();
        for token in split_list(&next.depicts) {
            if token.split('#').next().unwrap_or("").trim().is_empty() {
                continue;
            }
            match clean_qid(&token) {
                Some(qid) => {
                    if !depicts.contains(&qid) {
                        depicts.push(qid);
                    }
                }
                None => errors.push(format!("depicts: \"{}\" is not a Q-number", token.trim())),
            }
        }
        next.depicts = depicts;

        let mut categories: Vec<String> = Vec::new();
        for category in &next.categories {
            let name = category.trim();
            let name = name
                .strip_prefix("Category:")
                .or_else(|| name.strip_prefix("category:"))
                .unwrap_or(name)
                .trim()
                .replace('_', " ");
            if !name.is_empty() && !categories.contains(&name) {
                categories.push(name);
            }
        }
        next.categories = categories;

        for (name, value) in [
            ("cameraPosition", &mut next.camera_position),
            ("objectPosition", &mut next.object_position),
        ] {
            if value.is_empty() {
                continue;
            }
            match normalize_position(value) {
                Some(position) => *value = position,
                None => errors.push(format!(
                    "{name}: \"{value}\" is not \"latitude, longitude\" in decimal degrees"
                )),
            }
        }

        let mut captions = BTreeMap::new();
        for (language, text) in &next.captions {
            let language = language.trim().to_ascii_lowercase();
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            if !is_language_code(&language) {
                errors.push(format!("captions: \"{language}\" is not a language code"));
            } else if text.contains(['\n', '\r']) {
                errors.push(format!("captions[{language}]: must be a single line"));
            } else if text.chars().count() > MAX_CAPTION_CHARS {
                errors.push(format!(
                    "captions[{language}]: longer than {MAX_CAPTION_CHARS} characters"
                ));
            } else {
                captions.insert(language, text.to_string());
            }
        }
        next.captions = captions;

        let mut statements = BTreeMap::new();
        for (property, value) in &next.statements {
            let property = property.trim().to_ascii_uppercase();
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            if is_property_id(&property) {
                statements.insert(property, value.to_string());
            } else {
                errors.push(format!(
                    "statements: \"{property}\" is not a property (P123)"
                ));
            }
        }
        next.statements = statements;

        if errors.is_empty() {
            *self = next;
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn is_empty(&self) -> bool {
        to_map(self).is_empty()
    }

    /// The effective values for one image: `file` on top of `base`.
    ///
    /// Rules (Cammello `merge_descriptions`):
    ///   * depicts            - union, base first, duplicates removed
    ///   * captions           - per language, the file wins
    ///   * statements         - per property, the file wins
    ///   * gallery fields     - base only, a per-image value is ignored
    ///   * everything else    - a non-empty file value replaces the base value
    ///
    /// Descriptions are not merged here; see [`merged_description`].
    pub fn merge(base: &Self, file: &Self) -> Self {
        let mut out = to_map(base);
        for (key, value) in to_map(file) {
            if BASE_ONLY.contains(&key.as_str()) {
                continue;
            }
            match key.as_str() {
                "depicts" => {
                    let mut union: Vec<Value> = out
                        .get("depicts")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    for item in value.as_array().into_iter().flatten() {
                        if !union.contains(item) {
                            union.push(item.clone());
                        }
                    }
                    out.insert(key, Value::Array(union));
                }
                "captions" | "statements" => {
                    let mut merged = out
                        .get(&key)
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(overlay) = value.as_object() {
                        for (k, v) in overlay {
                            merged.insert(k.clone(), v.clone());
                        }
                    }
                    out.insert(key, Value::Object(merged));
                }
                _ => {
                    out.insert(key, value);
                }
            }
        }
        serde_json::from_value(Value::Object(out)).unwrap_or_else(|_| base.clone())
    }

    /// Change only the fields named in `patch` (a JSON object with the same
    /// camelCase names). `null`, `""`, `[]` and `{}` clear a field. The patch
    /// is validated as a whole; on error nothing changes.
    pub fn apply_patch(&mut self, patch: &Value) -> Result<(), Vec<String>> {
        let object = patch
            .as_object()
            .ok_or_else(|| vec!["patch must be a JSON object".to_string()])?;

        let unknown: Vec<String> = object
            .keys()
            .filter(|k| !ALL_KEYS.contains(&k.as_str()))
            .map(|k| format!("unknown field \"{k}\""))
            .collect();
        if !unknown.is_empty() {
            return Err(unknown);
        }

        // `null` means "clear", which the typed struct cannot hold.
        let typed_input: Map<String, Value> = object
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut typed: MediaWikiFields = serde_json::from_value(Value::Object(typed_input))
            .map_err(|e| vec![format!("invalid value: {e}")])?;
        typed.normalize()?;

        let normalized = to_map(&typed);
        let mut current = to_map(self);
        for key in object.keys() {
            match normalized.get(key) {
                Some(value) => {
                    current.insert(key.clone(), value.clone());
                }
                None => {
                    current.remove(key);
                }
            }
        }
        *self = serde_json::from_value(Value::Object(current))
            .map_err(|e| vec![format!("invalid result: {e}")])?;
        Ok(())
    }
}

/// The description of one image as it will be written: the base description
/// first, then the image's own, joined by a line break (Cammello: base text,
/// then file text).
pub fn merged_description(base: &MediaWikiFields, file: &MediaWikiFields) -> String {
    [base.base_description.trim(), file.description.trim()]
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Only the non-empty fields, as a JSON object.
fn to_map(fields: &MediaWikiFields) -> Map<String, Value> {
    match serde_json::to_value(fields) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fully_populated() -> MediaWikiFields {
        let s = |t: &str| t.to_string();
        MediaWikiFields {
            target_filename: s("a.jpg"),
            date: s("2026-02-20"),
            description: s("d"),
            captions: BTreeMap::from([(s("de"), s("x"))]),
            templates: s("t"),
            base_description: s("b"),
            author: s("a"),
            photographer_category: s("p"),
            creator: s("Q1"),
            source: s("s"),
            permission: s("pe"),
            license: s("l"),
            copyright: s("Q2"),
            depicts: vec![s("Q3")],
            depicts_override: s("o"),
            if_no_depicts: s("n"),
            categories: vec![s("C")],
            created_during: s("Q4"),
            camera_position: s("1, 2"),
            object_position: s("3, 4"),
            gallery_page: s("g"),
            gallery_suffix: s("gs"),
            extra_wikitext: s("e"),
            statements: BTreeMap::from([(s("P1"), s("v"))]),
            composer: s("c"),
            composer_died: s("1847"),
            composition_year: s("1845"),
            composition_license: s("{{PD}}"),
            work: s("w"),
            recorded_by: s("r"),
            recording_technique: s("t"),
            source_template: s("st"),
            recording_license: s("rl"),
            other_versions: s("ov"),
            instrument: s("organ"),
            period: s("Romantic"),
            country: s("Germany"),
        }
    }

    #[test]
    fn all_keys_matches_struct() {
        let value = serde_json::to_value(fully_populated()).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        let mut expected: Vec<&str> = ALL_KEYS.to_vec();
        keys.sort_unstable();
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert_eq!(ALL_KEYS.len(), 37);
    }

    #[test]
    fn text_fields_mut_covers_every_text_field() {
        // 37 keys = 33 text fields + captions + statements + depicts + categories
        let mut f = MediaWikiFields::default();
        assert_eq!(f.text_fields_mut().len(), 33);
    }

    #[test]
    fn empty_and_old_sidecars_deserialize() {
        let empty: MediaWikiFields = serde_json::from_str("{}").unwrap();
        assert!(empty.is_empty());
        let with_unknown: MediaWikiFields =
            serde_json::from_str(r#"{"author":"H","somethingNew":1}"#).unwrap();
        assert_eq!(with_unknown.author, "H");
    }

    #[test]
    fn empty_fields_are_not_serialized() {
        assert_eq!(
            serde_json::to_string(&MediaWikiFields::default()).unwrap(),
            "{}"
        );
    }

    #[test]
    fn serde_round_trip() {
        let original = fully_populated();
        let back: MediaWikiFields =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn depicts_are_cleaned() {
        let mut f = MediaWikiFields {
            depicts: vec!["q42 # Berlin; Q7, Q42".into(), "  ".into()],
            ..Default::default()
        };
        f.normalize().unwrap();
        assert_eq!(f.depicts, vec!["Q42", "Q7"]);
    }

    #[test]
    fn invalid_values_are_reported_together_and_change_nothing() {
        let mut f = MediaWikiFields {
            depicts: vec!["Berlin".into()],
            creator: "nope".into(),
            camera_position: "91, 0".into(),
            author: "  keep me  ".into(),
            ..Default::default()
        };
        let errors = f.normalize().unwrap_err();
        assert_eq!(errors.len(), 3);
        assert_eq!(f.author, "  keep me  ", "unchanged on error");
    }

    #[test]
    fn qid_rules() {
        assert!(is_qid("Q1"));
        assert!(is_qid("Q123456"));
        assert!(!is_qid("Q0"));
        assert!(!is_qid("Q042"));
        assert!(!is_qid("Q"));
        assert!(!is_qid("q1"));
        assert!(!is_qid("Q1a"));
        assert_eq!(clean_qid(" q73566113 # CC ").as_deref(), Some("Q73566113"));
        assert_eq!(clean_qid("# only a comment"), None);
    }

    #[test]
    fn positions() {
        assert_eq!(
            normalize_position("52.52 , 13.405").as_deref(),
            Some("52.52, 13.405")
        );
        assert_eq!(
            normalize_position("-33.9;151.2").as_deref(),
            Some("-33.9, 151.2")
        );
        assert_eq!(normalize_position("52.52").as_deref(), None);
        assert_eq!(normalize_position("1, 2, 3").as_deref(), None);
        assert_eq!(normalize_position("90.1, 0").as_deref(), None);
        assert_eq!(normalize_position("0, 180.5").as_deref(), None);
        assert_eq!(normalize_position("a, b").as_deref(), None);
        assert_eq!(normalize_position("NaN, 1").as_deref(), None);
    }

    #[test]
    fn captions_are_checked() {
        let mut ok = MediaWikiFields {
            captions: BTreeMap::from([
                ("DE".into(), " Hallo ".into()),
                ("pt-br".into(), "Olá".into()),
                ("fr".into(), "   ".into()),
            ]),
            ..Default::default()
        };
        ok.normalize().unwrap();
        assert_eq!(ok.captions.len(), 2);
        assert_eq!(ok.captions["de"], "Hallo");

        let long = "ä".repeat(MAX_CAPTION_CHARS + 1);
        let exact = "ä".repeat(MAX_CAPTION_CHARS);
        let mut too_long = MediaWikiFields {
            captions: BTreeMap::from([("de".into(), long)]),
            ..Default::default()
        };
        assert!(too_long.normalize().is_err());
        let mut at_limit = MediaWikiFields {
            captions: BTreeMap::from([("de".into(), exact)]),
            ..Default::default()
        };
        assert!(
            at_limit.normalize().is_ok(),
            "counted in characters, not bytes"
        );

        let mut multiline = MediaWikiFields {
            captions: BTreeMap::from([("de".into(), "a\nb".into())]),
            ..Default::default()
        };
        assert!(multiline.normalize().is_err());
        let mut bad_lang = MediaWikiFields {
            captions: BTreeMap::from([("Deutsch".into(), "a".into())]),
            ..Default::default()
        };
        assert!(bad_lang.normalize().is_err());
    }

    #[test]
    fn categories_lose_prefix_underscores_and_duplicates() {
        let mut f = MediaWikiFields {
            categories: vec![
                "Category:Films_by_Harald Krichel, Berlin".into(),
                "Films by Harald Krichel, Berlin".into(),
                " ".into(),
            ],
            ..Default::default()
        };
        f.normalize().unwrap();
        assert_eq!(f.categories, vec!["Films by Harald Krichel, Berlin"]);
    }

    #[test]
    fn statements_need_property_ids() {
        let mut ok = MediaWikiFields {
            statements: BTreeMap::from([("p1435".into(), "Q123".into())]),
            ..Default::default()
        };
        ok.normalize().unwrap();
        assert!(ok.statements.contains_key("P1435"));
        let mut bad = MediaWikiFields {
            statements: BTreeMap::from([("Q1".into(), "x".into())]),
            ..Default::default()
        };
        assert!(bad.normalize().is_err());
    }

    #[test]
    fn merge_follows_cammello_rules() {
        let base = MediaWikiFields {
            depicts: vec!["Q1".into(), "Q2".into()],
            created_during: "Q10".into(),
            license: "{{Cc-by-sa-4.0}}".into(),
            author: "Base".into(),
            gallery_suffix: "Berlinale 2026".into(),
            captions: BTreeMap::from([
                ("en".into(), "base en".into()),
                ("de".into(), "base de".into()),
            ]),
            statements: BTreeMap::from([("P1".into(), "a".into()), ("P2".into(), "b".into())]),
            ..Default::default()
        };
        let file = MediaWikiFields {
            depicts: vec!["Q2".into(), "Q3".into()],
            created_during: "Q11".into(),
            gallery_suffix: "ignored".into(),
            gallery_page: "ignored".into(),
            captions: BTreeMap::from([("de".into(), "file de".into())]),
            statements: BTreeMap::from([("P2".into(), "B".into())]),
            ..Default::default()
        };
        let merged = MediaWikiFields::merge(&base, &file);
        assert_eq!(merged.depicts, vec!["Q1", "Q2", "Q3"]);
        assert_eq!(merged.created_during, "Q11", "file wins");
        assert_eq!(
            merged.license, "{{Cc-by-sa-4.0}}",
            "empty file value keeps base"
        );
        assert_eq!(merged.author, "Base");
        assert_eq!(merged.gallery_suffix, "Berlinale 2026", "base only");
        assert_eq!(merged.gallery_page, "", "base only, even if base empty");
        assert_eq!(merged.captions["en"], "base en");
        assert_eq!(merged.captions["de"], "file de");
        assert_eq!(merged.statements["P1"], "a");
        assert_eq!(merged.statements["P2"], "B");
    }

    #[test]
    fn merge_with_empty_sides() {
        let some = fully_populated();
        assert_eq!(
            MediaWikiFields::merge(&some, &MediaWikiFields::default()),
            some
        );
        let only_file = MediaWikiFields {
            author: "F".into(),
            ..Default::default()
        };
        assert_eq!(
            MediaWikiFields::merge(&MediaWikiFields::default(), &only_file).author,
            "F"
        );
    }

    #[test]
    fn description_is_base_then_file() {
        let base = MediaWikiFields {
            base_description: " Base ".into(),
            ..Default::default()
        };
        let file = MediaWikiFields {
            description: "File".into(),
            ..Default::default()
        };
        assert_eq!(merged_description(&base, &file), "Base\nFile");
        assert_eq!(
            merged_description(&base, &MediaWikiFields::default()),
            "Base"
        );
        assert_eq!(
            merged_description(&MediaWikiFields::default(), &file),
            "File"
        );
        assert_eq!(
            merged_description(&MediaWikiFields::default(), &MediaWikiFields::default()),
            ""
        );
    }

    #[test]
    fn patch_changes_only_named_fields() {
        let mut f = MediaWikiFields {
            author: "Keep".into(),
            depicts: vec!["Q1".into()],
            ..Default::default()
        };
        f.apply_patch(&json!({"source": "  own work ", "depicts": ["q2; Q3"]}))
            .unwrap();
        assert_eq!(f.author, "Keep");
        assert_eq!(f.source, "own work");
        assert_eq!(f.depicts, vec!["Q2", "Q3"]);
    }

    #[test]
    fn patch_clears_with_null_empty_string_and_empty_list() {
        let mut f = fully_populated();
        f.apply_patch(&json!({"author": null, "source": "", "depicts": [], "captions": {}}))
            .unwrap();
        assert_eq!(f.author, "");
        assert_eq!(f.source, "");
        assert!(f.depicts.is_empty());
        assert!(f.captions.is_empty());
        assert_eq!(f.license, "l", "untouched");
    }

    #[test]
    fn patch_replaces_whole_captions_map() {
        let mut f = MediaWikiFields {
            captions: BTreeMap::from([("de".into(), "a".into()), ("en".into(), "b".into())]),
            ..Default::default()
        };
        f.apply_patch(&json!({"captions": {"fr": "c"}})).unwrap();
        assert_eq!(f.captions.len(), 1);
        assert_eq!(f.captions["fr"], "c");
    }

    #[test]
    fn bad_patch_changes_nothing() {
        let mut f = MediaWikiFields {
            author: "Keep".into(),
            ..Default::default()
        };
        let before = f.clone();
        assert!(
            f.apply_patch(&json!({"author": "New", "creator": "oops"}))
                .is_err()
        );
        assert_eq!(f, before);
        assert!(f.apply_patch(&json!({"nonsense": 1})).is_err());
        assert!(f.apply_patch(&json!("not an object")).is_err());
        assert!(f.apply_patch(&json!({"depicts": "not a list"})).is_err());
        assert_eq!(f, before);
    }

    #[test]
    fn clearing_everything_makes_the_record_empty() {
        let mut f = fully_populated();
        let all_null: Map<String, Value> = ALL_KEYS
            .iter()
            .map(|k| (k.to_string(), Value::Null))
            .collect();
        f.apply_patch(&Value::Object(all_null)).unwrap();
        assert!(f.is_empty());
    }
}
