# mpm - Minecraft Plugin Manager

mpm is a native Rust-based CLI for Minecraft servers. It brings modern DevOps practices to plugin management, using declarative manifests and lockfiles to ensure every installation is deterministic, verified, and reproducible.

## Features

### 📦 Modern Package Management

- **Manifest-Driven:** Define your environment in `plugins.toml` and eliminate manual `.jar` hunting.
- **Reproducible Installs:** A `plugins.lock` ensures every setup is bit-for-bit identical across all environments.
- **Multi-Source:** Native integration with **Modrinth**, **Hangar**, and **GitHub Releases** APIs. SpigotMC support coming soon.

### 🛡️ Safety & Reliability

- **Integrity Verification:** Automated hash checking for every download to prevent corruption or tampering.
- **Atomic Sync:** Downloads and verifies the entire environment before updating your live folder to prevent broken states.
- **Version Compatibility:** Automatic validation that plugins are compatible with your Minecraft version when adding them.

## 🚀 Coming Soon (In no particular order)

- [ ] **SpigotMC Integration:** Support for downloading plugins from SpigotMC's plugin repository with Minecraft version compatibility checking.
- [ ] **Bukkit Integration:** Support for downloading plugins from the Bukkit plugin repository.
- [ ] **Polymart Integration:** Support for downloading plugins from the Polymart plugin marketplace.
- [ ] **CurseForge Integration:** Support for downloading plugins and mods from CurseForge.
- [ ] **Custom URLs:** Support for direct plugin downloads from custom URLs.
- [ ] **Hosting Panel Integration:** Native support for Pterodactyl and WINGS for seamless, one-click managed deployments.
- [ ] **Expanded Sources:** Support for custom repositories, private mirrors, and direct Jenkins/CI build artifacts.
- [ ] **Version Range Support:** Support for version ranges (e.g., `>=1.20.0,<1.21.0`) in compatibility checking.
- [ ] **GitHub Version Detection:** Parse release notes or tags to detect Minecraft version compatibility for GitHub Releases.

## Installation

### Building from Source

```bash
git clone https://github.com/yipjunkai/mpm.git
cd mpm
cargo build --release
```

The binary will be located at `target/release/mpm`.

## Usage

### Getting Started

#### New Installation

1. **Initialize a new project**:

   ```bash
   mpm init
   ```

   This creates a `plugins.toml` manifest file with the default Minecraft version.

2. **Add plugins**:

   ```bash
   mpm add fabric-api
   mpm add worldedit@7.3.0
   ```

   You can specify a version with `@version`, or omit it to use the latest compatible version. You can specify the source explicitly:

   ```bash
   mpm add modrinth:fabric-api
   mpm add hangar:GeyserMC/Geyser
   mpm add github:PaperMC/Paper@1.20.1
   ```

   **Version Compatibility:** When adding a plugin, mpm automatically checks if it's compatible with the Minecraft version specified in your manifest. If a plugin version is incompatible, you'll get a clear error message with the supported Minecraft versions. This prevents adding plugins that won't work with your server.

3. **Synchronize plugins**:

   ```bash
   mpm sync
   ```

   This downloads all plugins specified in the lockfile to the `plugins/` directory.

#### Existing Installation

If you already have a `plugins/` directory with plugin JAR files, you can import them:

```bash
mpm import
```

This command:

- Scans the `plugins/` directory for JAR files
- Reads plugin metadata from each JAR
- Computes SHA-256 hashes for verification
- Generates `plugins.toml` and `plugins.lock` files

**Note**: The `import` command requires that `plugins.toml` does not already exist. Plugins are marked with source "unknown" since they weren't installed via mpm.

After importing, you can continue using mpm normally:

- Run `mpm sync` to ensure all plugins match the lockfile
- Use `mpm add` and `mpm remove` to manage plugins going forward

### Commands

#### `mpm init [version]`

Initialize a new plugin manifest. Creates `plugins.toml` in the current directory.

- `version`: Minecraft version (default: 1.21.11)

#### `mpm add <spec> [--no-update]`

Add a plugin to the manifest. Automatically validates compatibility with the Minecraft version in your manifest and updates the lockfile.

- `<spec>`: Plugin specification in format `[source:]id[@version]`
  - `fabric-api` - Searches through all sources in priority order
  - `worldedit@7.3.0` - Adds specific version
  - `modrinth:fabric-api` - Explicitly specify Modrinth source
  - `hangar:GeyserMC/Geyser` - Add from Hangar (PaperMC repository)
  - `github:PaperMC/Paper@1.20.1` - Add from GitHub Releases
- `--no-update`: Skip automatic lockfile update after adding

**Version Compatibility Checking:**

When adding a plugin, mpm validates that it's compatible with the Minecraft version specified in your `plugins.toml` manifest:

- **Modrinth & Hangar:** Full compatibility checking - only compatible plugin versions are resolved
- **GitHub:** Warning displayed (GitHub Releases don't include Minecraft version metadata)

If a plugin version is incompatible, you'll receive an error message like:

```text
Error: Plugin 'worldedit' version '7.3.0' is not compatible with Minecraft 1.21.11.
Compatible versions: 1.20, 1.20.1, 1.20.2, 1.20.3, 1.20.4
```

This ensures you only add plugins that will work with your server version.

**Supported sources:**

| Source     | Description                        | Format                                 | MC Version Check | Status       |
| ---------- | ---------------------------------- | -------------------------------------- | ---------------- | ------------ |
| `modrinth` | Modrinth plugin repository         | `plugin-id` or `plugin-id@version`     | ✅ Yes           | ✅ Available |
| `hangar`   | Hangar (PaperMC plugin repository) | `author/slug` or `author/slug@version` | ✅ Yes           | ✅ Available |
| `github`   | GitHub Releases                    | `owner/repo` or `owner/repo@tag`       | ⚠️ Warning only  | ✅ Available |

**Note:** GitHub Releases don't include Minecraft version metadata, so compatibility cannot be automatically verified. A warning will be displayed when adding GitHub plugins.

#### `mpm remove <name> [--no-update]`

Remove a plugin from the manifest. Automatically updates the lockfile.

- `<name>`: Plugin name (as it appears in the manifest)
- `--no-update`: Skip automatic lockfile update after removing

#### `mpm lock [--dry-run]`

Generate or update the lockfile with resolved plugin versions, URLs, and hashes. Only resolves plugin versions that are compatible with the Minecraft version specified in your manifest.

- `--dry-run`: Preview changes without writing the lockfile
  - Exit code 0: No changes needed
  - Exit code 1: Changes would be made

**Note:** The lock command filters plugin versions by Minecraft compatibility, ensuring your lockfile only contains compatible versions. This is especially useful if you manually edit `plugins.toml`.

#### `mpm sync [--dry-run]`

Synchronize the `plugins/` directory with the lockfile. Downloads missing plugins, verifies hashes, and removes unmanaged files.

- `--dry-run`: Preview changes without modifying the plugins directory
  - Exit code 0: No changes needed
  - Exit code 1: Changes would be made

#### `mpm doctor [--json]`

Check plugin manager health. Verifies manifest, lockfile, and plugin files.

- `--json`: Output results in JSON format (useful for CI/CD)
- Exit codes:
  - 0: Healthy (no issues)
  - 1: Warnings only (e.g., unmanaged files)
  - 2: Errors present (e.g., missing files, hash mismatches)

#### `mpm import`

Import existing plugins from the `plugins/` directory. Scans for JAR files, reads plugin metadata, computes hashes, and generates `plugins.toml` and `plugins.lock`.

**Note**: Requires that `plugins.toml` does not already exist.

## File Structure

```text
.
├── plugins.toml      # Plugin manifest (human-editable)
├── plugins.lock      # Lockfile (machine-generated, deterministic)
└── plugins/          # Plugin files directory
    └── *.jar         # Plugin JAR files
```

### plugins.toml

The manifest file defines which plugins to install and the target Minecraft version:

```toml
[minecraft]
version = "1.21.11"

[plugins]
fabric-api = { source = "modrinth", id = "fabric-api" }
worldedit = { source = "modrinth", id = "worldedit", version = "7.3.0" }
```

**Important:** The `[minecraft]` version determines which plugin versions are resolved. When you run `mpm add`, only plugin versions compatible with this Minecraft version will be added. If you manually edit `plugins.toml` and add an incompatible plugin, `mpm lock` will filter it out automatically.

### plugins.lock

The lockfile (automatically generated) contains exact versions, URLs, and hashes:

```toml
[[plugin]]
name = "fabric-api"
source = "modrinth"
version = "0.140.3+26.1"
file = "fabric-api-0.140.3+26.1.jar"
url = "https://cdn.modrinth.com/data/..."
hash = "sha512:..."
```

## Configuration

### Environment Variables

- `PM_DIR`: Override the configuration directory (default: current directory)
- `PM_PLUGINS_DIR`: Override the plugins directory path (default: `{PM_DIR}/plugins/` or `./plugins/` if `PM_DIR` is not set)

### Default Values

- Default Minecraft version: `1.21.11`
- Plugins directory: `plugins/` (relative to config directory, or `PM_PLUGINS_DIR` if set)

## Exit Codes

All commands follow consistent exit code semantics:

- **0**: Success / No changes detected
- **1**: Changes detected / Warnings only
- **2+**: Errors present

This makes mpm suitable for use in CI/CD pipelines and scripts.

## Examples

### Basic Workflow

```bash
# Initialize with your Minecraft version
mpm init 1.20.1

# Add plugins from different sources
# Compatibility is automatically checked
mpm add fabric-api                    # Searches through all sources in priority order
mpm add hangar:GeyserMC/Geyser       # Hangar
mpm add github:PaperMC/Paper         # GitHub Releases (warning shown)

# Sync (downloads plugins)
mpm sync

# Check health
mpm doctor
```

### Version Compatibility Example

```bash
# Initialize for Minecraft 1.20.1
mpm init 1.20.1

# Add a plugin - compatibility is checked automatically
mpm add worldedit@7.3.0              # ✅ Success (compatible with 1.20.1)

# Try adding incompatible version
mpm add some-plugin@latest          # ❌ Error if not compatible with 1.20.1
# Error: No versions of plugin 'some-plugin' are compatible with Minecraft 1.20.1
```

### CI/CD Integration

```bash
# Check health in CI
mpm doctor --json | jq '.exit_code'  # Returns 0, 1, or 2

# Dry-run before deploying
mpm sync --dry-run
if [ $? -eq 1 ]; then
    echo "Plugins need to be updated"
    mpm sync
fi
```

### Importing Existing Plugins

If you have an existing `plugins/` directory with JAR files but no manifest, you can import them:

```bash
mpm import
```

This will:

- Scan the `plugins/` directory for all JAR files
- Extract plugin metadata (name, version) from each JAR
- Compute SHA-256 hashes for verification
- Generate `plugins.toml` and `plugins.lock` files

**Important**:

- The `plugins.toml` file must not exist before running import
- Imported plugins are marked with source "unknown" since they weren't installed via mpm
- After importing, you can use `mpm sync` to ensure everything matches the lockfile

## Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Running

```bash
cargo run -- <command>
```

## License

mpm is licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in **mpm** by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any additional terms or conditions.
