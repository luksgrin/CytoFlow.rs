# Building from Source

This guide provides detailed instructions for building CytoFlow.rs from source code.

## Project Layout

```
crates/cytoflow-core   pure Rust engine (no UI): FCS reader, transforms, gates, statistics, wsp / Gating-ML readers, tests
src-tauri              Tauri 2 shell exposing the engine as IPC commands (binary name: cytoflow)
src                    Vite + TypeScript user interface
docs                   this documentation (MkDocs); docs/ipc.md is the frontend ⇄ backend contract
testdata               public test fixtures (FlowKit, BSD-3)
scripts                fetch-testdata.sh, check-version.sh, collect-bundles.sh
```

The repository is a Cargo workspace: build outputs go to `target/` at the repository root (not `src-tauri/target/`).

## Prerequisites

### Required for All Platforms

1. **Rust** (latest stable version)
    ```bash
    # Install Rust
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source $HOME/.cargo/env

    # Verify installation
    rustc --version
    cargo --version
    ```

2. **Node.js** (v20 or later; CI uses v22) and npm
    - Download from [nodejs.org](https://nodejs.org/)
    - Or use a version manager:
      ```bash
      # Using nvm (Node Version Manager)
      curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
      nvm install 22
      nvm use 22
      ```
    - Verify installation:
      ```bash
      node --version  # Should be v20 or later
      npm --version
      ```

3. **Tauri CLI** (version 2)
    ```bash
    cargo install tauri-cli --version "^2" --locked
    ```
    (`npm install` also installs `@tauri-apps/cli`, so `npx tauri …` works as an alternative to `cargo tauri …`.)

4. **Git**
    - Usually pre-installed on macOS and Linux
    - Windows: Download from [git-scm.com](https://git-scm.com/)

### Platform-Specific Requirements

#### macOS

1. **Xcode Command Line Tools**
    ```bash
    xcode-select --install
    ```

2. **Verify installation**
    ```bash
    xcode-select -p
    ```

#### Windows

1. **Microsoft Visual C++ Build Tools**
    - Download from [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
    - Install the "Desktop development with C++" workload

2. **Windows SDK**
    - Included with Visual Studio Build Tools

3. **WebView2**
    - Pre-installed on Windows 10 (recent updates) and Windows 11

The MSI bundler (WiX) and the NSIS bundler are downloaded by the Tauri CLI on first use.

#### Linux (Ubuntu/Debian)

Install system dependencies:
```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf
```

These are the packages the CI and release workflows install on Ubuntu 22.04.

## Getting the Source Code

```bash
# Clone the repository
git clone https://github.com/luksgrin/CytoFlow.rs.git
cd CytoFlow.rs

# Install the frontend dependencies
npm install
```

## Development Build

For development and testing:

```bash
cargo tauri dev
```

This will:

- Start the Vite development server on `http://localhost:1420` (`npm run dev`)
- Compile the Rust engine and shell in debug mode and launch the application
- Hot-reload the user interface when files under `src/` change; changes to Rust code rebuild and restart the app

### Browser preview (mock mode)

The user interface can also run in a normal web browser, without Rust:

```bash
npm run dev
```

Open `http://localhost:1420`. When it is not running inside Tauri, the UI uses an in-memory mock of the engine (`src/mock.ts`) with a synthetic demo workspace (three PBMC-like samples with gates). File dialogs become text prompts. The status bar says "Running in the browser with synthetic mock data". This is handy for UI work; the numbers are not real.

### Type checking

```bash
npm run typecheck     # tsc --noEmit
npm run build         # type check + production build of the UI into dist/
```

## Production Build

### Build for Current Platform

```bash
npm install
cargo tauri build
```

`cargo tauri build` runs `npm run build` first (the `beforeBuildCommand`), then compiles the release binary and creates every bundle type available on the platform. The bundles are in `target/release/bundle/`.

!!! note "Updater signing key"
    The configuration also asks for signed in-app update bundles (`bundle.createUpdaterArtifacts`), which need the maintainer's private key. Without it the build stops with "A public key has been found, but no private key". Build without the update bundles instead:
    ```bash
    cargo tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
    ```
    See [Releasing](releasing.md#updater-signing-and-latestjson).

### Build for Specific Platforms

These are the commands the [release workflow](releasing.md) runs.

#### macOS ARM64 (Apple Silicon)

```bash
rustup target add aarch64-apple-darwin
cargo tauri build --target aarch64-apple-darwin --bundles app,dmg
```

Output: `target/aarch64-apple-darwin/release/bundle/`

#### macOS Intel

```bash
rustup target add x86_64-apple-darwin
cargo tauri build --target x86_64-apple-darwin --bundles app,dmg
```

Output: `target/x86_64-apple-darwin/release/bundle/`

The Intel build can be cross-compiled on an Apple Silicon Mac (the release workflow does exactly that).

#### Windows

```bash
rustup target add x86_64-pc-windows-msvc
cargo tauri build --target x86_64-pc-windows-msvc --bundles msi,nsis
```

Output: `target/x86_64-pc-windows-msvc/release/bundle/msi/` and `bundle/nsis/`

#### Linux (Ubuntu/Debian)

```bash
# Install the system dependencies first (see Prerequisites)
rustup target add x86_64-unknown-linux-gnu
cargo tauri build --target x86_64-unknown-linux-gnu --bundles deb,appimage
```

Output: `target/x86_64-unknown-linux-gnu/release/bundle/deb/` and `bundle/appimage/`

Build on the oldest distribution you want to support (the releases are built on Ubuntu 22.04): the result depends on the system's glibc and WebKitGTK versions.

### Version and pre-release numbers

The version lives in three files that must agree: `Cargo.toml` (`[workspace.package] version`), `package.json` and `src-tauri/tauri.conf.json`. Check them with:

```bash
scripts/check-version.sh
```

The Windows MSI bundler only accepts numeric pre-release identifiers (`0.2.0-1`, not `0.2.0-rc.1`).

## Build Output Locations

After building, find your artifacts (`<target>` is omitted from the path when you build without `--target`):

- **macOS**:
    - App: `target/<target>/release/bundle/macos/CytoFlow.app`
    - DMG: `target/<target>/release/bundle/dmg/CytoFlow_<version>_<arch>.dmg`

    **Note**: When running a locally built app that was copied from another machine or downloaded, macOS Gatekeeper may report that it is "damaged and cannot be opened". To fix this, run:
    ```bash
    xattr -cr target/<target>/release/bundle/macos/CytoFlow.app
    ```
    Or right-click the app and select "Open". See the [Installation Guide](installation.md#macos) for more details.
- **Windows**:
    - MSI: `target/x86_64-pc-windows-msvc/release/bundle/msi/`
    - Setup program: `target/x86_64-pc-windows-msvc/release/bundle/nsis/`
- **Linux**:
    - DEB: `target/x86_64-unknown-linux-gnu/release/bundle/deb/`
    - AppImage: `target/x86_64-unknown-linux-gnu/release/bundle/appimage/`
- **Plain binary** (all platforms): `target/<target>/release/cytoflow` (`cytoflow.exe` on Windows)

Tauri's own bundle file names may differ slightly from the release asset names; the release workflow renames them with `scripts/collect-bundles.sh`.

Files can be opened from the command line:

```bash
open target/release/bundle/macos/CytoFlow.app --args /path/to/sample.fcs /path/to/workspace.cfw.json
```

## Running the Tests

### Engine tests

```bash
cargo test -p cytoflow-core
```

This runs the unit tests and the conformance suites described in [Validation](validation.md): the FCS reader, Gating-ML 2.0 conformance, the FlowJo `biex` table, the `.wsp` import cross-checks and the workspace file round trip.

### Large test fixtures

Some `.wsp` import tests need FCS files that are too large to commit (the 8-colour ICS files are about 17 MB each). Download them once with:

```bash
scripts/fetch-testdata.sh
```

The script downloads into `testdata/large/` (ignored by git) and is safe to re-run; it skips files that are already there. Without these files, the tests that need them print a message and are skipped instead of failing.

### Lints

```bash
cargo clippy -p cytoflow-core -p cytoflow --all-targets -- -D warnings
```

### What CI runs

On every push to `main` and every pull request to `main` (`.github/workflows/ci.yml`):

- **Frontend**: `scripts/check-version.sh`, `npm ci`, `npm run typecheck`, `npm run build`
- **Engine** (Ubuntu 24.04): fetches the large fixtures with `scripts/fetch-testdata.sh` (cached), `cargo test -p cytoflow-core`, and clippy with `-D warnings`
- **Compile app** on Ubuntu 22.04, macOS and Windows: `cargo build --locked -p cytoflow` (no installers)

Cargo builds use `--locked`, so **commit `Cargo.lock` changes** together with dependency or version changes.

## Troubleshooting Build Issues

### Common Issues

1. **"no such command: `tauri`"**
    ```bash
    cargo install tauri-cli --version "^2" --locked
    ```

2. **"Target not found" / "can't find crate for `core`"**
    ```bash
    rustup target add <target-triple>
    ```

3. **"Port 1420 is already in use"**
    - Another `npm run dev` / `cargo tauri dev` is running; stop it (the port is fixed because Tauri connects to it)

4. **"Icon files missing"**
    - Ensure all icon files exist in `src-tauri/icons/`
    - Check `tauri.conf.json` has correct icon paths

5. **Linux: Missing dependencies** (`webkit2gtk-4.1` or `javascriptcoregtk-4.1` not found by pkg-config)
    - Install all system dependencies listed in Prerequisites
    - Use `apt-cache search <package>` to find package names on your distribution

6. **Windows: Linker errors**
    - Ensure Visual C++ Build Tools are installed
    - Restart terminal after installation

7. **CI fails with "the lock file needs to be updated"**
    - Run `cargo check` locally and commit `Cargo.lock`

8. **macOS: "App is damaged and cannot be opened"**
    - This is macOS Gatekeeper blocking the unsigned app
    - Remove quarantine attribute: `xattr -cr "/path/to/CytoFlow.app"`
    - Or right-click the app and select "Open"
    - See [Troubleshooting Guide](troubleshooting.md#macos-app-is-damaged-and-cant-be-opened) for detailed instructions

### Getting Help

- Check the [Troubleshooting Guide](troubleshooting.md)
- Open an issue on [GitHub](https://github.com/luksgrin/CytoFlow.rs/issues)
- Review the [Tauri documentation](https://tauri.app/)

## Development Tips

- Use `cargo tauri dev` for faster iteration; `npm run dev` alone for pure UI work in the browser
- The engine has no UI dependency: most changes can be developed and tested with `cargo test -p cytoflow-core` alone
- The IPC commands and JSON shapes are documented in [docs/ipc.md](ipc.md); the browser mock (`src/mock.ts`) implements the same contract, so keep both in sync
- Run a single test with output: `cargo test -p cytoflow-core --test wsp_import -- --nocapture`
- Clean build: `cargo clean` then rebuild
