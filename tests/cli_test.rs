use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use zip::CompressionMethod;
use zip::write::{FileOptions, ZipWriter};

fn run_command(args: &[&str], test_dir: &str) -> (bool, String, String) {
    // Use cargo run which will build if needed
    // Set PM_DIR in the environment for the subprocess
    let output = Command::new("cargo")
        .args(&["run", "--quiet", "--"])
        .args(args)
        .env("PM_DIR", test_dir)
        .current_dir(env::current_dir().unwrap())
        .output()
        .expect("Failed to execute command");

    let success = output.status.success();
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    let stderr = String::from_utf8(output.stderr).unwrap_or_default();

    // Filter out cargo compilation messages from stderr
    let filtered_stderr: String = stderr
        .lines()
        .filter(|line| {
            !line.contains("Compiling")
                && !line.contains("Finished")
                && !line.contains("warning:")
                && !line.contains("note:")
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Combine stdout and filtered stderr for checking messages
    let combined_output = if stdout.is_empty() {
        filtered_stderr.clone()
    } else if filtered_stderr.is_empty() {
        stdout.clone()
    } else {
        format!("{}\n{}", stdout, filtered_stderr)
    };

    (success, combined_output, filtered_stderr)
}

fn setup_test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

#[test]
fn test_init_creates_manifest() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    let (success, output, _) = run_command(&["init", "1.21.0"], test_dir);

    assert!(success, "Init command should succeed. output: {}", output);
    assert!(
        output.contains("Initialized plugins.toml"),
        "Expected 'Initialized plugins.toml' in output: {}",
        output
    );

    let manifest_path = format!("{}/plugins.toml", test_dir);
    assert!(
        Path::new(&manifest_path).exists(),
        "Manifest file should be created"
    );

    // Verify content
    let content = fs::read_to_string(&manifest_path).unwrap();
    assert!(content.contains("1.21.0"));
    assert!(content.contains("[minecraft]"));
    assert!(content.contains("[plugins]"));
}

#[test]
fn test_init_with_default_version() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    let (success, _, _) = run_command(&["init"], test_dir);

    assert!(success, "Init command should succeed with default version");

    let manifest_path = format!("{}/plugins.toml", test_dir);
    let content = fs::read_to_string(&manifest_path).unwrap();
    assert!(content.contains("1.21.11")); // Default version from CLI
}

#[test]
fn test_init_skips_if_exists() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // First init
    let (success1, _, _) = run_command(&["init"], test_dir);
    assert!(success1);

    // Second init should skip
    let (success2, stdout, stderr) = run_command(&["init"], test_dir);
    assert!(
        success2,
        "Second init should succeed. stdout: {}, stderr: {}",
        stdout, stderr
    );
    assert!(
        stdout.contains("Manifest detected") || stdout.contains("Skipping"),
        "Expected 'Manifest detected' in output: {}",
        stdout
    );
}

#[test]
fn test_add_plugin() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Init first
    run_command(&["init"], test_dir);

    // Add plugin
    let (success, output, _) = run_command(&["add", "modrinth:fabric-api"], test_dir);

    assert!(success, "Add command should succeed. output: {}", output);
    assert!(
        output.contains("Added plugin 'fabric-api'"),
        "Expected 'Added plugin' in output: {}",
        output
    );

    // Verify manifest contains the plugin
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let content = fs::read_to_string(&manifest_path).unwrap();
    assert!(content.contains("fabric-api"));
    assert!(content.contains("modrinth"));
}

#[test]
fn test_add_plugin_with_version() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);

    let (success, output, _) = run_command(&["add", "modrinth:worldedit@7.3.0"], test_dir);

    assert!(success, "Add command should succeed. output: {}", output);
    assert!(
        output.contains("Added plugin 'worldedit'"),
        "Expected 'Added plugin' in output: {}",
        output
    );

    let manifest_path = format!("{}/plugins.toml", test_dir);
    let content = fs::read_to_string(&manifest_path).unwrap();
    assert!(content.contains("worldedit"));
    assert!(content.contains("7.3.0"));
}

#[test]
fn test_add_multiple_plugins() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);

    let (success, _, _) = run_command(&["add", "modrinth:worldedit@7.3.0"], test_dir);

    assert!(success);

    let manifest_path = format!("{}/plugins.toml", test_dir);
    let content = fs::read_to_string(&manifest_path).unwrap();
    assert!(content.matches("fabric-api").count() >= 1);
    assert!(content.contains("worldedit"));
}

#[test]
fn test_add_fails_without_init() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    let (success, output, _) = run_command(&["add", "modrinth:fabric-api"], test_dir);

    assert!(!success, "Add should fail without init. output: {}", output);
    assert!(
        output.contains("Manifest not found") || output.contains("Run 'pm init' first"),
        "Expected error message in output: {}",
        output
    );
}

#[test]
fn test_add_fails_with_invalid_spec() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);

    let (success, output, _) = run_command(&["add", "invalid-spec"], test_dir);

    assert!(
        !success,
        "Add should fail with invalid spec. output: {}",
        output
    );
    assert!(
        output.contains("Invalid spec format"),
        "Expected 'Invalid spec format' in output: {}",
        output
    );
}

#[test]
fn test_remove_plugin() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);

    let (success, output, _) = run_command(&["remove", "fabric-api"], test_dir);

    assert!(success, "Remove command should succeed. output: {}", output);
    assert!(
        output.contains("Removed plugin 'fabric-api'"),
        "Expected 'Removed plugin' in output: {}",
        output
    );

    // Verify plugin is removed
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let content = fs::read_to_string(&manifest_path).unwrap();
    assert!(!content.contains("fabric-api"));
}

#[test]
fn test_remove_nonexistent_plugin() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);

    let (success, output, _) = run_command(&["remove", "nonexistent"], test_dir);

    assert!(
        !success,
        "Remove should fail for nonexistent plugin. output: {}",
        output
    );
    assert!(
        output.contains("not found in manifest"),
        "Expected 'not found in manifest' in output: {}",
        output
    );
}

#[test]
fn test_remove_fails_without_init() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    let (success, output, _) = run_command(&["remove", "fabric-api"], test_dir);

    assert!(
        !success,
        "Remove should fail without init. output: {}",
        output
    );
    assert!(
        output.contains("Manifest not found") || output.contains("Run 'pm init' first"),
        "Expected error message in output: {}",
        output
    );
}

#[test]
fn test_add_and_remove_workflow() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Init
    run_command(&["init"], test_dir);

    // Add two plugins
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["add", "modrinth:worldedit@7.3.0"], test_dir);

    // Remove one
    run_command(&["remove", "fabric-api"], test_dir);

    // Verify only worldedit remains
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let content = fs::read_to_string(&manifest_path).unwrap();
    assert!(!content.contains("fabric-api"));
    assert!(content.contains("worldedit"));
}

#[test]
fn test_lock_creates_lockfile() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Init and add plugins
    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);

    let (success, output, _) = run_command(&["lock"], test_dir);

    assert!(success, "Lock command should succeed. output: {}", output);
    assert!(output.contains("Locked"));

    // Verify lockfile was created
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    assert!(
        Path::new(&lockfile_path).exists(),
        "Lockfile should be created"
    );

    // Verify lockfile content
    let content = fs::read_to_string(&lockfile_path).unwrap();
    assert!(content.contains("fabric-api"));
    assert!(content.contains("modrinth"));
    assert!(content.contains("version"));
    assert!(content.contains("url"));
    assert!(content.contains("hash"));
}

#[test]
fn test_lock_with_specified_version() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);

    let (success, _, _) = run_command(&["lock"], test_dir);
    assert!(success);

    // Get the version from lockfile
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let content1 = fs::read_to_string(&lockfile_path).unwrap();

    // Run lock again - should produce same result (deterministic)
    run_command(&["lock"], test_dir);
    let content2 = fs::read_to_string(&lockfile_path).unwrap();

    assert_eq!(content1, content2, "Lockfile should be deterministic");
}

#[test]
fn test_lock_sorts_plugins() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    // Add plugins in non-alphabetical order
    run_command(&["add", "modrinth:worldedit"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);

    let (success, _, _) = run_command(&["lock"], test_dir);
    assert!(success);

    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let content = fs::read_to_string(&lockfile_path).unwrap();

    // Find positions of plugin names
    let fabric_pos = content.find("fabric-api").unwrap();
    let worldedit_pos = content.find("worldedit").unwrap();

    // fabric-api should come before worldedit (alphabetically)
    assert!(
        fabric_pos < worldedit_pos,
        "Plugins should be sorted alphabetically"
    );
}

#[test]
fn test_lock_fails_without_init() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    let (success, output, _) = run_command(&["lock"], test_dir);

    assert!(
        !success,
        "Lock should fail without init. output: {}",
        output
    );
    assert!(
        output.contains("Manifest not found") || output.contains("Run 'pm init' first"),
        "Expected error message in output: {}",
        output
    );
}

#[test]
fn test_lock_deterministic_multiple_runs() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["add", "modrinth:worldedit"], test_dir);

    // Run lock multiple times
    run_command(&["lock"], test_dir);
    let content1 = fs::read_to_string(&format!("{}/plugins.lock", test_dir)).unwrap();

    run_command(&["lock"], test_dir);
    let content2 = fs::read_to_string(&format!("{}/plugins.lock", test_dir)).unwrap();

    run_command(&["lock"], test_dir);
    let content3 = fs::read_to_string(&format!("{}/plugins.lock", test_dir)).unwrap();

    // All runs should produce identical lockfiles
    assert_eq!(
        content1, content2,
        "First and second lock should be identical"
    );
    assert_eq!(
        content2, content3,
        "Second and third lock should be identical"
    );
    assert_eq!(
        content1, content3,
        "First and third lock should be identical"
    );
}

#[test]
fn test_lock_dry_run_previews_changes() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Init and add plugins
    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);

    let (success, output, _) = run_command(&["lock", "--dry-run"], test_dir);

    // Exit code 1 = changes detected (no lockfile exists, so it would be created)
    assert!(
        !success,
        "Lock --dry-run should exit with code 1 (changes detected). output: {}",
        output
    );
    assert!(
        output.contains("[DRY RUN]") || output.contains("Would lock"),
        "Expected dry-run message in output: {}",
        output
    );
    assert!(
        output.contains("Resolving") || output.contains("fabric-api"),
        "Expected plugin resolution in output: {}",
        output
    );

    // Verify lockfile was NOT created
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    assert!(
        !Path::new(&lockfile_path).exists(),
        "Lockfile should NOT be created in dry-run mode"
    );
}

#[test]
fn test_lock_dry_run_vs_normal_lock() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);

    // Run lock --dry-run first
    let (success1, output1, _) = run_command(&["lock", "--dry-run"], test_dir);
    // Exit code 1 = changes detected (no lockfile exists, so it would be created)
    assert!(
        !success1,
        "Lock --dry-run should exit with code 1 (changes detected). output: {}",
        output1
    );

    let lockfile_path = format!("{}/plugins.lock", test_dir);
    assert!(
        !Path::new(&lockfile_path).exists(),
        "Lockfile should not exist after dry-run"
    );

    // Now run normal lock
    let (success2, output2, _) = run_command(&["lock"], test_dir);
    assert!(success2, "Lock should succeed. output: {}", output2);
    assert!(
        output2.contains("Locked"),
        "Expected 'Locked' message in output: {}",
        output2
    );

    // Verify lockfile was created
    assert!(
        Path::new(&lockfile_path).exists(),
        "Lockfile should be created after normal lock"
    );
}

#[test]
fn test_doctor_fails_without_lockfile() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);

    let (success, output, _) = run_command(&["doctor"], test_dir);

    assert!(
        !success,
        "Doctor should fail without lockfile. output: {}",
        output
    );
    assert!(
        output.contains("plugins.lock") && output.contains("not found"),
        "Expected error message about lockfile in output: {}",
        output
    );
}

#[test]
fn test_doctor_passes_with_synced_plugins() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    let (success, output, _) = run_command(&["doctor"], test_dir);

    assert!(
        success,
        "Doctor should pass with synced plugins. output: {}",
        output
    );
    assert!(
        output.contains("✅") && output.contains("check(s) passed"),
        "Expected success markers in output: {}",
        output
    );
}

#[test]
fn test_doctor_detects_missing_files() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    // Don't sync - files should be missing

    let (success, output, _) = run_command(&["doctor"], test_dir);

    assert!(
        !success,
        "Doctor should fail with missing files. output: {}",
        output
    );
    assert!(
        output.contains("File not found") || output.contains("❌"),
        "Expected error about missing file in output: {}",
        output
    );
}

#[test]
fn test_doctor_detects_hash_mismatch() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    // Corrupt a plugin file
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    let filename_line = lockfile_content
        .lines()
        .find(|l| l.contains("file ="))
        .unwrap();
    let filename = filename_line.split('"').nth(1).unwrap();
    let plugin_path = format!("{}/plugins/{}", test_dir, filename);
    fs::write(&plugin_path, b"corrupted content").unwrap();

    let (success, output, _) = run_command(&["doctor"], test_dir);

    assert!(
        !success,
        "Doctor should fail with hash mismatch. output: {}",
        output
    );
    assert!(
        output.contains("Hash mismatch") || output.contains("❌"),
        "Expected hash mismatch error in output: {}",
        output
    );
}

#[test]
fn test_doctor_detects_unmanaged_files() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    // Add an unmanaged file
    let unmanaged_file = format!("{}/plugins/unmanaged-plugin.jar", test_dir);
    fs::write(&unmanaged_file, b"fake plugin").unwrap();

    let (success, output, _) = run_command(&["doctor"], test_dir);

    // Doctor should fail with exit code 1 (drift) when warnings are present
    assert!(
        !success,
        "Doctor should fail with warnings (drift). output: {}",
        output
    );
    assert!(
        output.contains("Unmanaged file") || output.contains("⚠️"),
        "Expected warning about unmanaged file in output: {}",
        output
    );
}

#[test]
fn test_doctor_detects_wrong_filename() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    // Rename a plugin file to wrong name
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    let filename_line = lockfile_content
        .lines()
        .find(|l| l.contains("file ="))
        .unwrap();
    let filename = filename_line.split('"').nth(1).unwrap();
    let plugin_path = format!("{}/plugins/{}", test_dir, filename);
    let wrong_path = format!("{}/plugins/wrong-name.jar", test_dir);
    fs::rename(&plugin_path, &wrong_path).unwrap();

    let (success, output, _) = run_command(&["doctor"], test_dir);

    assert!(
        !success,
        "Doctor should fail with wrong filename. output: {}",
        output
    );
    assert!(
        output.contains("not found")
            || output.contains("Filename mismatch")
            || output.contains("❌"),
        "Expected error about filename in output: {}",
        output
    );
}

#[test]
fn test_doctor_passes_after_sync() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["add", "modrinth:worldedit"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    let (success, output, _) = run_command(&["doctor"], test_dir);

    assert!(success, "Doctor should pass after sync. output: {}", output);
    assert!(
        output.contains("✅") && !output.contains("❌"),
        "Expected all checks to pass in output: {}",
        output
    );
}

#[test]
fn test_doctor_json_output_healthy() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    let (success, output, _) = run_command(&["doctor", "--json"], test_dir);

    assert!(success, "Doctor should pass. output: {}", output);

    // Extract JSON from output (JSON should be in stdout, may have trailing stderr)
    let json_start = output.find('{').expect("Should contain JSON");
    let json_str = &output[json_start..];
    // Find the end of JSON (last closing brace)
    let json_end = json_str.rfind('}').expect("Should have closing brace") + 1;
    let json_str = &json_str[..json_end];

    let json: serde_json::Value = serde_json::from_str(json_str).expect("Should be valid JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["status"], "healthy");
    assert!(json["summary"]["ok"].as_u64().unwrap() > 0);
}

#[test]
fn test_doctor_json_output_drift() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    // Add an unmanaged file to create drift
    let unmanaged_file = format!("{}/plugins/unmanaged-plugin.jar", test_dir);
    fs::write(&unmanaged_file, b"fake plugin").unwrap();

    let (success, output, _) = run_command(&["doctor", "--json"], test_dir);

    assert!(
        !success,
        "Doctor should fail with drift. output: {}",
        output
    );

    // Extract JSON from output
    let json_start = output.find('{').expect("Should contain JSON");
    let json_str = &output[json_start..];
    let json_end = json_str.rfind('}').expect("Should have closing brace") + 1;
    let json_str = &json_str[..json_end];

    let json: serde_json::Value = serde_json::from_str(json_str).expect("Should be valid JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["status"], "drift");
    assert!(json["summary"]["warnings"].as_u64().unwrap() > 0);
}

#[test]
fn test_doctor_json_output_failure() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    // Don't sync - files should be missing

    let (success, output, _) = run_command(&["doctor", "--json"], test_dir);

    assert!(
        !success,
        "Doctor should fail with errors. output: {}",
        output
    );

    // Extract JSON from output
    let json_start = output.find('{').expect("Should contain JSON");
    let json_str = &output[json_start..];
    let json_end = json_str.rfind('}').expect("Should have closing brace") + 1;
    let json_str = &json_str[..json_end];

    let json: serde_json::Value = serde_json::from_str(json_str).expect("Should be valid JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["status"], "failure");
    assert!(json["summary"]["errors"].as_u64().unwrap() > 0);
}

#[test]
fn test_sync_fails_without_lockfile() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);

    let (success, output, _) = run_command(&["sync"], test_dir);

    assert!(
        !success,
        "Sync should fail without lockfile. output: {}",
        output
    );
    assert!(
        output.contains("Lockfile not found") || output.contains("Run 'pm lock' first"),
        "Expected error message in output: {}",
        output
    );
}

#[test]
fn test_sync_downloads_plugins() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);

    let (success, output, _) = run_command(&["sync"], test_dir);

    assert!(success, "Sync command should succeed. output: {}", output);
    assert!(
        output.contains("Synced") || output.contains("verified"),
        "Expected sync success message in output: {}",
        output
    );

    // Verify plugin file was created
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();

    // Extract filename from lockfile
    let filename_line = lockfile_content
        .lines()
        .find(|l| l.contains("file ="))
        .unwrap();
    let filename = filename_line.split('"').nth(1).unwrap();

    let plugin_path = format!("{}/plugins/{}", test_dir, filename);
    assert!(
        Path::new(&plugin_path).exists(),
        "Plugin file should be created: {}",
        plugin_path
    );
}

#[test]
fn test_sync_is_idempotent() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);

    // First sync
    let (success1, output1, _) = run_command(&["sync"], test_dir);
    assert!(success1, "First sync should succeed. output: {}", output1);

    // Get file modification time
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    let filename_line = lockfile_content
        .lines()
        .find(|l| l.contains("file ="))
        .unwrap();
    let filename = filename_line.split('"').nth(1).unwrap();
    let plugin_path = format!("{}/plugins/{}", test_dir, filename);
    let metadata1 = fs::metadata(&plugin_path).unwrap();

    // Second sync should skip downloads
    let (success2, output2, _) = run_command(&["sync"], test_dir);
    assert!(success2, "Second sync should succeed. output: {}", output2);
    assert!(
        output2.contains("already synced") || output2.contains("Synced"),
        "Expected idempotent behavior in output: {}",
        output2
    );

    // File should not have been modified (or at least should still exist)
    let metadata2 = fs::metadata(&plugin_path).unwrap();
    assert_eq!(
        metadata1.len(),
        metadata2.len(),
        "File size should not change on second sync"
    );
}

#[test]
fn test_sync_removes_unmanaged_files() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    // Add an unmanaged file
    let unmanaged_file = format!("{}/plugins/unmanaged-plugin.jar", test_dir);
    fs::write(&unmanaged_file, b"fake plugin content").unwrap();
    assert!(
        Path::new(&unmanaged_file).exists(),
        "Unmanaged file should exist before sync"
    );

    // Sync should remove it
    let (success, output, _) = run_command(&["sync"], test_dir);
    assert!(success, "Sync should succeed. output: {}", output);
    assert!(
        output.contains("Removing unmanaged file") || !Path::new(&unmanaged_file).exists(),
        "Expected unmanaged file removal in output: {}",
        output
    );

    // Verify unmanaged file is gone
    assert!(
        !Path::new(&unmanaged_file).exists(),
        "Unmanaged file should be removed"
    );
}

#[test]
fn test_sync_preserves_config_files() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);

    // Create a custom file in the directory
    let custom_file = format!("{}/custom.txt", test_dir);
    fs::write(&custom_file, b"custom content").unwrap();

    let (success, _, _) = run_command(&["sync"], test_dir);
    assert!(success, "Sync should succeed");

    // Verify config files still exist
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    assert!(
        Path::new(&manifest_path).exists(),
        "plugins.toml should be preserved"
    );
    assert!(
        Path::new(&lockfile_path).exists(),
        "plugins.lock should be preserved"
    );
    assert!(
        Path::new(&custom_file).exists(),
        "Non-.jar files should be preserved"
    );
}

#[test]
fn test_sync_full_workflow() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Setup
    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["add", "modrinth:worldedit"], test_dir);
    run_command(&["lock"], test_dir);

    // Sync
    let (success, output, _) = run_command(&["sync"], test_dir);
    assert!(success, "Sync should succeed. output: {}", output);
    assert!(
        output.contains("Synced 2 plugin"),
        "Expected sync message in output: {}",
        output
    );

    // Verify both plugins are downloaded
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();

    let filenames: Vec<&str> = lockfile_content
        .lines()
        .filter_map(|l| {
            if l.contains("file =") {
                l.split('"').nth(1)
            } else {
                None
            }
        })
        .collect();

    for filename in filenames {
        let plugin_path = format!("{}/plugins/{}", test_dir, filename);
        assert!(
            Path::new(&plugin_path).exists(),
            "Plugin file should exist: {}",
            plugin_path
        );
    }
}

#[test]
fn test_sync_dry_run_previews_changes() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Setup
    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);

    let (success, output, _) = run_command(&["sync", "--dry-run"], test_dir);

    // Exit code 1 = changes detected (plugins need to be downloaded)
    assert!(
        !success,
        "Sync --dry-run should exit with code 1 (changes detected). output: {}",
        output
    );
    assert!(
        output.contains("[DRY RUN]") || output.contains("Would"),
        "Expected dry-run message in output: {}",
        output
    );

    // Verify plugins directory was NOT modified
    let plugins_dir = format!("{}/plugins", test_dir);
    if Path::new(&plugins_dir).exists() {
        let entries: Vec<_> = fs::read_dir(&plugins_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("jar"))
            .collect();
        assert!(
            entries.is_empty(),
            "No plugin files should be downloaded in dry-run mode"
        );
    }
}

#[test]
fn test_sync_dry_run_shows_unmanaged_files() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Setup
    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);
    run_command(&["sync"], test_dir);

    // Add an unmanaged file
    let plugins_dir = format!("{}/plugins", test_dir);
    let unmanaged_file = format!("{}/unmanaged-plugin.jar", plugins_dir);
    fs::write(&unmanaged_file, b"fake plugin content").unwrap();

    let (success, output, _) = run_command(&["sync", "--dry-run"], test_dir);

    // Exit code 1 = changes detected (unmanaged file would be removed)
    assert!(
        !success,
        "Sync --dry-run should exit with code 1 (changes detected). output: {}",
        output
    );
    assert!(
        output.contains("Would remove unmanaged file") || output.contains("unmanaged-plugin"),
        "Expected unmanaged file warning in output: {}",
        output
    );

    // Verify unmanaged file still exists (not actually removed)
    assert!(
        Path::new(&unmanaged_file).exists(),
        "Unmanaged file should still exist after dry-run"
    );
}

#[test]
fn test_sync_dry_run_vs_normal_sync() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Setup
    run_command(&["init"], test_dir);
    run_command(&["add", "modrinth:fabric-api"], test_dir);
    run_command(&["lock"], test_dir);

    // Run sync --dry-run first
    let (success1, output1, _) = run_command(&["sync", "--dry-run"], test_dir);
    // Exit code 1 = changes detected (plugins need to be downloaded)
    assert!(
        !success1,
        "Sync --dry-run should exit with code 1 (changes detected). output: {}",
        output1
    );
    assert!(
        output1.contains("[DRY RUN]") || output1.contains("Would"),
        "Expected dry-run message in output: {}",
        output1
    );

    // Verify no files were downloaded
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    let filename_line = lockfile_content
        .lines()
        .find(|l| l.contains("file ="))
        .unwrap();
    let filename = filename_line.split('"').nth(1).unwrap();
    let plugin_path = format!("{}/plugins/{}", test_dir, filename);
    assert!(
        !Path::new(&plugin_path).exists(),
        "Plugin file should not exist after dry-run"
    );

    // Now run normal sync
    let (success2, output2, _) = run_command(&["sync"], test_dir);
    assert!(success2, "Sync should succeed. output: {}", output2);
    assert!(
        output2.contains("Synced"),
        "Expected 'Synced' message in output: {}",
        output2
    );

    // Verify plugin file was created
    assert!(
        Path::new(&plugin_path).exists(),
        "Plugin file should be created after normal sync"
    );
}

// Helper function to create a test JAR file with plugin.yml
fn create_test_jar(
    jar_path: &Path,
    plugin_name: &str,
    plugin_version: Option<&str>,
) -> std::io::Result<()> {
    let file = fs::File::create(jar_path)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::<()>::default().compression_method(CompressionMethod::Stored);

    // Create plugin.yml content
    let mut plugin_yml = format!("name: {}\n", plugin_name);
    if let Some(version) = plugin_version {
        plugin_yml.push_str(&format!("version: {}\n", version));
    }
    plugin_yml.push_str("main: com.example.TestPlugin\n");

    zip.start_file("plugin.yml", options)?;
    zip.write_all(plugin_yml.as_bytes())?;
    zip.finish()?;

    Ok(())
}

// Helper function to create a test JAR file without plugin.yml
fn create_empty_jar(jar_path: &Path) -> std::io::Result<()> {
    let file = fs::File::create(jar_path)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::<()>::default().compression_method(CompressionMethod::Stored);

    // Just add a dummy file
    zip.start_file("META-INF/MANIFEST.MF", options)?;
    zip.write_all(b"Manifest-Version: 1.0\n")?;
    zip.finish()?;

    Ok(())
}

#[test]
fn test_import_creates_manifest_and_lockfile() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Create plugins directory
    let plugins_dir = format!("{}/plugins", test_dir);
    fs::create_dir_all(&plugins_dir).unwrap();

    // Create a test JAR file
    let jar_path = format!("{}/test-plugin.jar", plugins_dir);
    create_test_jar(Path::new(&jar_path), "TestPlugin", Some("1.0.0")).unwrap();

    // Run import
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(success, "Import command should succeed. output: {}", output);
    assert!(
        output.contains("Imported 1 plugin"),
        "Expected 'Imported 1 plugin' in output: {}",
        output
    );
    assert!(
        output.contains("TestPlugin"),
        "Expected plugin name in output: {}",
        output
    );

    // Verify manifest was created
    let manifest_path = format!("{}/plugins.toml", test_dir);
    assert!(
        Path::new(&manifest_path).exists(),
        "Manifest file should be created"
    );

    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("TestPlugin"));
    assert!(manifest_content.contains("source = \"unknown\""));
    assert!(manifest_content.contains("id = \"TestPlugin\""));

    // Verify lockfile was created
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    assert!(
        Path::new(&lockfile_path).exists(),
        "Lockfile should be created"
    );

    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    assert!(lockfile_content.contains("TestPlugin"));
    assert!(lockfile_content.contains("test-plugin.jar"));
    assert!(lockfile_content.contains("sha256:"));
    assert!(lockfile_content.contains("unknown://"));
}

#[test]
fn test_import_multiple_plugins() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Create plugins directory
    let plugins_dir = format!("{}/plugins", test_dir);
    fs::create_dir_all(&plugins_dir).unwrap();

    // Create multiple test JAR files
    create_test_jar(
        Path::new(&format!("{}/plugin1.jar", plugins_dir)),
        "PluginOne",
        Some("1.0.0"),
    )
    .unwrap();
    create_test_jar(
        Path::new(&format!("{}/plugin2.jar", plugins_dir)),
        "PluginTwo",
        Some("2.0.0"),
    )
    .unwrap();

    // Run import
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(success, "Import command should succeed. output: {}", output);
    assert!(
        output.contains("Imported 2 plugin"),
        "Expected 'Imported 2 plugin' in output: {}",
        output
    );

    // Verify both plugins are in manifest
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("PluginOne"));
    assert!(manifest_content.contains("PluginTwo"));

    // Verify both plugins are in lockfile
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    assert!(lockfile_content.contains("PluginOne"));
    assert!(lockfile_content.contains("PluginTwo"));
    assert!(lockfile_content.contains("plugin1.jar"));
    assert!(lockfile_content.contains("plugin2.jar"));
}

#[test]
fn test_import_without_plugin_yml() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Create plugins directory
    let plugins_dir = format!("{}/plugins", test_dir);
    fs::create_dir_all(&plugins_dir).unwrap();

    // Create a JAR file without plugin.yml
    let jar_path = format!("{}/no-plugin-yml.jar", plugins_dir);
    create_empty_jar(Path::new(&jar_path)).unwrap();

    // Run import
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(success, "Import command should succeed. output: {}", output);
    assert!(
        output.contains("Imported 1 plugin"),
        "Expected 'Imported 1 plugin' in output: {}",
        output
    );

    // Verify manifest uses filename as plugin name
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("no-plugin-yml"));
}

#[test]
fn test_import_without_version() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Create plugins directory
    let plugins_dir = format!("{}/plugins", test_dir);
    fs::create_dir_all(&plugins_dir).unwrap();

    // Create a JAR file with plugin.yml but no version
    let jar_path = format!("{}/no-version.jar", plugins_dir);
    create_test_jar(Path::new(&jar_path), "NoVersionPlugin", None).unwrap();

    // Run import
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(success, "Import command should succeed. output: {}", output);

    // Verify manifest doesn't have version
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("NoVersionPlugin"));

    // Verify lockfile uses filename as version fallback
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    assert!(lockfile_content.contains("no-version.jar"));
}

#[test]
fn test_import_fails_when_manifest_exists() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Create manifest first
    run_command(&["init"], test_dir);

    // Create plugins directory with a JAR
    let plugins_dir = format!("{}/plugins", test_dir);
    fs::create_dir_all(&plugins_dir).unwrap();
    create_test_jar(
        Path::new(&format!("{}/test.jar", plugins_dir)),
        "TestPlugin",
        Some("1.0.0"),
    )
    .unwrap();

    // Run import - should fail
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(
        !success,
        "Import should fail when manifest exists. output: {}",
        output
    );
    assert!(
        output.contains("plugins.toml already exists") || output.contains("Remove it first"),
        "Expected error message in output: {}",
        output
    );
}

#[test]
fn test_import_fails_when_plugins_dir_missing() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Don't create plugins directory

    // Run import - should fail
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(
        !success,
        "Import should fail when plugins directory doesn't exist. output: {}",
        output
    );
    assert!(
        output.contains("does not exist") || output.contains("Plugins directory"),
        "Expected error message in output: {}",
        output
    );
}

#[test]
fn test_import_empty_plugins_directory() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Create empty plugins directory
    let plugins_dir = format!("{}/plugins", test_dir);
    fs::create_dir_all(&plugins_dir).unwrap();

    // Run import
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(success, "Import command should succeed. output: {}", output);
    assert!(
        output.contains("No JAR files found") || output.contains("Created empty"),
        "Expected empty directory message in output: {}",
        output
    );

    // Verify empty manifest and lockfile were created
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let lockfile_path = format!("{}/plugins.lock", test_dir);
    assert!(
        Path::new(&manifest_path).exists(),
        "Manifest file should be created"
    );
    assert!(
        Path::new(&lockfile_path).exists(),
        "Lockfile should be created"
    );
}

#[test]
fn test_import_ignores_non_jar_files() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    // Create plugins directory
    let plugins_dir = format!("{}/plugins", test_dir);
    fs::create_dir_all(&plugins_dir).unwrap();

    // Create a JAR file
    create_test_jar(
        Path::new(&format!("{}/plugin.jar", plugins_dir)),
        "TestPlugin",
        Some("1.0.0"),
    )
    .unwrap();

    // Create a non-JAR file
    fs::write(format!("{}/not-a-plugin.txt", plugins_dir), b"content").unwrap();

    // Run import
    let (success, output, _) = run_command(&["import"], test_dir);

    assert!(success, "Import command should succeed. output: {}", output);
    assert!(
        output.contains("Imported 1 plugin"),
        "Expected only 1 plugin (JAR file) in output: {}",
        output
    );

    // Verify only the JAR is in manifest
    let manifest_path = format!("{}/plugins.toml", test_dir);
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("TestPlugin"));
    assert!(!manifest_content.contains("not-a-plugin"));
}
