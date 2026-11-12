use super::model::MediaWikiFields;
use crate::exif_processing;
use crate::file_management::parse_virtual_path;

/// The MediaWiki metadata of each path, in the order given. An image without
/// any returns an empty record.
#[tauri::command]
pub fn mediawiki_get_metadata(paths: Vec<String>) -> Vec<MediaWikiFields> {
    paths
        .iter()
        .map(|path| {
            let (_source, sidecar) = parse_virtual_path(path);
            exif_processing::load_sidecar(&sidecar)
                .media_wiki
                .unwrap_or_default()
        })
        .collect()
}

/// Apply `patch` to every path. Only the fields named in the patch change;
/// `null`, `""`, `[]` and `{}` clear a field. The patch is checked once
/// against an empty record, so a bad value stops the whole call before any
/// sidecar is touched.
#[tauri::command]
pub fn mediawiki_set_metadata(paths: Vec<String>, patch: serde_json::Value) -> Result<(), String> {
    MediaWikiFields::default()
        .apply_patch(&patch)
        .map_err(|errors| errors.join("; "))?;

    let mut failures = Vec::new();
    for path in &paths {
        let (source, sidecar) = parse_virtual_path(path);
        let result = exif_processing::update_sidecar(&sidecar, &source, |metadata| {
            let mut fields = metadata.media_wiki.clone().unwrap_or_default();
            if fields.apply_patch(&patch).is_ok() {
                metadata.media_wiki = (!fields.is_empty()).then_some(fields);
            }
        });
        if let Err(error) = result {
            failures.push(format!("{path}: {error}"));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}
