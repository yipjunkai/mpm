// Doctor module for health checking

use crate::config;
use crate::constants;
use crate::lockfile::Lockfile;
use crate::manifest::Manifest;
use crate::sync::verify_plugin_hash;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
struct CheckResult {
    name: String,
    status: CheckStatus,
    message: String,
}

#[derive(Debug, Serialize)]
struct DoctorOutput {
    /// Schema version for the JSON output format.
    /// Increment only on breaking changes to ensure future integrations can safely evolve.
    /// See constants::SCHEMA_VERSION for the current version.
    schema_version: u32,
    status: String,
    summary: Summary,
    checks: Vec<CheckResult>,
}

#[derive(Debug, Serialize)]
struct Summary {
    ok: usize,
    warnings: usize,
    errors: usize,
}

pub fn check_health(json: bool) -> anyhow::Result<i32> {
    let mut results = Vec::new();
    let mut has_errors = false;
    let mut has_warnings = false;

    if !json {
        println!("Checking plugin manager health...\n");
    }

    // Check configuration files
    if !json {
        println!("Configuration Files:");
    }
    match check_manifest() {
        Ok(msg) => {
            if !json {
                println!("  ✅ {}: {}", crate::constants::MANIFEST_FILE, msg);
            }
            results.push(CheckResult {
                name: crate::constants::MANIFEST_FILE.to_string(),
                status: CheckStatus::Ok,
                message: msg,
            });
        }
        Err(e) => {
            if !json {
                println!("  ❌ {}: {}", crate::constants::MANIFEST_FILE, e);
            }
            results.push(CheckResult {
                name: crate::constants::MANIFEST_FILE.to_string(),
                status: CheckStatus::Error,
                message: e.to_string(),
            });
            has_errors = true;
        }
    }

    match check_lockfile() {
        Ok((lockfile, msg)) => {
            if !json {
                println!("  ✅ {}: {}", crate::constants::LOCKFILE_FILE, msg);
            }
            results.push(CheckResult {
                name: crate::constants::LOCKFILE_FILE.to_string(),
                status: CheckStatus::Ok,
                message: msg,
            });

            // Check plugin files
            if !json {
                println!("\nPlugin Files:");
            }
            let (plugin_results, plugin_errors, plugin_warnings) =
                check_plugin_files(&lockfile, json);
            results.extend(plugin_results);
            if plugin_errors {
                has_errors = true;
            }
            if plugin_warnings {
                has_warnings = true;
            }

            // Check unmanaged files
            if !json {
                println!("\nUnmanaged Files:");
            }
            let (unmanaged_results, unmanaged_warnings) = check_unmanaged_files(&lockfile, json);
            results.extend(unmanaged_results);
            if unmanaged_warnings {
                has_warnings = true;
            }
        }
        Err(e) => {
            if !json {
                println!("  ❌ {}: {}", crate::constants::LOCKFILE_FILE, e);
            }
            results.push(CheckResult {
                name: crate::constants::LOCKFILE_FILE.to_string(),
                status: CheckStatus::Error,
                message: e.to_string(),
            });
            has_errors = true;
        }
    }

    // Summary
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

    if json {
        // Output JSON
        let status = if has_errors {
            "failure"
        } else if has_warnings {
            "drift"
        } else {
            "healthy"
        };

        let output = DoctorOutput {
            schema_version: constants::SCHEMA_VERSION,
            status: status.to_string(),
            summary: Summary {
                ok: ok_count,
                warnings: warning_count,
                errors: error_count,
            },
            checks: results,
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Output human-readable format
        println!("\nSummary:");
        println!("  ✅ {} check(s) passed", ok_count);
        if warning_count > 0 {
            println!("  ⚠️  {} warning(s)", warning_count);
        }
        if error_count > 0 {
            println!("  ❌ {} error(s)", error_count);
        }
    }

    // Deterministic exit codes:
    // 0 = healthy (no errors, no warnings)
    // 1 = drift (warnings present)
    // 2 = failure (errors present)
    if has_errors {
        Ok(2)
    } else if has_warnings {
        Ok(1)
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

fn check_plugin_files(lockfile: &Lockfile, json: bool) -> (Vec<CheckResult>, bool, bool) {
    let mut results = Vec::new();
    let mut has_errors = false;
    let has_warnings = false;
    let plugins_dir = config::plugins_dir();

    for plugin in &lockfile.plugin {
        let file_path = Path::new(&plugins_dir).join(&plugin.file);
        let mut checks_passed = 0;
        let mut checks_total = 0;

        // Check file exists
        checks_total += 1;
        if file_path.exists() {
            checks_passed += 1;
        } else {
            if !json {
                println!("  ❌ {}: File not found ({})", plugin.name, plugin.file);
            }
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
            if !json {
                println!(
                    "  ❌ {}: Filename mismatch (expected: {})",
                    plugin.name, plugin.file
                );
            }
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
                        if !json {
                            println!("  ❌ {}: Hash mismatch", plugin.name);
                        }
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
                    if !json {
                        println!("  ❌ {}: Failed to compute hash: {}", plugin.name, e);
                    }
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
                if !json {
                    println!("  ❌ {}: Failed to parse hash: {}", plugin.name, e);
                }
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
            if !json {
                println!(
                    "  ✅ {}: File exists, filename matches, hash verified",
                    plugin.name
                );
            }
            results.push(CheckResult {
                name: format!("plugin:{}", plugin.name),
                status: CheckStatus::Ok,
                message: format!("All checks passed for '{}'", plugin.file),
            });
        }
    }

    (results, has_errors, has_warnings)
}

fn check_unmanaged_files(lockfile: &Lockfile, json: bool) -> (Vec<CheckResult>, bool) {
    let mut results = Vec::new();
    let mut has_warnings = false;
    let plugins_dir = config::plugins_dir();
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
                            if !json {
                                println!("  ⚠️  Unmanaged file: {}", filename);
                            }
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

    if !has_warnings && !json {
        println!("  ✅ No unmanaged .jar files found");
    }

    (results, has_warnings)
}
