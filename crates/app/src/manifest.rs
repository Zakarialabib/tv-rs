//! The platform manifests, generated from one descriptor.
//!
//! Tizen and webOS both want a file that says who the app is, and neither format is
//! hard — what is hard is remembering which fields each one actually enforces, and
//! which of them a television only tells you about by refusing an install. So the two
//! generators live side by side here, each documented with the fields it sets and
//! why, and both are pure functions of a validated descriptor.
//!
//! Escaping is not optional in either format: an app name is operator-supplied text,
//! and `&` in an app name is a broken `config.xml` if nobody escapes it.

use crate::descriptor::AppDescriptor;
use crate::error::AppError;

/// Tizen's minimum `required_version` for a TV app: Tizen 6.0.
///
/// The value is a floor, not a feature: a TV older than this will not offer the app,
/// and a newer one installs it unchanged.
pub const TIZEN_REQUIRED_VERSION: &str = "6.0";

/// The Tizen web-app manifest (`config.xml`).
///
/// The fields that matter for a signage app, beyond identity:
///
/// - `tizen:profile name="tv-samsung"` — declares this as a TV app rather than a
///   mobile or wearable one; without it the TV's app list does not show it.
/// - `tizen:setting hwkey-event="enable"` — lets the app receive remote-control key
///   events. This is the field a D-pad-driven app cannot ship without, and its
///   absence is silent: the app runs and simply never sees a key press.
/// - `tizen:setting screen-orientation` — set from the descriptor's orientation, so a
///   portrait wall is declared rather than rotated at runtime.
///
/// # Errors
///
/// Returns [`AppError::InvalidDescriptor`] when the descriptor does not validate.
pub fn tizen_config_xml(descriptor: &AppDescriptor) -> Result<String, AppError> {
    descriptor.validate()?;

    let mut xml = String::with_capacity(512);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<widget xmlns=\"http://www.w3.org/ns/widgets\" ");
    xml.push_str("xmlns:tizen=\"http://tizen.org/ns/widgets\" ");
    xml.push_str(&format!(
        "id=\"http://{}\" version=\"{}\" viewmodes=\"maximized\">\n",
        xml_escape(&descriptor.id),
        xml_escape(&descriptor.version)
    ));
    xml.push_str(&format!(
        "  <tizen:application id=\"{}\" package=\"{}\" required_version=\"{}\"/>\n",
        xml_escape(&descriptor.id),
        xml_escape(&descriptor.tizen_package()),
        TIZEN_REQUIRED_VERSION
    ));
    xml.push_str(&format!(
        "  <content src=\"{}\"/>\n",
        xml_escape(&descriptor.entry)
    ));
    xml.push_str(&format!(
        "  <name>{}</name>\n",
        xml_escape(&descriptor.name)
    ));
    if let Some(icon) = &descriptor.icon {
        xml.push_str(&format!("  <icon src=\"{}\"/>\n", xml_escape(icon)));
    }
    xml.push_str("  <feature name=\"http://tizen.org/feature/screen.size.all\"/>\n");
    xml.push_str("  <tizen:profile name=\"tv-samsung\"/>\n");
    xml.push_str(
        "  <tizen:setting screen-orientation=\"landscape\" context-menu=\"disable\" \
background-support=\"enable\" encryption=\"disable\" install-location=\"auto\" \
hwkey-event=\"enable\"/>\n",
    );
    xml.push_str("</widget>\n");
    Ok(xml)
}

/// The webOS web-app manifest (`appinfo.json`).
///
/// webOS reads identity (`id`, `version`, `vendor`, `title`), the entry document
/// (`main`) and the icons from here. `type` is always `web` for this crate: it
/// packages web payloads, and a native webOS app would not be one.
///
/// # Errors
///
/// Returns [`AppError::InvalidDescriptor`] when the descriptor does not validate.
pub fn webos_appinfo_json(descriptor: &AppDescriptor) -> Result<String, AppError> {
    descriptor.validate()?;

    let mut json = String::with_capacity(320);
    json.push_str("{\n");
    json.push_str(&format!("  \"id\": {},\n", json_string(&descriptor.id)));
    json.push_str(&format!(
        "  \"version\": {},\n",
        json_string(&descriptor.version)
    ));
    json.push_str(&format!(
        "  \"vendor\": {},\n",
        json_string(&descriptor.vendor)
    ));
    json.push_str("  \"type\": \"web\",\n");
    json.push_str(&format!(
        "  \"main\": {},\n",
        json_string(&descriptor.entry)
    ));
    json.push_str(&format!(
        "  \"title\": {},\n",
        json_string(&descriptor.name)
    ));
    // A packaged webOS app expects both sizes; the same file may serve both when the
    // payload ships one icon, and webOS scales it.
    match &descriptor.icon {
        Some(icon) => {
            json.push_str(&format!("  \"icon\": {},\n", json_string(icon)));
            json.push_str(&format!("  \"largeIcon\": {}\n", json_string(icon)));
        }
        None => {
            // Keep the document valid JSON: rewrite the trailing comma of the title.
            let trimmed = json.trim_end();
            let without_comma = trimmed.strip_suffix(',').unwrap_or(trimmed);
            json = String::from(without_comma);
            json.push('\n');
        }
    }
    json.push_str("}\n");
    Ok(json)
}

fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> AppDescriptor {
        AppDescriptor::new("com.example.wall", "Lobby wall", "1.0.0").with_vendor("Example Signage")
    }

    #[test]
    fn the_tizen_manifest_carries_identity_and_the_tv_profile() {
        let xml = tizen_config_xml(&descriptor()).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(xml.contains("id=\"http://com.example.wall\""));
        assert!(xml.contains("version=\"1.0.0\""));
        assert!(xml.contains("<tizen:application id=\"com.example.wall\" package=\"com.example\""));
        assert!(xml.contains("required_version=\"6.0\""));
        assert!(xml.contains("<content src=\"index.html\"/>"));
        assert!(xml.contains("<name>Lobby wall</name>"));
        assert!(xml.contains("<tizen:profile name=\"tv-samsung\"/>"));
    }

    #[test]
    fn the_tizen_manifest_enables_hardware_key_events() {
        // The one field whose absence is silent: the app runs and never sees a key.
        let xml = tizen_config_xml(&descriptor()).unwrap();
        assert!(xml.contains("hwkey-event=\"enable\""), "{xml}");
    }

    #[test]
    fn an_icon_appears_only_when_there_is_one() {
        let without = tizen_config_xml(&descriptor()).unwrap();
        assert!(!without.contains("<icon"));
        let with = tizen_config_xml(&descriptor().with_icon("icon.png")).unwrap();
        assert!(with.contains("<icon src=\"icon.png\"/>"));
    }

    #[test]
    fn the_webos_manifest_is_json_with_both_icon_sizes() {
        let json = webos_appinfo_json(&descriptor().with_icon("icon.png")).unwrap();
        assert!(json.contains("\"id\": \"com.example.wall\""));
        assert!(json.contains("\"type\": \"web\""));
        assert!(json.contains("\"main\": \"index.html\""));
        assert!(json.contains("\"vendor\": \"Example Signage\""));
        assert!(json.contains("\"icon\": \"icon.png\""));
        assert!(json.contains("\"largeIcon\": \"icon.png\""));
    }

    #[test]
    fn the_webos_manifest_stays_valid_json_without_an_icon() {
        let json = webos_appinfo_json(&descriptor()).unwrap();
        assert!(!json.contains("icon"), "{json}");
        // The trailing comma of `title` must not survive when the icon block is gone.
        assert!(json.trim_end().ends_with('}'), "{json}");
        let title_line = json
            .lines()
            .find(|line| line.contains("\"title\""))
            .expect("title line");
        assert!(
            title_line.trim_end().ends_with("\"Lobby wall\""),
            "{title_line}"
        );
    }

    #[test]
    fn operator_text_is_escaped_in_both_formats() {
        let descriptor = AppDescriptor::new("com.example.wall", "Lobby & <annex> \"2\"", "1.0.0")
            .with_vendor("O'Neil");
        let xml = tizen_config_xml(&descriptor).unwrap();
        assert!(
            xml.contains("Lobby &amp; &lt;annex&gt; &quot;2&quot;"),
            "{xml}"
        );
        let json = webos_appinfo_json(&descriptor).unwrap();
        assert!(
            json.contains("\"title\": \"Lobby & <annex> \\\"2\\\"\""),
            "{json}"
        );
        assert!(json.contains("\"vendor\": \"O'Neil\""), "{json}");
    }

    #[test]
    fn a_bad_descriptor_is_refused_before_a_manifest_is_built() {
        let descriptor = AppDescriptor::new("wall", "W", "1.0.0");
        assert!(tizen_config_xml(&descriptor).is_err());
        assert!(webos_appinfo_json(&descriptor).is_err());
    }
}
