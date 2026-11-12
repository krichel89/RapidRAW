//! Structured data on Commons (labels and statements of the file's MediaInfo
//! entity), built from the effective metadata of an image.
//!
//! The property numbers are the ones Cammello uses (`constants.py`,
//! `PROPERTY_MAP`); they were checked on wikidata.org there. The result is the
//! `data` parameter of `wbeditentity`.

use crate::mediawiki::model::MediaWikiFields;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const CREATOR: &str = "P170";
const COPYRIGHT_STATUS: &str = "P6216";
const LICENSE: &str = "P275";
const DEPICTS: &str = "P180";
const CREATED_DURING: &str = "P10408";
const CAMERA_POSITION: &str = "P1259";
const OBJECT_POSITION: &str = "P9149";
const INCEPTION: &str = "P571";
const MEDIA_TYPE: &str = "P1163";
const SOURCE_OF_FILE: &str = "P7482";
const OWN_WORK: &str = "Q66458942";

/// Licence template → licence item (P275), the presets of Cammello. Only a
/// licence that is exactly one of these gets a statement; any other text
/// would need a guess.
const LICENSES: &[(&str, &str)] = &[
    ("{{cc-zero}}", "Q6938433"),
    ("{{cc-by-4.0}}", "Q20007257"),
    ("{{cc-by-sa-4.0}}", "Q18199165"),
];

/// Source texts that unambiguously mean "own work" (Cammello `OWN_WORK_TEMPLATES`).
const OWN_WORK_TEXTS: &[&str] = &["own", "own work", "self-photographed", "own photograph"];

#[derive(Debug, Clone, PartialEq)]
pub enum ClaimValue {
    Item(String),
    /// Latitude, longitude.
    Coordinate(f64, f64),
    /// `YYYY-MM-DD`, written with day precision.
    Day(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    pub property: String,
    pub value: ClaimValue,
}

#[derive(Debug, Default, PartialEq)]
pub struct StructuredData {
    /// Language code → caption.
    pub labels: Vec<(String, String)>,
    pub claims: Vec<Claim>,
}

fn claim(property: &str, value: ClaimValue) -> Claim {
    Claim {
        property: property.to_string(),
        value,
    }
}

fn is_day(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let number = |from: usize, to: usize| text[from..to].parse::<u32>().ok();
    matches!((number(5, 7), number(8, 10)), (Some(1..=12), Some(1..=31)))
}

fn mime_for_extension(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "tif" | "tiff" => Some("image/tiff"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn is_qid(text: &str) -> bool {
    text.strip_prefix('Q').is_some_and(|d| {
        !d.is_empty() && !d.starts_with('0') && d.bytes().all(|b| b.is_ascii_digit())
    })
}

/// `statements` of the metadata: text that is only Q-numbers separated by `;`
/// gives one item claim each, anything else is one string claim (a URL or an
/// identifier may well contain `;`). Nothing is checked against the
/// property's datatype; Wikibase does that and answers with an error.
fn statement_claims(fields: &MediaWikiFields, taken: &BTreeSet<String>) -> Vec<Claim> {
    let mut out = Vec::new();
    for (property, text) in &fields.statements {
        if taken.contains(property) || text.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = text
            .split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if !parts.is_empty() && parts.iter().all(|p| is_qid(p)) {
            let mut seen = BTreeSet::new();
            for qid in parts {
                if seen.insert(qid) {
                    out.push(claim(property, ClaimValue::Item(qid.to_string())));
                }
            }
        } else {
            out.push(claim(property, ClaimValue::Text(text.trim().to_string())));
        }
    }
    out
}

/// Labels and statements of one upload.
///
/// `file_extension` is the extension of the uploaded file (for the media
/// type), `with_capture` switches the statements derived from the file itself
/// (inception, media type, source of file) as Cammello's setting does.
pub fn build(fields: &MediaWikiFields, file_extension: &str, with_capture: bool) -> StructuredData {
    let mut data = StructuredData {
        labels: fields
            .captions
            .iter()
            .filter(|(_, text)| !text.is_empty())
            .map(|(language, text)| (language.clone(), text.clone()))
            .collect(),
        claims: Vec::new(),
    };

    for (property, value) in [
        (CREATOR, &fields.creator),
        (COPYRIGHT_STATUS, &fields.copyright),
        (CREATED_DURING, &fields.created_during),
    ] {
        if is_qid(value) {
            data.claims
                .push(claim(property, ClaimValue::Item(value.clone())));
        }
    }

    let license_key: String = fields
        .license
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();
    if let Some((_, item)) = LICENSES
        .iter()
        .find(|(template, _)| *template == license_key)
    {
        data.claims
            .push(claim(LICENSE, ClaimValue::Item(item.to_string())));
    }

    for qid in &fields.depicts {
        data.claims
            .push(claim(DEPICTS, ClaimValue::Item(qid.clone())));
    }

    for (property, text) in [
        (CAMERA_POSITION, &fields.camera_position),
        (OBJECT_POSITION, &fields.object_position),
    ] {
        if let Some((lat, lon)) = super::wikitext::parse_position(text) {
            data.claims
                .push(claim(property, ClaimValue::Coordinate(lat, lon)));
        }
    }

    if with_capture {
        let day = fields.date.get(..10).unwrap_or("");
        if is_day(day) {
            data.claims
                .push(claim(INCEPTION, ClaimValue::Day(day.to_string())));
        }
        if let Some(mime) = mime_for_extension(file_extension) {
            data.claims
                .push(claim(MEDIA_TYPE, ClaimValue::Text(mime.to_string())));
        }
        let source = fields
            .source
            .trim()
            .trim_matches(['{', '}', ' '])
            .to_lowercase();
        if OWN_WORK_TEXTS.contains(&source.as_str()) {
            data.claims.push(claim(
                SOURCE_OF_FILE,
                ClaimValue::Item(OWN_WORK.to_string()),
            ));
        }
    }

    // Last, so that a property the fields already produced wins.
    let taken: BTreeSet<String> = data.claims.iter().map(|c| c.property.clone()).collect();
    data.claims.extend(statement_claims(fields, &taken));
    data
}

impl Claim {
    fn to_json(&self) -> Option<Value> {
        let datavalue = match &self.value {
            ClaimValue::Item(qid) => {
                let numeric: u64 = qid.strip_prefix('Q')?.parse().ok()?;
                json!({
                    "type": "wikibase-entityid",
                    "value": { "entity-type": "item", "numeric-id": numeric, "id": qid },
                })
            }
            ClaimValue::Coordinate(latitude, longitude) => json!({
                "type": "globecoordinate",
                "value": {
                    "latitude": latitude,
                    "longitude": longitude,
                    "altitude": null,
                    // 1e-6 degrees is about 0.1 m, what Commons uses for camera positions.
                    "precision": 1e-6,
                    "globe": "http://www.wikidata.org/entity/Q2",
                },
            }),
            ClaimValue::Day(day) => json!({
                "type": "time",
                "value": {
                    "time": format!("+{day}T00:00:00Z"),
                    "timezone": 0, "before": 0, "after": 0,
                    "precision": 11,
                    "calendarmodel": "http://www.wikidata.org/entity/Q1985727",
                },
            }),
            ClaimValue::Text(text) => json!({ "type": "string", "value": text }),
        };
        Some(json!({
            "mainsnak": {
                "snaktype": "value",
                "property": self.property,
                "datavalue": datavalue,
            },
            "type": "statement",
            "rank": "normal",
        }))
    }
}

impl StructuredData {
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty() && self.claims.is_empty()
    }

    pub fn has_depicts(&self) -> bool {
        self.claims.iter().any(|c| c.property == DEPICTS)
    }

    /// The `data` parameter of `wbeditentity`.
    pub fn to_json(&self) -> Value {
        let mut data = serde_json::Map::new();
        if !self.labels.is_empty() {
            let labels: serde_json::Map<String, Value> = self
                .labels
                .iter()
                .map(|(language, text)| {
                    (
                        language.clone(),
                        json!({ "language": language, "value": text }),
                    )
                })
                .collect();
            data.insert("labels".into(), Value::Object(labels));
        }
        let claims: Vec<Value> = self.claims.iter().filter_map(Claim::to_json).collect();
        if !claims.is_empty() {
            data.insert("claims".into(), Value::Array(claims));
        }
        Value::Object(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn props(data: &StructuredData) -> Vec<&str> {
        data.claims.iter().map(|c| c.property.as_str()).collect()
    }

    #[test]
    fn fields_become_claims() {
        let fields = MediaWikiFields {
            creator: "Q1".into(),
            copyright: "Q73566113".into(),
            created_during: "Q2".into(),
            license: "{{ Cc-by-sa-4.0 }}".into(),
            depicts: vec!["Q10".into(), "Q11".into()],
            camera_position: "52.5, 13.4".into(),
            object_position: "52.6, 13.5".into(),
            captions: BTreeMap::from([("de".into(), "Anna Meier".into())]),
            ..Default::default()
        };
        let data = build(&fields, "jpg", false);
        assert_eq!(
            props(&data),
            [
                "P170", "P6216", "P10408", "P275", "P180", "P180", "P1259", "P9149"
            ]
        );
        assert_eq!(
            data.labels,
            vec![("de".to_string(), "Anna Meier".to_string())]
        );
        assert!(data.has_depicts());
    }

    #[test]
    fn capture_statements_are_switchable() {
        let fields = MediaWikiFields {
            date: "2026-02-20 14:03:11".into(),
            source: "{{own}}".into(),
            ..Default::default()
        };
        assert!(build(&fields, "jpg", false).is_empty());
        let data = build(&fields, "JPG", true);
        assert_eq!(props(&data), ["P571", "P1163", "P7482"]);
        assert_eq!(data.claims[0].value, ClaimValue::Day("2026-02-20".into()));
        assert_eq!(data.claims[1].value, ClaimValue::Text("image/jpeg".into()));
    }

    #[test]
    fn bad_dates_and_unknown_things_give_no_claim() {
        let fields = MediaWikiFields {
            date: "20.02.2026".into(),
            source: "Flickr".into(),
            license: "{{cc-by-sa-3.0}}".into(),
            ..Default::default()
        };
        assert!(build(&fields, "heic", true).is_empty());
        assert!(!is_day("2026-13-01"));
        assert!(!is_day("2026-00-10"));
        assert!(is_day("2026-12-31"));
    }

    #[test]
    fn statements_never_duplicate_a_field_and_split_correctly() {
        let fields = MediaWikiFields {
            creator: "Q1".into(),
            statements: BTreeMap::from([
                ("P170".into(), "Q999".into()),
                ("P1435".into(), "Q5; Q6;Q5".into()),
                ("P856".into(), "https://example.org/?a=1;b=2".into()),
            ]),
            ..Default::default()
        };
        let data = build(&fields, "jpg", false);
        assert_eq!(props(&data), ["P170", "P1435", "P1435", "P856"]);
        assert_eq!(
            data.claims[3].value,
            ClaimValue::Text("https://example.org/?a=1;b=2".into())
        );
    }

    #[test]
    fn json_has_the_wikibase_shape() {
        let fields = MediaWikiFields {
            depicts: vec!["Q42".into()],
            camera_position: "52.5, 13.4".into(),
            date: "2026-02-20".into(),
            captions: BTreeMap::from([("en".into(), "Hello".into())]),
            ..Default::default()
        };
        let json = build(&fields, "png", true).to_json();
        assert_eq!(
            json["labels"]["en"],
            json!({ "language": "en", "value": "Hello" })
        );
        let claims = json["claims"].as_array().unwrap();
        let depicts = claims
            .iter()
            .find(|c| c["mainsnak"]["property"] == "P180")
            .unwrap();
        assert_eq!(
            depicts["mainsnak"]["datavalue"]["value"],
            json!({ "entity-type": "item", "numeric-id": 42, "id": "Q42" })
        );
        let position = claims
            .iter()
            .find(|c| c["mainsnak"]["property"] == "P1259")
            .unwrap();
        assert_eq!(position["mainsnak"]["datavalue"]["type"], "globecoordinate");
        assert_eq!(position["mainsnak"]["datavalue"]["value"]["latitude"], 52.5);
        let day = claims
            .iter()
            .find(|c| c["mainsnak"]["property"] == "P571")
            .unwrap();
        assert_eq!(
            day["mainsnak"]["datavalue"]["value"]["time"],
            "+2026-02-20T00:00:00Z"
        );
        assert_eq!(day["mainsnak"]["datavalue"]["value"]["precision"], 11);
        assert_eq!(day["type"], "statement");
    }

    #[test]
    fn nothing_to_say_is_an_empty_object() {
        assert_eq!(
            build(&MediaWikiFields::default(), "jpg", false).to_json(),
            json!({})
        );
    }
}
