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
