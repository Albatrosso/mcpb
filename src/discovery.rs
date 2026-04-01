use std::path::PathBuf;
use std::{env, fs};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserApp {
    pub app_path: PathBuf,
    pub executable_path: PathBuf,
    pub bundle_id: String,
    pub display_name: String,
    pub slug: String,
}

pub fn find_browser(query: &str) -> Result<BrowserApp, String> {
    find_browser_in_dirs(query, &application_dirs())
}

fn find_browser_in_dirs(query: &str, directories: &[PathBuf]) -> Result<BrowserApp, String> {
    let browsers = discover_browsers(directories)?;
    let query_normalized = normalize(query);

    let mut matches = browsers
        .into_iter()
        .filter_map(|app| {
            let score = browser_score(&app, &query_normalized);
            (score > 0).then_some((score, app))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| right.0.cmp(&left.0));

    let Some((top_score, best_match)) = matches.first() else {
        return Err(format!("Browser \"{query}\" is not installed"));
    };

    let tied_matches = matches
        .iter()
        .filter(|(score, _)| score == top_score)
        .map(|(_, app)| app.display_name.clone())
        .collect::<Vec<_>>();

    if tied_matches.len() > 1 {
        return Err(format!(
            "Multiple browsers match \"{query}\": {}",
            tied_matches.join(", ")
        ));
    }

    Ok(best_match.clone())
}

fn discover_browsers(directories: &[PathBuf]) -> Result<Vec<BrowserApp>, String> {
    let mut browsers = Vec::new();

    for directory in directories {
        if !directory.exists() {
            continue;
        }

        let entries = fs::read_dir(directory)
            .map_err(|error| format!("Could not read {}: {error}", directory.display()))?;

        for entry in entries {
            let entry = entry.map_err(|error| format!("Could not inspect app bundle: {error}"))?;
            let app_path = entry.path();

            if !app_path.is_dir()
                || app_path.extension().and_then(|ext| ext.to_str()) != Some("app")
            {
                continue;
            }

            let info_path = app_path.join("Contents").join("Info.plist");
            if !info_path.exists() {
                continue;
            }

            let Ok(info) = plist::from_file::<_, InfoPlist>(&info_path) else {
                continue;
            };

            let executable_path = app_path
                .join("Contents")
                .join("MacOS")
                .join(&info.bundle_executable);

            if !executable_path.exists() {
                continue;
            }

            let display_name = info
                .bundle_name
                .clone()
                .unwrap_or_else(|| app_name_without_extension(&app_path));

            browsers.push(BrowserApp {
                app_path: app_path.clone(),
                executable_path,
                bundle_id: info.bundle_identifier,
                display_name: display_name.clone(),
                slug: slugify(&display_name),
            });
        }
    }

    Ok(browsers)
}

fn application_dirs() -> Vec<PathBuf> {
    if let Ok(value) = env::var("MCPB_APPLICATION_DIRS") {
        return value
            .split(':')
            .filter(|part| !part.is_empty())
            .map(PathBuf::from)
            .collect();
    }

    let mut directories = vec![PathBuf::from("/Applications")];

    if let Ok(home) = env::var("HOME") {
        directories.push(PathBuf::from(home).join("Applications"));
    }

    directories
}

fn browser_score(app: &BrowserApp, query: &str) -> usize {
    aliases(app)
        .into_iter()
        .map(|alias| alias_score(&alias, query))
        .max()
        .unwrap_or(0)
}

fn aliases(app: &BrowserApp) -> Vec<String> {
    vec![
        app.display_name.clone(),
        app_name_without_extension(&app.app_path),
        app.executable_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
    ]
}

fn alias_score(alias: &str, query: &str) -> usize {
    let alias_normalized = normalize(alias);
    if alias_normalized.is_empty() || query.is_empty() {
        return 0;
    }

    if alias_normalized == query {
        return 1000;
    }

    let tokens = tokenize(alias);
    if tokens.iter().any(|token| token == query) {
        return 800;
    }

    if alias_normalized.starts_with(query) || query.starts_with(&alias_normalized) {
        return 600;
    }

    0
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|char| char.is_ascii_alphanumeric())
        .flat_map(|char| char.to_lowercase())
        .collect()
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|char: char| !char.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;

    for char in value.chars() {
        if char.is_ascii_alphanumeric() {
            slug.push(char.to_ascii_lowercase());
            previous_was_separator = false;
            continue;
        }

        if !previous_was_separator && !slug.is_empty() {
            slug.push('-');
            previous_was_separator = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn app_name_without_extension(app_path: &std::path::Path) -> String {
    app_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string()
}

#[derive(Debug, Deserialize)]
struct InfoPlist {
    #[serde(rename = "CFBundleName")]
    bundle_name: Option<String>,
    #[serde(rename = "CFBundleExecutable")]
    bundle_executable: String,
    #[serde(rename = "CFBundleIdentifier")]
    bundle_identifier: String,
}

#[cfg(test)]
mod tests {
    use super::find_browser_in_dirs;
    use plist::Value;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn finds_browser_case_insensitively_by_app_name_and_display_name() {
        let temp = TempDir::new().unwrap();
        let apps_dir = temp.path().join("Applications");

        write_app_bundle(
            &apps_dir,
            "Brave Browser.app",
            "Brave",
            "Brave Browser",
            "com.brave.Browser",
        );

        let app = find_browser_in_dirs("bRaVe", &[apps_dir]).unwrap();

        assert_eq!(app.display_name, "Brave");
        assert_eq!(app.bundle_id, "com.brave.Browser");
        assert!(app.executable_path.ends_with("Brave Browser"));
    }

    #[test]
    fn rejects_unknown_browser() {
        let temp = TempDir::new().unwrap();
        let apps_dir = temp.path().join("Applications");
        fs::create_dir_all(&apps_dir).unwrap();

        let error = find_browser_in_dirs("arc", &[apps_dir]).unwrap_err();

        assert!(error.contains("is not installed"));
    }

    #[test]
    fn rejects_ambiguous_browser_matches() {
        let temp = TempDir::new().unwrap();
        let apps_dir = temp.path().join("Applications");

        write_app_bundle(
            &apps_dir,
            "Google Chrome.app",
            "Google Chrome",
            "Google Chrome",
            "com.google.Chrome",
        );
        write_app_bundle(
            &apps_dir,
            "Chrome Canary.app",
            "Chrome Canary",
            "Google Chrome Canary",
            "com.google.Chrome.canary",
        );

        let error = find_browser_in_dirs("chrome", &[apps_dir]).unwrap_err();

        assert!(error.contains("Multiple browsers match"));
    }

    #[test]
    fn skips_malformed_app_bundles_and_keeps_searching() {
        let temp = TempDir::new().unwrap();
        let apps_dir = temp.path().join("Applications");

        write_malformed_app_bundle(&apps_dir, "Broken Browser.app");
        write_app_bundle(&apps_dir, "Dia.app", "Dia", "Dia", "company.thebrowser.dia");

        let app = find_browser_in_dirs("dia", &[apps_dir]).unwrap();

        assert_eq!(app.display_name, "Dia");
    }

    #[test]
    fn does_not_match_short_substrings_inside_unrelated_app_names() {
        let temp = TempDir::new().unwrap();
        let apps_dir = temp.path().join("Applications");

        write_app_bundle(
            &apps_dir,
            "The Unarchiver.app",
            "The Unarchiver",
            "The Unarchiver",
            "cx.c3.theunarchiver",
        );

        let error = find_browser_in_dirs("arc", &[apps_dir]).unwrap_err();

        assert!(error.contains("is not installed"));
    }

    fn write_app_bundle(
        apps_dir: &Path,
        app_dir_name: &str,
        display_name: &str,
        executable_name: &str,
        bundle_id: &str,
    ) {
        let contents_dir = apps_dir.join(app_dir_name).join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        fs::create_dir_all(&macos_dir).unwrap();
        fs::write(macos_dir.join(executable_name), "#!/bin/sh\n").unwrap();

        let plist_path = contents_dir.join("Info.plist");
        let plist = Value::Dictionary(
            [
                (
                    String::from("CFBundleName"),
                    Value::String(display_name.into()),
                ),
                (
                    String::from("CFBundleExecutable"),
                    Value::String(executable_name.into()),
                ),
                (
                    String::from("CFBundleIdentifier"),
                    Value::String(bundle_id.into()),
                ),
            ]
            .into_iter()
            .collect(),
        );
        plist.to_file_xml(plist_path).unwrap();
    }

    fn write_malformed_app_bundle(apps_dir: &Path, app_dir_name: &str) {
        let contents_dir = apps_dir.join(app_dir_name).join("Contents");
        fs::create_dir_all(&contents_dir).unwrap();
        fs::write(
            contents_dir.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Broken Browser</string>
</dict>
</plist>
"#,
        )
        .unwrap();
    }
}
