use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

const PACKAGES_INDEX_API_URL: &str =
    "https://api.github.com/repos/paintdev/rpackages/contents/packages?ref=main";

#[derive(Deserialize)]
struct GitHubEntry {
    name: String,
    #[serde(rename = "type")]
    entry_type: String,
}

fn parse_package_names(body: &str) -> Result<Vec<String>, serde_json::Error> {
    let entries: Vec<GitHubEntry> = serde_json::from_str(body)?;
    let mut packages: Vec<String> = entries
        .into_iter()
        .filter(|entry| entry.entry_type == "file")
        .filter_map(|entry| entry.name.strip_suffix(".json").map(String::from))
        .collect();
    packages.sort();
    Ok(packages)
}

fn filter_packages(packages: &[String], query: Option<&str>) -> Vec<String> {
    match query.map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => {
            let terms: Vec<String> = q.split_whitespace().map(|t| t.to_lowercase()).collect();
            packages
                .iter()
                .filter(|pkg| {
                    let lower = pkg.to_lowercase();
                    terms.iter().all(|term| lower.contains(term.as_str()))
                })
                .cloned()
                .collect()
        }
        None => packages.to_vec(),
    }
}

fn search_packages(query: Option<&str>) {
    let client = match reqwest::blocking::Client::builder()
        .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            println!("Failed to initialize HTTP client: {}", e);
            return;
        }
    };

    match client
        .get(PACKAGES_INDEX_API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.text() {
                    Ok(body) => match parse_package_names(&body) {
                        Ok(packages) => {
                            let matched = filter_packages(&packages, query);
                            match query.map(str::trim).filter(|q| !q.is_empty()) {
                                Some(q) => {
                                    if matched.is_empty() {
                                        println!("No packages found matching '{}'.", q);
                                    } else {
                                        println!(
                                            "Found {} package(s) matching '{}':",
                                            matched.len(),
                                            q
                                        );
                                        for pkg in matched {
                                            println!("  {}", pkg);
                                        }
                                    }
                                }
                                None => {
                                    if matched.is_empty() {
                                        println!("No packages found in registry.");
                                    } else {
                                        println!("Available packages ({}):", matched.len());
                                        for pkg in matched {
                                            println!("  {}", pkg);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => println!("Failed to parse registry package list: {}", e),
                    },
                    Err(e) => println!("Failed to read registry response: {}", e),
                }
            } else {
                println!(
                    "Failed to fetch package list from registry (HTTP {}).",
                    response.status()
                );
            }
        }
        Err(e) => println!("Network error connecting to GitHub: {}", e),
    }
}

fn get_current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-aarch64"
        } else {
            "macos-x86_64"
        }
    } else {
        "linux-x86_64"
    }
}

fn get_install_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        let local_app_data = env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| String::from("C:\\Users\\Public"));
        PathBuf::from(local_app_data).join("rcrate").join("bin")
    } else {
        let home = env::var("HOME")
            .unwrap_or_else(|_| String::from("/tmp"));
        PathBuf::from(home).join(".local").join("bin")
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: rcrate <command> [<args>]");
        return;
    }

    let command = &args[1];

    match command.as_str() {
        "install" => {
            if args.len() < 3 {
                println!("Error: please specify a package");
                return;
            }
            let package_name = &args[2];
            let platform = get_current_platform();
            let install_dir = get_install_dir();

            println!("Fetching index for {}...", package_name);

            let registry_url = format!(
                "https://raw.githubusercontent.com/paintdev/rpackages/main/packages/{}.json",
                package_name
            );

            match reqwest::blocking::get(&registry_url) {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.text() {
                            Ok(body) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                                    let is_installer = json.get("installer")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);

                                    if let Some(url) = json.get("urls").and_then(|u| u.get(platform)).and_then(|v| v.as_str()) {
                                        println!("Downloading from external source: {}", url);
                                        match reqwest::blocking::get(url) {
                                            Ok(bin_resp) => {
                                                if let Ok(bytes) = bin_resp.bytes() {
                                                    if let Err(e) = fs::create_dir_all(&install_dir) {
                                                        println!("Error creating install directory: {}", e);
                                                        return;
                                                    }

                                                    let mut target_path = install_dir.join(package_name);
                                                    if cfg!(target_os = "windows") && is_installer {
                                                        target_path.set_extension("exe");
                                                    }

                                                    if let Err(e) = fs::write(&target_path, &bytes) {
                                                        println!("Error saving file: {}", e);
                                                    } else {
                                                        println!("Successfully saved {} to {:?}", package_name, target_path);

                                                        if is_installer {
                                                            println!("Running installer...");
                                                            if let Err(e) = Command::new(&target_path).spawn() {
                                                                println!("Failed to run installer: {}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => println!("Failed to download file: {}", e),
                                        }
                                    } else {
                                        println!("No download URL found in index for platform: {}", platform);
                                    }
                                }
                            }
                            Err(e) => println!("Failed to read registry response: {}", e),
                        }
                    } else {
                        println!("Package '{}' not found in registry index.", package_name);
                    }
                }
                Err(e) => println!("Network error connecting to GitHub: {}", e),
            }
        }
        "remove" => {
            if args.len() < 3 {
                println!("Error: please specify a package to remove");
                return;
            }
            let package_name = &args[2];
            let install_dir = get_install_dir();
            let mut target_path = install_dir.join(package_name);
            if cfg!(target_os = "windows") {
                target_path.set_extension("exe");
            }

            if target_path.exists() {
                if let Err(e) = fs::remove_file(&target_path) {
                    println!("Error removing package file: {}", e);
                } else {
                    println!("Successfully removed {}.", package_name);
                }
            } else {
                println!("Package '{}' not found in installation path.", package_name);
            }
        }
        "search" => {
            let query = if args.len() >= 3 {
                let joined = args[2..].join(" ");
                let trimmed = joined.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            } else {
                None
            };
            search_packages(query.as_deref());
        }
        "publish" => {
            println!("To publish, commit and push your package JSON file to your GitHub index repository.");
        }
        _ => {
            println!("Unknown command: '{}'", command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_sorts_json_package_files_only() {
        let json = r#"[
            {"name": "vesktop.json", "type": "file"},
            {"name": "discord.json", "type": "file"},
            {"name": "README.md", "type": "file"},
            {"name": "nested", "type": "dir"}
        ]"#;

        let packages = parse_package_names(json).expect("valid GitHub contents JSON");
        assert_eq!(packages, vec!["discord", "vesktop"]);
    }

    #[test]
    fn filter_returns_all_packages_when_query_is_none_or_blank() {
        let packages = vec![
            "discord".to_string(),
            "firefox".to_string(),
            "prism-launcher".to_string(),
        ];

        assert_eq!(filter_packages(&packages, None), packages);
        assert_eq!(filter_packages(&packages, Some("   ")), packages);
    }

    #[test]
    fn filter_matches_case_insensitively_and_across_terms() {
        let packages = vec![
            "discord".to_string(),
            "firefox".to_string(),
            "prism-launcher".to_string(),
            "vencord".to_string(),
        ];

        assert_eq!(
            filter_packages(&packages, Some("CORD")),
            vec!["discord", "vencord"]
        );
        assert_eq!(
            filter_packages(&packages, Some("prism launcher")),
            vec!["prism-launcher"]
        );
        assert!(filter_packages(&packages, Some("nonexistent")).is_empty());
    }
}
