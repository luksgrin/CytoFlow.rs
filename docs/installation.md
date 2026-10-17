# Installation Guide

This guide explains how to install CytoFlow.rs on your system.

## Pre-built Binaries (Recommended)

The easiest way to get started is to download pre-built binaries from the [GitHub Releases](https://github.com/luksgrin/CytoFlow.rs/releases) page. Every release provides the following files (`<version>` is the release number, for example `0.2.0`):

| Platform | File |
|---|---|
| macOS, Apple Silicon (M1 and later) | `CytoFlow_<version>_aarch64.dmg` |
| macOS, Intel | `CytoFlow_<version>_x64.dmg` |
| Windows, MSI installer | `CytoFlow_<version>_x64_en-US.msi` |
| Windows, setup program | `CytoFlow_<version>_x64-setup.exe` |
| Linux, Debian / Ubuntu package | `CytoFlow_<version>_amd64.deb` |
| Linux, portable | `CytoFlow_<version>_amd64.AppImage` |
| Source code | `source.tar.gz` |

!!! note "Unsigned builds"
    The binaries are not code-signed or notarised. macOS and Windows will warn you the first time you open them; the sections below explain how to proceed. If you prefer, you can [build the application yourself](building.md).

### macOS

1. Download the `.dmg` file for your architecture:
    - **Apple Silicon (M1/M2/M3/M4)**: `CytoFlow_<version>_aarch64.dmg`
    - **Intel**: `CytoFlow_<version>_x64.dmg`

    Not sure which one you need? Open  > **About This Mac**: a "Chip" line (Apple M…) means Apple Silicon, a "Processor" line (Intel…) means Intel.

2. Open the downloaded DMG file

3. Drag `CytoFlow.app` to your Applications folder

4. **Important - macOS Gatekeeper:** Since the app is not code-signed, macOS may show an error that "CytoFlow" is "damaged and can't be opened", or that it "cannot be opened because the developer cannot be verified". To fix this, use one of the following methods:

    **Method 1: Remove quarantine attribute (Recommended)**
    ```bash
    xattr -cr /Applications/CytoFlow.app
    ```
    Then open the app normally from Applications.

    **Method 2: Right-click to open**

    - Right-click (or Control-click) `CytoFlow` in Applications
    - Select "Open" from the context menu
    - Click "Open" in the security dialog
    - This only needs to be done once

    **Method 3: System Settings**

    - Try to open the app once (it will be blocked)
    - Go to System Settings > Privacy & Security (System Preferences > Security & Privacy on macOS 12 and earlier)
    - Scroll down to find the message about "CytoFlow" being blocked
    - Click "Open Anyway"
    - Confirm by clicking "Open" (you may be asked for your password)

    After using any of these methods, you can open the app normally in the future.

!!! tip "Opening files from the Terminal"
    You can pass FCS files, CytoFlow workspaces (`*.cfw.json`) or FlowJo workspaces (`*.wsp`) on the command line:
    ```bash
    open /Applications/CytoFlow.app --args ~/data/sample1.fcs ~/data/sample2.fcs
    ```

### Windows

Two installers are provided; both install the same application. Use the **MSI** (`CytoFlow_<version>_x64_en-US.msi`) if you manage software centrally or prefer Windows Installer, or the **setup program** (`CytoFlow_<version>_x64-setup.exe`) for a per-user install that does not need administrator rights.

1. Download one of the two installers from the releases page

2. Double-click it to run the installer

3. **Windows SmartScreen Warning:** Windows may show a blue "Windows protected your PC" screen. This is normal for unsigned applications. To proceed:
    - Click **"More info"** on the warning screen
    - Click **"Run anyway"** to continue installation
    - Alternatively, right-click the downloaded file, select **"Properties"**, check **"Unblock"** if available, click **"OK"**, then run the installer

4. Follow the installation wizard

5. Launch **CytoFlow** from the Start menu

CytoFlow.rs uses the Microsoft Edge WebView2 runtime, which is part of Windows 10 (recent updates) and Windows 11. If it is missing, the installer downloads it automatically (this needs an internet connection the first time).

### Linux (Ubuntu/Debian)

#### Option 1: DEB Package (Recommended)

1. Download `CytoFlow_<version>_amd64.deb` from the releases page

2. Install it with apt, which also pulls in the required libraries (WebKitGTK, GTK 3):
    ```bash
    sudo apt install ./CytoFlow_*_amd64.deb
    ```

    Or with dpkg:
    ```bash
    sudo dpkg -i CytoFlow_*_amd64.deb
    ```

3. If dpkg reports dependency issues, fix them with:
    ```bash
    sudo apt-get install -f
    ```

4. Launch **CytoFlow** from your application menu

#### Option 2: AppImage

The AppImage runs on most x86_64 distributions without installation.

1. Download `CytoFlow_<version>_amd64.AppImage` from the releases page

2. Make it executable:
    ```bash
    chmod +x CytoFlow_*_amd64.AppImage
    ```

3. Run it:
    ```bash
    ./CytoFlow_*_amd64.AppImage
    ```

    If it fails with a FUSE error on Ubuntu 22.04 or later, install `libfuse2` (`sudo apt install libfuse2`; on Ubuntu 24.04 the package is `libfuse2t64`).

## Building from Source

If you prefer to build from source or need a custom build, see the [Building from Source](building.md) guide.

### Quick Build

```bash
# Clone the repository
git clone https://github.com/luksgrin/CytoFlow.rs.git
cd CytoFlow.rs

# Install the frontend dependencies and the Tauri CLI
npm install
cargo install tauri-cli --version "^2" --locked

# Build the application (the frontend is built automatically)
cargo tauri build
```

The bundles are written to `target/release/bundle/`. For detailed platform-specific instructions, see [Building from Source](building.md).

## Verification

After installation, verify the application works:

1. Launch the application
2. You should see the main window: the toolbar (**Add FCS…**, **New**, **Open…**, **Save…**, **Import FlowJo .wsp…**) at the top, the Groups / Samples / Populations column on the left, the graph in the centre and the Statistics panel on the right. The plot area reads "Add FCS files to begin (⌘O)"
3. Click **Add FCS…** and load an FCS file: it appears under **Samples** with its event count, and the first plot is drawn when you select it

The status bar at the bottom says **Ready** when the application has connected to its engine. If it says "Running in the browser with synthetic mock data", you are looking at the development preview in a web browser, not the installed application.

## Updating

### In-app updates

CytoFlow can update itself on macOS, Windows and with the Linux AppImage.

- **Automatic check**: about 10 seconds after start-up CytoFlow asks GitHub whether a newer release exists, at most once every 24 hours. Nothing is downloaded without asking.
- **Manual check**: click **⟳ Updates** at the right end of the toolbar.
- When an update is available a dialog shows the new version and its release notes, with **Install and restart**, **Later** and **Skip this version**. *Install and restart* downloads the update (with a progress bar), checks its signature, installs it and restarts CytoFlow. *Skip this version* hides that version from the automatic check (the **⟳ Updates** button still shows it); you are told again when a newer one appears.
- The same dialog has a **Check for updates automatically** checkbox. Untick it to turn the automatic check off; the setting is remembered.
- If you are up to date, the status bar says "CytoFlow X.Y.Z is up to date".

What is offered:

- Only **full releases**. Pre-releases are never offered in the app.
- Only versions with the **same major version** as the one you run (for example 1.2 → 1.5, but not 1.x → 2.0). A new major version may change how workspaces or results behave, so the dialog tells you it exists and links to the releases page; install it by hand, as described above, when you are ready.
- **Debian package (`.deb`)**: the app does not replace files that belong to the package manager. The dialog links to the release; download the new `.deb` and install it with `sudo apt install ./CytoFlow_<new version>_amd64.deb`.

Updates are signed: CytoFlow refuses to install a download whose signature does not match the key built into the app. The installers themselves are still not code-signed by Apple or Microsoft, but an update installed from inside the app usually does not need the Gatekeeper or SmartScreen steps again.

### Updating by hand

1. Download the latest release from GitHub
2. Follow the installation steps above
3. On macOS, replace `CytoFlow.app` in Applications (you may need to repeat the Gatekeeper step once for the new version)
4. On Windows, run the new installer; it upgrades the existing installation
5. On Linux, `sudo apt install ./CytoFlow_<new version>_amd64.deb` upgrades the package in place; for the AppImage, replace the file

Workspace files (`*.cfw.json`) written by older versions can be opened by newer ones.

## Uninstallation

### macOS

1. Open the Applications folder
2. Drag `CytoFlow.app` to the Trash
3. Empty the Trash
4. Optionally remove the application's data (theme preference, cached web data):
    ```bash
    rm -rf ~/Library/WebKit/rs.cytoflow.app ~/Library/Caches/rs.cytoflow.app
    ```

### Windows

1. Open Settings > Apps > Installed apps
2. Find "CytoFlow"
3. Click Uninstall

### Linux (DEB)

Find the package name and remove it:

```bash
dpkg -l | grep -i cyto
sudo apt remove <package name shown above>
```

For the AppImage, simply delete the file.

Your workspace files (`*.cfw.json`) and FCS files are never touched by uninstalling.
