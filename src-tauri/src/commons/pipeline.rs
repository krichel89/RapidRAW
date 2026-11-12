//! One upload, start to finish, without Tauri: combine the metadata, build the
//! page and the structured data, send both.

use super::client::CommonsClient;
use super::sdc::{self, StructuredData};
use super::settings::CommonsSettings;
use super::wikitext;
use crate::mediawiki::model::{MediaWikiFields, merged_description};

pub struct Prepared {
    pub filename: String,
    pub page_text: String,
    pub data: StructuredData,
}

pub struct Outcome {
    pub filename: String,
    /// The file is on Commons, but its structured data could not be written.
    pub structured_data_error: Option<String>,
}

/// `2026:02:20 14:03:11` (EXIF) → `2026-02-20 14:03:11`.
pub fn exif_datetime_to_commons(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_end_matches('\0');
    let (date, time) = raw.split_once(' ').unwrap_or((raw, ""));
    let parts: Vec<&str> = date.split(':').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
        || parts[0] == "0000"
    {
        return None;
    }
    let date = parts.join("-");
    let time = time.trim();
    if time.is_empty() || !time.bytes().all(|b| b.is_ascii_digit() || b == b':') {
        Some(date)
    } else {
        Some(format!("{date} {time}"))
    }
}

/// Everything that goes to Commons for one image.
///
/// `file` is the image's own metadata, `exported_name` the name of the file
/// RapidRAW exported, `exif_date` the capture date from that file, used when
/// no date was entered.
pub fn prepare(
    settings: &CommonsSettings,
    file: &MediaWikiFields,
    exported_name: &str,
    exif_date: Option<&str>,
) -> Result<Prepared, String> {
    // The target file name and the description of the defaults are per file
    // by nature; only their base description takes part (see `merged_description`).
    let mut base = settings.defaults.clone();
    base.target_filename.clear();
    base.description.clear();

    let mut fields = MediaWikiFields::merge(&base, file);
    if fields.date.is_empty()
        && let Some(date) = exif_date
    {
        fields.date = date.to_string();
    }
    let description = merged_description(&base, file);

    let filename = wikitext::commons_filename(&fields.target_filename, exported_name)?;
    let extension = filename.rsplit('.').next().unwrap_or("");
    Ok(Prepared {
        page_text: wikitext::build_page(&fields, &description, &settings.tracking_category),
        data: sdc::build(&fields, extension, settings.structured_capture),
        filename,
    })
}

/// Send the file, then its structured data. A failure of the first step means
/// nothing is on Commons; of the second, that the file is there without its
/// statements, which is reported in the outcome rather than as an error.
pub async fn publish(
    client: &CommonsClient,
    settings: &CommonsSettings,
    prepared: &Prepared,
    bytes: Vec<u8>,
) -> Result<Outcome, String> {
    client
        .upload(
            &prepared.filename,
            bytes,
            &prepared.page_text,
            &settings.edit_summary,
            settings.ignore_warnings,
        )
        .await?;

    let mut outcome = Outcome {
        filename: prepared.filename.clone(),
        structured_data_error: None,
    };
    if prepared.data.is_empty() {
        return Ok(outcome);
    }
    let written = async {
        let page_id = client
            .page_id(&prepared.filename)
            .await?
            .ok_or_else(|| "no page id for the new file".to_string())?;
        client
            .set_structured_data(page_id, &prepared.data.to_json())
            .await?;
        if prepared.data.has_depicts() {
            // Non-fatal: only moves the file out of the maintenance categories sooner.
            if let Err(e) = client.purge(&prepared.filename).await {
                log::info!("Could not purge {}: {e}", prepared.filename);
            }
        }
        Ok::<(), String>(())
    }
    .await;
    outcome.structured_data_error = written.err();
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn settings() -> CommonsSettings {
        let mut settings = CommonsSettings {
            tracking_category: "Uploaded with RapidRAW".into(),
            ..Default::default()
        };
        settings.defaults.author = "[[User:Harald Krichel|Harald Krichel]]".into();
        settings.defaults.source = "{{own}}".into();
        settings.defaults.license = "{{Cc-by-sa-4.0}}".into();
        settings.defaults.base_description = "{{en|1=Berlinale 2026}}".into();
        settings.defaults.depicts = vec!["Q1".into()];
        settings.defaults.target_filename = "Should not apply.jpg".into();
        settings.defaults.captions = BTreeMap::from([("en".into(), "Default".into())]);
        settings
    }

    #[test]
    fn defaults_and_image_are_combined() {
        let file = MediaWikiFields {
            target_filename: "Anna Meier at the Berlinale 2026.jpg".into(),
            description: "{{de|1=Anna Meier}}".into(),
            depicts: vec!["Q2".into()],
            captions: BTreeMap::from([("de".into(), "Anna Meier".into())]),
            ..Default::default()
        };
        let prepared = prepare(
            &settings(),
            &file,
            "IMG_0001.jpg",
            Some("2026-02-20 14:03:11"),
        )
        .unwrap();
        assert_eq!(prepared.filename, "Anna Meier at the Berlinale 2026.jpg");
        assert!(
            prepared
                .page_text
                .contains("|description={{en|1=Berlinale 2026}}\n{{de|1=Anna Meier}}\n")
        );
        assert!(prepared.page_text.contains("|date=2026-02-20 14:03:11\n"));
        assert!(
            prepared
                .page_text
                .contains("|author=[[User:Harald Krichel|Harald Krichel]]\n")
        );
        assert!(
            prepared
                .page_text
                .ends_with("[[Category:Uploaded with RapidRAW]]")
        );
        let json = prepared.data.to_json();
        assert_eq!(json["labels"]["en"]["value"], "Default");
        assert_eq!(json["labels"]["de"]["value"], "Anna Meier");
        let depicts: Vec<&str> = json["claims"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["mainsnak"]["property"] == "P180")
            .map(|c| c["mainsnak"]["datavalue"]["value"]["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            depicts,
            ["Q1", "Q2"],
            "depicts of defaults and image are joined"
        );
    }

    #[test]
    fn the_defaults_target_name_never_applies() {
        let prepared = prepare(
            &settings(),
            &MediaWikiFields::default(),
            "IMG_0002.jpg",
            None,
        )
        .unwrap();
        assert_eq!(prepared.filename, "IMG 0002.jpg");
        assert!(
            !prepared.page_text.contains("|date="),
            "no date known, none written"
        );
    }

    #[test]
    fn a_typed_date_beats_the_exif_date() {
        let file = MediaWikiFields {
            date: "2026-02-21".into(),
            ..Default::default()
        };
        let prepared = prepare(&settings(), &file, "a.jpg", Some("2026-02-20 10:00:00")).unwrap();
        assert!(prepared.page_text.contains("|date=2026-02-21\n"));
    }

    #[test]
    fn a_bad_file_name_stops_before_anything_is_sent() {
        let file = MediaWikiFields {
            target_filename: "Anna: Meier".into(),
            ..Default::default()
        };
        assert!(prepare(&settings(), &file, "a.jpg", None).is_err());
    }

    #[test]
    fn exif_dates() {
        assert_eq!(
            exif_datetime_to_commons("2026:02:20 14:03:11").as_deref(),
            Some("2026-02-20 14:03:11")
        );
        assert_eq!(
            exif_datetime_to_commons("2026:02:20").as_deref(),
            Some("2026-02-20")
        );
        assert_eq!(
            exif_datetime_to_commons("2026:02:20 14:03:11\0").as_deref(),
            Some("2026-02-20 14:03:11")
        );
        assert_eq!(exif_datetime_to_commons("0000:00:00 00:00:00"), None);
        assert_eq!(exif_datetime_to_commons("yesterday"), None);
        assert_eq!(exif_datetime_to_commons("2026:2:20"), None);
    }
}
