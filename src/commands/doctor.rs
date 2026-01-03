// Doctor module for health checking

use crate::commands::sync::verify_plugin_hash;
use crate::config;
use crate::constants;
use crate::lockfile::Lockfile;
use crate::manifest::Manifest;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
struct Issue {
    severity: String,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct ManifestInfo {
    present: bool,
    valid: bool,
    path: String,
}

#[derive(Debug, Serialize)]
struct LockfileInfo {
    present: bool,
    valid: bool,
    path: String,
}

#[derive(Debug, Serialize)]
struct PluginsInfo {
    directory_present: bool,
    installed: usize,
    expected: usize,
    missing: Vec<String>,
    hash_mismatch: Vec<String>,
    unmanaged: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DoctorOutput {
    schema_version: u32,
    status: String,
    exit_code: i32,
    manifest: ManifestInfo,
    lockfile: LockfileInfo,
    plugins: PluginsInfo,
    issues: Vec<Issue>,
}

pub fn check_health(json: bool) -> anyhow::Result<i32> {
    let manifest_path = config::manifest_path();
    let lockfile_path = config::lockfile_path();
    let plugins_dir = config::plugins_dir();

    // Check manifest
    let (manifest_info, mut issues) = check_manifest(&manifest_path);

    // Check lockfile (independent check)
    let (lockfile_info, lockfile_opt, lockfile_issues) = check_lockfile(&lockfile_path);
    issues.extend(lockfile_issues);

    // Check plugins (only if lockfile is valid)
    let (plugins_info, plugins_issues) = if let Some(ref lockfile) = lockfile_opt {
        check_plugins(&plugins_dir, lockfile)
    } else {
        // If lockfile is invalid, we can still check if the directory exists
        let dir_present = Path::new(&plugins_dir).exists();
        (
            PluginsInfo {
                directory_present: dir_present,
                installed: 0,
                expected: 0,
                missing: Vec::new(),
                hash_mismatch: Vec::new(),
                unmanaged: Vec::new(),
            },
            Vec::new(), // Don't add extra issues - lockfile issues already cover this
        )
    };
    issues.extend(plugins_issues);

    // Sort issues deterministically by code, then message
    issues.sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.message.cmp(&b.message)));

    // Determine overall status and exit code
    let has_errors = issues.iter().any(|i| i.severity == "error");
    let has_warnings = issues.iter().any(|i| i.severity == "warning");

    let (status, exit_code) = if has_errors {
        ("error".to_string(), 2)
    } else if has_warnings {
        ("warning".to_string(), 1)
    } else {
        ("ok".to_string(), 0)
    };

    let output = DoctorOutput {
        schema_version: constants::SCHEMA_VERSION,
        status: status.clone(),
        exit_code,
        manifest: manifest_info,
        lockfile: lockfile_info,
        plugins: plugins_info,
        issues,
    };

    if json {
        // Output JSON
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Output human-readable format
        output_human_readable(&output);
    }

    Ok(exit_code)
}

fn check_manifest(path: &str) -> (ManifestInfo, Vec<Issue>) {
    let mut issues = Vec::new();

    let present = Path::new(path).exists();
    let mut valid = false;

    if !present {
        issues.push(Issue {
            severity: "error".to_string(),
            code: "MANIFEST_MISSING".to_string(),
            message: "Manifest file not found".to_string(),
            path: Some(path.to_string()),
        });
    } else {
        match Manifest::load() {
            Ok(_) => {
                valid = true;
            }
            Err(e) => {
                issues.push(Issue {
                    severity: "error".to_string(),
                    code: "MANIFEST_INVALID".to_string(),
                    message: format!("Manifest file is invalid: {}", e),
                    path: Some(path.to_string()),
                });
            }
        }
    }

    (
        ManifestInfo {
            present,
            valid,
            path: path.to_string(),
        },
        issues,
    )
}

fn check_lockfile(path: &str) -> (LockfileInfo, Option<Lockfile>, Vec<Issue>) {
    let mut issues = Vec::new();

    let present = Path::new(path).exists();
    let mut valid = false;
    let mut lockfile_opt = None;

    if !present {
        issues.push(Issue {
            severity: "error".to_string(),
            code: "LOCKFILE_MISSING".to_string(),
            message: "Lockfile not found".to_string(),
            path: Some(path.to_string()),
        });
    } else {
        match Lockfile::load() {
            Ok(lockfile) => {
                valid = true;
                lockfile_opt = Some(lockfile);
            }
            Err(e) => {
                issues.push(Issue {
                    severity: "error".to_string(),
                    code: "LOCKFILE_INVALID".to_string(),
                    message: format!("Lockfile is invalid: {}", e),
                    path: Some(path.to_string()),
                });
            }
        }
    }

    (
        LockfileInfo {
            present,
            valid,
            path: path.to_string(),
        },
        lockfile_opt,
        issues,
    )
}

fn check_plugins(plugins_dir: &str, lockfile: &Lockfile) -> (PluginsInfo, Vec<Issue>) {
    let mut issues = Vec::new();
    let plugins_path = Path::new(plugins_dir);
    let directory_present = plugins_path.exists();

    let expected = lockfile.plugin.len();
    let mut installed = 0;
    let mut missing = Vec::new();
    let mut hash_mismatch = Vec::new();
    let mut unmanaged = Vec::new();

    if !directory_present {
        issues.push(Issue {
            severity: "error".to_string(),
            code: "PLUGINS_DIR_MISSING".to_string(),
            message: "Plugins directory not found".to_string(),
            path: Some(plugins_dir.to_string()),
        });
    } else {
        // Get list of managed filenames
        let managed_files: std::collections::HashSet<String> =
            lockfile.plugin.iter().map(|p| p.file.clone()).collect();

        // Check each plugin in lockfile
        for plugin in &lockfile.plugin {
            let file_path = plugins_path.join(&plugin.file);

            if !file_path.exists() {
                missing.push(plugin.name.clone());
                issues.push(Issue {
                    severity: "error".to_string(),
                    code: "PLUGIN_MISSING".to_string(),
                    message: format!("Plugin '{}' file '{}' not found", plugin.name, plugin.file),
                    path: Some(file_path.to_string_lossy().to_string()),
                });
                continue;
            }

            // Check hash
            match plugin.parse_hash() {
                Ok((algorithm, _)) => match verify_plugin_hash(&file_path, algorithm) {
                    Ok(computed_hash) => {
                        if computed_hash == plugin.hash {
                            installed += 1;
                        } else {
                            hash_mismatch.push(plugin.name.clone());
                            issues.push(Issue {
                                severity: "error".to_string(),
                                code: "HASH_MISMATCH".to_string(),
                                message: format!("Plugin '{}' hash mismatch", plugin.name),
                                path: Some(file_path.to_string_lossy().to_string()),
                            });
                        }
                    }
                    Err(e) => {
                        hash_mismatch.push(plugin.name.clone());
                        issues.push(Issue {
                            severity: "error".to_string(),
                            code: "HASH_MISMATCH".to_string(),
                            message: format!(
                                "Plugin '{}' hash verification failed: {}",
                                plugin.name, e
                            ),
                            path: Some(file_path.to_string_lossy().to_string()),
                        });
                    }
                },
                Err(e) => {
                    hash_mismatch.push(plugin.name.clone());
                    issues.push(Issue {
                        severity: "error".to_string(),
                        code: "HASH_MISMATCH".to_string(),
                        message: format!("Plugin '{}' hash parsing failed: {}", plugin.name, e),
                        path: Some(file_path.to_string_lossy().to_string()),
                    });
                }
            }
        }

        // Check for unmanaged files (sorted for determinism)
        if let Ok(entries) = fs::read_dir(plugins_path) {
            let mut unmanaged_files: Vec<String> = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Some(filename) = path.file_name().and_then(|n| n.to_str())
                    && filename.ends_with(".jar")
                    && !managed_files.contains(filename)
                {
                    unmanaged_files.push(filename.to_string());
                }
            }
            unmanaged_files.sort(); // Deterministic order

            for filename in &unmanaged_files {
                unmanaged.push(filename.clone());
                issues.push(Issue {
                    severity: "warning".to_string(),
                    code: "UNMANAGED_PLUGIN".to_string(),
                    message: format!("Unmanaged plugin file '{}'", filename),
                    path: Some(plugins_path.join(filename).to_string_lossy().to_string()),
                });
            }
        }
    }

    (
        PluginsInfo {
            directory_present,
            installed,
            expected,
            missing,
            hash_mismatch,
            unmanaged,
        },
        issues,
    )
}

fn output_human_readable(output: &DoctorOutput) {
    // 1. Manifest section
    println!("Manifest (plugins.toml)");
    if output.manifest.present && output.manifest.valid {
        println!("  ✓ Present and valid");
    } else if output.manifest.present {
        println!("  ✗ Present but invalid");
    } else {
        println!("  ✗ Not found");
    }

    // 2. Lockfile section
    println!("\nLockfile (plugins.lock)");
    if output.lockfile.present && output.lockfile.valid {
        println!("  ✓ Present and valid");
    } else if output.lockfile.present {
        println!("  ✗ Present but invalid");
    } else {
        println!("  ✗ Not found");
    }

    // 3. Plugins directory section
    let plugins_dir_path = config::plugins_dir();
    println!("\nPlugins directory ({})", plugins_dir_path);
    if output.plugins.directory_present {
        println!("  ✓ Directory exists");
        println!(
            "  Installed: {} / Expected: {}",
            output.plugins.installed, output.plugins.expected
        );

        if !output.plugins.missing.is_empty() {
            for plugin_name in &output.plugins.missing {
                println!("  ✗ Missing: {}", plugin_name);
            }
        }
        if !output.plugins.hash_mismatch.is_empty() {
            for plugin_name in &output.plugins.hash_mismatch {
                println!("  ✗ Hash mismatch: {}", plugin_name);
            }
        }
        if !output.plugins.unmanaged.is_empty() {
            for filename in &output.plugins.unmanaged {
                println!("  ⚠ Unmanaged: {}", filename);
            }
        }
    } else {
        println!("  ✗ Directory not found");
    }

    // 4. Summary
    println!("\nSummary");
    let error_count = output
        .issues
        .iter()
        .filter(|i| i.severity == "error")
        .count();
    let warning_count = output
        .issues
        .iter()
        .filter(|i| i.severity == "warning")
        .count();

    if error_count > 0 {
        println!("  ✗ {} error(s)", error_count);
    }
    if warning_count > 0 {
        println!("  ⚠ {} warning(s)", warning_count);
    }
    if error_count == 0 && warning_count == 0 {
        println!("  ✓ No issues");
    }

    // Status line
    let status_label = match output.status.as_str() {
        "error" => "errors",
        "warning" => "warnings",
        _ => "healthy",
    };
    println!("\nStatus: {}", status_label);
}
