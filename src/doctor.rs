// Doctor module for health checking

use crate::config;
use crate::lockfile::Lockfile;
use crate::manifest::Manifest;
use crate::sync::verify_plugin_hash;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
enum CheckStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
struct CheckResult {
    name: String,
    status: CheckStatus,
    message: String,
}

pub fn check_health() -> anyhow::Result<i32> {
    let mut results = Vec::new();
    let mut has_errors = false;
    let mut has_warnings = false;

    println!("Checking plugin manager health...\n");

    // Check configuration files
    println!("Configuration Files:");
    match check_manifest() {
        Ok(msg) => {
            println!("  ✅ plugins.toml: {}", msg);
            results.push(CheckResult {
                name: "plugins.toml".to_string(),
                status: CheckStatus::Ok,
                message: msg,
            });
        }
        Err(e) => {
            println!("  ❌ plugins.toml: {}", e);
            results.push(CheckResult {
                name: "plugins.toml".to_string(),
                status: CheckStatus::Error,
                message: e.to_string(),
            });
            has_errors = true;
        }
    }

    match check_lockfile() {
        Ok((lockfile, msg)) => {
            println!("  ✅ plugins.lock: {}", msg);
            results.push(CheckResult {
                name: "plugins.lock".to_string(),
                status: CheckStatus::Ok,
                message: msg,
            });

            // Check plugin files
            println!("\nPlugin Files:");
            let (plugin_results, plugin_errors, plugin_warnings) = check_plugin_files(&lockfile);
            results.extend(plugin_results);
            if plugin_errors {
                has_errors = true;
            }
            if plugin_warnings {
                has_warnings = true;
            }

            // Check unmanaged files
            println!("\nUnmanaged Files:");
            let (unmanaged_results, unmanaged_warnings) = check_unmanaged_files(&lockfile);
            results.extend(unmanaged_results);
            if unmanaged_warnings {
                has_warnings = true;
            }
        }
        Err(e) => {
            println!("  ❌ plugins.lock: {}", e);
            results.push(CheckResult {
                name: "plugins.lock".to_string(),
                status: CheckStatus::Error,
                message: e.to_string(),
            });
            has_errors = true;
        }
    }

    // Summary
    println!("\nSummary:");
    let ok_count = results
        .iter()
        .filter(|r| matches!(r.status, CheckStatus::Ok))
        .count();
    let warning_count = results
        .iter()
        .filter(|r| matches!(r.status, CheckStatus::Warning))
        .count();
    let error_count = results
        .iter()
        .filter(|r| matches!(r.status, CheckStatus::Error))
        .count();

    println!("  ✅ {} check(s) passed", ok_count);
    if warning_count > 0 {
        println!("  ⚠️  {} warning(s)", warning_count);
    }
    if error_count > 0 {
        println!("  ❌ {} error(s)", error_count);
    }

    if has_errors {
        Ok(1)
    } else if has_warnings {
        Ok(0) // Warnings don't fail the command
    } else {
        Ok(0)
    }
}

fn check_manifest() -> anyhow::Result<String> {
    let path = config::manifest_path();
    if !Path::new(&path).exists() {
        anyhow::bail!("File not found");
    }

    Manifest::load()?;
    Ok("File exists and parses correctly".to_string())
}

fn check_lockfile() -> anyhow::Result<(Lockfile, String)> {
    let path = config::lockfile_path();
    if !Path::new(&path).exists() {
        anyhow::bail!("File not found");
    }

    let lockfile = Lockfile::load()?;
    let plugin_count = lockfile.plugin.len();
    Ok((
        lockfile,
        format!(
            "File exists and parses correctly ({} plugin(s))",
            plugin_count
        ),
    ))
}

fn check_plugin_files(lockfile: &Lockfile) -> (Vec<CheckResult>, bool, bool) {
    let mut results = Vec::new();
    let mut has_errors = false;
    let has_warnings = false;
    let plugins_dir = config::config_dir();

    for plugin in &lockfile.plugin {
        let file_path = Path::new(&plugins_dir).join(&plugin.file);
        let mut checks_passed = 0;
        let mut checks_total = 0;

        // Check file exists
        checks_total += 1;
        if file_path.exists() {
            checks_passed += 1;
        } else {
            println!("  ❌ {}: File not found ({})", plugin.name, plugin.file);
            results.push(CheckResult {
                name: format!("plugin:{}", plugin.name),
                status: CheckStatus::Error,
                message: format!("File '{}' not found", plugin.file),
            });
            has_errors = true;
            continue;
        }

        // Check filename matches
        checks_total += 1;
        if file_path.file_name().and_then(|n| n.to_str()) == Some(&plugin.file) {
            checks_passed += 1;
        } else {
            println!(
                "  ❌ {}: Filename mismatch (expected: {})",
                plugin.name, plugin.file
            );
            results.push(CheckResult {
                name: format!("plugin:{}", plugin.name),
                status: CheckStatus::Error,
                message: format!("Filename mismatch: expected '{}'", plugin.file),
            });
            has_errors = true;
            continue;
        }

        // Check hash
        checks_total += 1;
        match plugin.parse_hash() {
            Ok((algorithm, _)) => match verify_plugin_hash(&file_path, algorithm) {
                Ok(computed_hash) => {
                    if computed_hash == plugin.hash {
                        checks_passed += 1;
                    } else {
                        println!("  ❌ {}: Hash mismatch", plugin.name);
                        results.push(CheckResult {
                            name: format!("plugin:{}", plugin.name),
                            status: CheckStatus::Error,
                            message: format!("Hash mismatch for '{}'", plugin.file),
                        });
                        has_errors = true;
                        continue;
                    }
                }
                Err(e) => {
                    println!("  ❌ {}: Failed to compute hash: {}", plugin.name, e);
                    results.push(CheckResult {
                        name: format!("plugin:{}", plugin.name),
                        status: CheckStatus::Error,
                        message: format!("Failed to compute hash: {}", e),
                    });
                    has_errors = true;
                    continue;
                }
            },
            Err(e) => {
                println!("  ❌ {}: Failed to parse hash: {}", plugin.name, e);
                results.push(CheckResult {
                    name: format!("plugin:{}", plugin.name),
                    status: CheckStatus::Error,
                    message: format!("Failed to parse hash: {}", e),
                });
                has_errors = true;
                continue;
            }
        }

        // All checks passed
        if checks_passed == checks_total {
            println!(
                "  ✅ {}: File exists, filename matches, hash verified",
                plugin.name
            );
            results.push(CheckResult {
                name: format!("plugin:{}", plugin.name),
                status: CheckStatus::Ok,
                message: format!("All checks passed for '{}'", plugin.file),
            });
        }
    }

    (results, has_errors, has_warnings)
}

fn check_unmanaged_files(lockfile: &Lockfile) -> (Vec<CheckResult>, bool) {
    let mut results = Vec::new();
    let mut has_warnings = false;
    let plugins_dir = config::config_dir();
    let plugins_path = Path::new(&plugins_dir);

    if !plugins_path.exists() {
        return (results, false);
    }

    // Get list of managed filenames
    let managed_files: std::collections::HashSet<String> =
        lockfile.plugin.iter().map(|p| p.file.clone()).collect();

    // Check for unmanaged .jar files
    if let Ok(entries) = fs::read_dir(plugins_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if filename.ends_with(".jar") && !managed_files.contains(filename) {
                            println!("  ⚠️  Unmanaged file: {}", filename);
                            results.push(CheckResult {
                                name: format!("unmanaged:{}", filename),
                                status: CheckStatus::Warning,
                                message: format!("Unmanaged .jar file: '{}'", filename),
                            });
                            has_warnings = true;
                        }
                    }
                }
            }
        }
    }

    if !has_warnings {
        println!("  ✅ No unmanaged .jar files found");
    }

    (results, has_warnings)
}
