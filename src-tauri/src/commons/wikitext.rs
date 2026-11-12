//! The file page of an upload: `{{Information}}`, location templates, other
//! templates, licence and categories, and the target file name.
//!
//! Follows Cammello's photo path (`workers.py`), so a page written here looks
//! like one written there. The music layout (`music.py`) is not built yet.

use crate::mediawiki::model::MediaWikiFields;
use regex::Regex;
use std::sync::LazyLock;

/// File types Commons stores. Used only to decide whether a typed extension
/// is replaced or an extension is appended.
const KNOWN_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "gif", "svg", "pdf", "jxl", "avif", "heic",
];

/// Characters MediaWiki refuses in a file name.
const ILLEGAL_CHARS: &[char] = &['#', '<', '>', '[', ']', '|', '{', '}', ':', '/', '\\'];

const MAX_NAME_BYTES: usize = 240;

static SECTION_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^[ \t]*=+[ \t]*\{\{[ \t]*int[ \t]*:[ \t]*(?:filedesc|license-header)[ \t]*\}\}[ \t]*=+[ \t]*\r?\n?",
    )
    .expect("valid regex")
});

static CATEGORY_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[[ \t]*[Cc]ategory[ \t]*:[^\]\n]+\]\]\r?\n?").expect("valid regex")
});

/// Typed `== {{int:filedesc}} ==` / `== {{int:license-header}} ==` lines are
/// removed: the page writes both headings itself, and a copy typed into a
/// field would land inside a parameter or twice (Cammello 0.18.26).
pub fn strip_section_headings(text: &str) -> String {
    SECTION_HEADING.replace_all(text, "").trim().to_string()
}

/// Category links out of a text: `(text without them, the links)`.
pub fn take_category_links(text: &str) -> (String, Vec<String>) {
    let mut links: Vec<String> = Vec::new();
    for found in CATEGORY_LINK.find_iter(text) {
        let link = found.as_str().trim().to_string();
        if !links.contains(&link) {
            links.push(link);
        }
    }
    let rest = CATEGORY_LINK.replace_all(text, "").trim().to_string();
    (rest, links)
}

fn category_link(name: &str) -> String {
    format!("[[Category:{name}]]")
}

fn strip_category_prefix(name: &str) -> &str {
    let name = name.trim();
    name.strip_prefix("Category:")
        .or_else(|| name.strip_prefix("category:"))
        .unwrap_or(name)
        .trim()
}

/// `"52.5, 13.4"` (as stored by the model) → latitude, longitude.
pub fn parse_position(text: &str) -> Option<(f64, f64)> {
    let (lat, lon) = text.split_once(',')?;
    let lat: f64 = lat.trim().parse().ok()?;
    let lon: f64 = lon.trim().parse().ok()?;
    (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

/// The complete wikitext of a file page.
///
/// `fields` are the effective values (defaults merged with the image),
/// `description` the already combined description text.
pub fn build_page(fields: &MediaWikiFields, description: &str, tracking_category: &str) -> String {
    let (description, description_categories) =
        take_category_links(&strip_section_headings(description));

    let mut info = String::from("{{Information\n");
    info.push_str(&format!("|description={description}\n"));
    for (name, value) in [
        ("date", &fields.date),
        ("source", &fields.source),
        ("author", &fields.author),
        ("permission", &fields.permission),
        ("other versions", &fields.other_versions),
    ] {
        let value = strip_section_headings(value);
        if !value.is_empty() {
            info.push_str(&format!("|{name}={value}\n"));
        }
    }
    info.push_str("}}");

    let mut parts = vec![format!("== {{{{int:filedesc}}}} ==\n{info}")];

    if let Some((lat, lon)) = parse_position(&fields.camera_position) {
        parts.push(format!("{{{{Location dec|{lat:.6}|{lon:.6}}}}}"));
    }
    if let Some((lat, lon)) = parse_position(&fields.object_position) {
        parts.push(format!("{{{{Object location dec|{lat:.6}|{lon:.6}}}}}"));
    }

    let (templates, template_categories) =
        take_category_links(&strip_section_headings(&fields.templates));
    if !templates.is_empty() {
        parts.push(templates);
    }
    let (extra, extra_categories) =
        take_category_links(&strip_section_headings(&fields.extra_wikitext));
    if !extra.is_empty() {
        parts.push(extra);
    }
    let license = strip_section_headings(&fields.license);
    if !license.is_empty() {
        parts.push(format!("== {{{{int:license-header}}}} ==\n{license}"));
    }

    // Categories: those typed into texts, then the field, the photographer's
    // own and the tracking category; each once.
    let mut categories: Vec<String> = Vec::new();
    let mut add = |link: String| {
        if !categories.contains(&link) {
            categories.push(link);
        }
    };
    for link in description_categories
        .into_iter()
        .chain(template_categories)
        .chain(extra_categories)
    {
        add(link);
    }
    for name in &fields.categories {
        let name = strip_category_prefix(name);
        if !name.is_empty() {
            add(category_link(name));
        }
    }
    let photographer = strip_category_prefix(&fields.photographer_category);
    if !photographer.is_empty() {
        add(category_link(photographer));
    }
    let tracking = strip_category_prefix(tracking_category);
    if !tracking.is_empty() {
        add(category_link(tracking));
    }
    if !categories.is_empty() {
        parts.push(categories.join("\n"));
    }

    parts.join("\n")
}

/// The name the file gets on Commons.
///
/// `wanted` is the target file name of the image (may be empty), `exported`
/// the name of the file RapidRAW exported, which supplies the extension and,
/// when nothing else is given, the name. Problems are reported instead of
/// silently fixed, because MediaWiki would otherwise store a different name
/// than the user typed.
pub fn commons_filename(wanted: &str, exported: &str) -> Result<String, String> {
    let exported_extension = std::path::Path::new(exported)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    let mut name = wanted.trim();
    for prefix in ["File:", "file:", "Datei:", "datei:"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.trim();
            break;
        }
    }
    let name = if name.is_empty() {
        exported.trim()
    } else {
        name
    };

    let name: String = name
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let found: Vec<char> = ILLEGAL_CHARS
        .iter()
        .copied()
        .filter(|c| name.contains(*c))
        .collect();
    if !found.is_empty() {
        let list: Vec<String> = found.iter().map(|c| format!("\"{c}\"")).collect();
        return Err(format!(
            "The file name \"{name}\" contains characters MediaWiki forbids: {}. Change the target file name.",
            list.join(", ")
        ));
    }

    let (stem, typed_extension) = match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && KNOWN_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) =>
        {
            (stem.to_string(), ext.to_ascii_lowercase())
        }
        _ => (name.clone(), String::new()),
    };
    let extension = if exported_extension.is_empty() {
        typed_extension
    } else {
        exported_extension
    };
    if stem.is_empty() {
        return Err("The target file name is empty.".to_string());
    }
    let full = if extension.is_empty() {
        stem
    } else {
        format!("{stem}.{extension}")
    };
    if full.len() > MAX_NAME_BYTES {
        return Err(format!(
            "The file name is {} bytes long; Commons allows {MAX_NAME_BYTES}.",
            full.len()
        ));
    }
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> MediaWikiFields {
        MediaWikiFields {
            date: "2026-02-20".into(),
            author: "[[User:Harald Krichel|Harald Krichel]]".into(),
            source: "{{own}}".into(),
            permission: "".into(),
            license: "{{Cc-by-sa-4.0}}".into(),
            categories: vec![
                "Berlinale 2026".into(),
                "Category:Portrait_photographs".into(),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn page_has_the_cammello_layout() {
        let page = build_page(&fields(), "{{en|1=A portrait}}", "");
        let expected = "== {{int:filedesc}} ==\n\
{{Information\n\
|description={{en|1=A portrait}}\n\
|date=2026-02-20\n\
|source={{own}}\n\
|author=[[User:Harald Krichel|Harald Krichel]]\n\
}}\n\
== {{int:license-header}} ==\n\
{{Cc-by-sa-4.0}}\n\
[[Category:Berlinale 2026]]\n\
[[Category:Portrait_photographs]]";
        assert_eq!(page, expected);
    }

    #[test]
    fn locations_templates_and_tracking() {
        let mut f = fields();
        f.camera_position = "52.5, 13.4".into();
        f.object_position = "52.51, 13.41".into();
        f.templates = "{{Do not crop}}".into();
        f.extra_wikitext = "<!-- note -->".into();
        f.photographer_category = "Photographs by Harald Krichel".into();
        f.other_versions = "[[File:Other.jpg]]".into();
        let page = build_page(&f, "x", "Uploaded with RapidRAW");
        let order = [
            "{{Information",
            "|other versions=[[File:Other.jpg]]",
            "{{Location dec|52.500000|13.400000}}",
            "{{Object location dec|52.510000|13.410000}}",
            "{{Do not crop}}",
            "<!-- note -->",
            "== {{int:license-header}} ==",
            "[[Category:Berlinale 2026]]",
            "[[Category:Photographs by Harald Krichel]]",
            "[[Category:Uploaded with RapidRAW]]",
        ];
        let mut at = 0;
        for needle in order {
            let found = page[at..]
                .find(needle)
                .unwrap_or_else(|| panic!("{needle} missing or out of order in:\n{page}"));
            at += found + needle.len();
        }
    }

    #[test]
    fn typed_headings_are_removed_and_categories_move_to_the_end() {
        let mut f = fields();
        f.categories.clear();
        let page = build_page(
            &f,
            "== {{int:filedesc}} ==\nText\n[[Category:Typed]]\n[[Category:Typed]]",
            "Typed",
        );
        assert_eq!(page.matches("{{int:filedesc}}").count(), 1);
        assert!(page.contains("|description=Text\n"));
        assert_eq!(page.matches("[[Category:Typed]]").count(), 1);
        assert!(page.ends_with("[[Category:Typed]]"));
    }

    #[test]
    fn empty_optional_parts_leave_no_gaps() {
        let page = build_page(&MediaWikiFields::default(), "", "");
        assert_eq!(
            page,
            "== {{int:filedesc}} ==\n{{Information\n|description=\n}}"
        );
    }

    #[test]
    fn filenames() {
        assert_eq!(commons_filename("", "IMG_1.jpg").unwrap(), "IMG 1.jpg");
        assert_eq!(
            commons_filename("File:Anna Meier 2026.jpeg", "x.jpg").unwrap(),
            "Anna Meier 2026.jpg",
            "typed extension is replaced by the real one"
        );
        assert_eq!(
            commons_filename("Anna  Meier", "x.JPG").unwrap(),
            "Anna Meier.jpg"
        );
        assert_eq!(
            commons_filename("Anna Meier 1.5", "x.png").unwrap(),
            "Anna Meier 1.5.png",
            "a dot that is no extension stays"
        );
        assert!(
            commons_filename("Anna: Meier", "x.jpg")
                .unwrap_err()
                .contains("\":\"")
        );
        assert!(commons_filename("a/b", "x.jpg").is_err());
        assert!(commons_filename(&"a".repeat(300), "x.jpg").is_err());
        assert!(commons_filename("   ", "").is_err());
    }

    #[test]
    fn positions() {
        assert_eq!(parse_position("52.5, 13.4"), Some((52.5, 13.4)));
        assert_eq!(parse_position("52.5"), None);
        assert_eq!(parse_position("91, 0"), None);
    }
}
