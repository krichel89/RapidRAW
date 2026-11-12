//! The hook into RapidRAW's export: images are exported into a temporary
//! folder, and each finished file is uploaded to Commons from there. Same
//! arrangement as the Immich export (`immich/uploads.rs`).

use once_cell::sync::Lazy;
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use super::{pipeline, session};

static FOLDERS: Lazy<Mutex<HashSet<PathBuf>>> = Lazy::new(|| Mutex::new(HashSet::new()));

/// A fresh folder to export into. Fails early, before any image is rendered,
/// when Commons is not set up or nobody is signed in.
pub fn prepare(app_handle: &AppHandle) -> Result<PathBuf, String> {
    session::client(app_handle)?;
    let root = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("commons-exports");
    let mut folders = FOLDERS.lock().unwrap();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if !folders.contains(&entry.path()) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let folder = root.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    folders.insert(folder.clone());
    Ok(folder)
}

/// Called for every exported file. Does nothing unless the file was exported
/// into a folder from [`prepare`].
pub fn on_exported(
    app_handle: &AppHandle,
    source_path: &str,
    output_path: &Path,
) -> Result<(), String> {
    let is_ours = output_path
        .ancestors()
        .skip(1)
        .any(|folder| FOLDERS.lock().unwrap().contains(folder));
    if !is_ours {
        return Ok(());
    }
    let file_name = output_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let result = tauri::async_runtime::block_on(upload(app_handle, source_path, output_path));
    let _ = std::fs::remove_file(output_path);
    match result {
        Ok(outcome) => {
            let _ = app_handle.emit(
                "commons-upload",
                json!({ "fileName": file_name, "commonsName": outcome.filename }),
            );
            match outcome.structured_data_error {
                None => Ok(()),
                Some(error) => Err(format!(
                    "{file_name} is on Commons as \"{}\", but its structured data could not be written: {error}",
                    outcome.filename
                )),
            }
        }
        Err(e) => Err(format!("Commons upload of {file_name} failed: {e}")),
    }
}

async fn upload(
    app_handle: &AppHandle,
    source_path: &str,
    output: &Path,
) -> Result<pipeline::Outcome, String> {
    let client = session::client(app_handle)?;
    let settings = session::load_settings(app_handle);

    let (_, sidecar) = crate::file_management::parse_virtual_path(source_path);
    let file_fields = crate::exif_processing::load_sidecar(&sidecar)
        .media_wiki
        .unwrap_or_default();

    let bytes = std::fs::read(output).map_err(|e| format!("Cannot read the exported file: {e}"))?;
    let exported_name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prepared = pipeline::prepare(
        &settings,
        &file_fields,
        &exported_name,
        exif_date(&bytes).as_deref(),
    )?;
    pipeline::publish(&client, &settings, &prepared, bytes).await
}

/// Capture date of the exported file, if it carries one.
fn exif_date(bytes: &[u8]) -> Option<String> {
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(bytes))
        .ok()?;
    let field = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)?;
    let exif::Value::Ascii(ref parts) = field.value else {
        return None;
    };
    let raw = String::from_utf8_lossy(parts.first()?).into_owned();
    pipeline::exif_datetime_to_commons(&raw)
}
