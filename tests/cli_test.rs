use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

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
    assert!(content.contains("sha256"));
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
fn test_sync_fails_without_lockfile() {
    let temp_dir = setup_test_dir();
    let test_dir = temp_dir.path().to_str().unwrap();

    run_command(&["init"], test_dir);

    let (success, output, _) = run_command(&["sync"], test_dir);

    assert!(!success, "Sync should fail without lockfile. output: {}", output);
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
    let filename = filename_line
        .split('"')
        .nth(1)
        .unwrap();

    let plugin_path = format!("{}/{}", test_dir, filename);
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
    let filename = filename_line
        .split('"')
        .nth(1)
        .unwrap();
    let plugin_path = format!("{}/{}", test_dir, filename);
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
    let unmanaged_file = format!("{}/unmanaged-plugin.jar", test_dir);
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
        let plugin_path = format!("{}/{}", test_dir, filename);
        assert!(
            Path::new(&plugin_path).exists(),
            "Plugin file should exist: {}",
            plugin_path
        );
    }
}
