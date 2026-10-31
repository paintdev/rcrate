use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
        "init" => {
            if args.len() < 3 {
                println!("Error: please specify a package name for init");
                return;
            }
            let package_name = &args[2];
            let filename = "rcrate.toml";
            let content = format!(
                "name = \"{}\"\nversion = \"0.1.0\"\nauthors = []\n\n[dependencies]\n",
                package_name
            );

            match fs::write(filename, content) {
                Ok(_) => println!("Initialized package '{}' and created {}", package_name, filename),
                Err(e) => println!("Error creating configuration file: {}", e),
            }
        }
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
        "login" => {
            println!("No login required for static GitHub registry index.");
        }
        "publish" => {
            println!("To publish, commit and push your package JSON file to your GitHub index repository.");
        }
        _ => {
            println!("Unknown command: '{}'", command);
        }
    }
}