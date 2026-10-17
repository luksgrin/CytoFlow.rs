# Troubleshooting Guide

Common issues and solutions when using or building CytoFlow.rs.

## Installation Issues

### macOS: "App is damaged and can't be opened"

This error occurs because the app is not code-signed. macOS Gatekeeper blocks unsigned apps downloaded from the internet. This is a security feature, not an actual problem with the app.

**Solution (choose one method):**

**Method 1: Remove quarantine attribute (Recommended)**
```bash
xattr -cr /Applications/CytoFlow.app
```
Then open the app normally from Applications.

**Method 2: Right-click to open**

- Right-click (or Control-click) the app in Applications
- Select "Open" from the context menu
- Click "Open" in the security dialog
- This only needs to be done once

**Method 3: System Settings**

- Go to System Settings > Privacy & Security (or System Preferences > Security & Privacy on older macOS)
- Scroll down to find the blocked app message
- Click "Open Anyway"
- Confirm by clicking "Open"

After using any of these methods, you can open the app normally in the future. You may need to repeat it after installing a new version.

### macOS: "CytoFlow can't be opened because it requires a newer version of macOS"

CytoFlow.rs needs macOS 11 (Big Sur) or later.

### macOS: wrong architecture

If the app bounces in the Dock and quits, or macOS asks to install Rosetta, check that you downloaded the right DMG: `aarch64` for Apple Silicon, `x64` for Intel (the Intel build also runs on Apple Silicon under Rosetta, but more slowly).

### Windows: SmartScreen warning "Windows protected your PC"

This warning appears because the application is not code-signed. Windows Defender SmartScreen blocks unrecognised applications as a security measure. This is normal for unsigned software.

**Solution (choose one method):**

**Method 1: Use "More info" (Recommended)**

1. When the SmartScreen warning appears, click **"More info"**
2. Click **"Run anyway"** to proceed with installation
3. This only needs to be done once per download

**Method 2: Unblock the file**

1. Right-click the downloaded `.msi` or `-setup.exe` file
2. Select **"Properties"**
3. If you see an **"Unblock"** checkbox at the bottom, check it
4. Click **"OK"**
5. Run the installer normally

**Method 3: Windows Security Settings**

1. Open Windows Security (Settings > Privacy & Security > Windows Security)
2. Go to **"App & browser control"**
3. Under **"Check apps and files"**, you can temporarily lower the protection level (not recommended for long-term)

**Note:** After the first installation, Windows will remember your choice for that file.

### Windows: "Installing this app requires administrator rights"

Use the setup program (`CytoFlow_<version>_x64-setup.exe`) instead of the MSI: it installs for the current user only and needs no administrator rights.

### Linux: Dependency errors when installing the DEB

**Solution:**
```bash
sudo apt-get install -f
```

This will install missing dependencies (`libwebkit2gtk-4.1-0`, `libgtk-3-0`). Installing with `sudo apt install ./CytoFlow_*_amd64.deb` resolves them automatically. On distributions older than Ubuntu 22.04 / Debian 11, WebKitGTK 4.1 may not be available: use a newer distribution.

### Linux: AppImage does not start

- Make it executable: `chmod +x CytoFlow_*_amd64.AppImage`
- "dlopen(): error loading libfuse.so.2": install `libfuse2` (`libfuse2t64` on Ubuntu 24.04)
- Blank window on some GPU drivers: try `WEBKIT_DISABLE_DMABUF_RENDERER=1 ./CytoFlow_*_amd64.AppImage`

## Runtime Issues

### Application won't start

**Check:**

1. System requirements are met
2. No other instance is running
3. Check system logs for errors

**macOS:**
```bash
# Check Console.app for errors
# Or from terminal:
log show --predicate 'process == "cytoflow"' --last 5m

# Or run the binary directly to see its output:
/Applications/CytoFlow.app/Contents/MacOS/cytoflow
```

**Linux:**
```bash
# Run from terminal to see errors
./CytoFlow_*_amd64.AppImage
# or, with the DEB installed, run the binary from a terminal (its name is usually `cytoflow`)
```

**Windows:** check Event Viewer > Windows Logs > Application. If the window stays blank, (re)install the [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).

### The status bar says "Running in the browser with synthetic mock data"

You are looking at the browser preview of the user interface (`npm run dev`), not the desktop application. Its data is synthetic. Start the installed application, or `cargo tauri dev` when developing.

### "Update check failed" / updates are never offered

**⟳ Updates** asks GitHub for `latest.json` of the latest release; the automatic check does the same silently once a day. It can fail when:

- **You are offline**, or a proxy or firewall blocks `github.com` and `objects.githubusercontent.com` (where release files are served from). Try again later or from another network, or update by hand from the [releases page](https://github.com/luksgrin/CytoFlow.rs/releases).
- **GitHub rate-limits or is unavailable** (an HTTP 403 / 429 / 5xx in the message). Wait a while and try again; the automatic check retries on a later start-up.
- **The latest release has no `latest.json`** (releases made before in-app updates existed). Nothing is wrong with your installation; update by hand once.
- **"signature" errors while installing** mean the download did not match the key built into the app (a corrupted download or an altered file). Nothing is installed; try again, or update by hand.

Some updates are deliberately not installed from inside the app: a **new major version**, and any update when CytoFlow was installed from the **`.deb`** package. The dialog links to the releases page instead (see [Updating](installation.md#updating)).

**Turning the automatic check off**: click **⟳ Updates** and untick **Check for updates automatically** in the dialog. The setting is stored per user; the button still works.

### FCS file won't load

The status bar shows "Open FCS: 1 problem(s) — …" with the reason.

**Possible causes:**

1. File is corrupted or truncated ("file shorter than the 58-byte header", "bad magic", "TEXT segment offsets out of range", "missing DATA segment offsets")
2. Unsupported FCS variant: ASCII data ("Unsupported FCS feature: ASCII ($DATATYPE/A) data"), non-list mode ("only list mode is supported"), integer widths that are not a multiple of 8 bits, or an unusual `$BYTEORD`
3. File is locked by another application, or you do not have permission to read it

**Solutions:**

- Try a different FCS file from the same instrument
- Check file permissions
- Re-export the file from the acquisition software as FCS 3.0 or 3.1 with float or integer data
- See [File formats](formats.md#fcs-files) for exactly what is supported

### A sample shows "file not found"

The workspace stores the path of each FCS file. If the files were moved, renamed or the workspace was copied to another computer:

- Put the workspace file in the same folder as its FCS files: CytoFlow.rs also looks for each file name next to the workspace
- Or re-add the files with **Add FCS…** and copy the gates (gates of "All Samples" are applied to new samples automatically)

### Plot is blank or shows no data

**Check:**

1. A sample is selected and loaded (not "file not found")
2. Both channels are selected (the plot says "Choose channels" otherwise)
3. The population is not empty (count 0 in the Statistics panel)
4. The view is not zoomed or panned away from the data: press **0** to reset it
5. The scale: on a **log** axis, values at or below zero are piled up on the axis; try **logicle**
6. The scale parameters: a **T** (top of scale) far below the data range pushes all events against the top edge; click ⚙ and **Defaults**

### Events are piled up on the edges of the plot

Events outside the display range are drawn on its edge. This usually means the scale does not fit the data: use **logicle** or **biex** for compensated fluorescence, increase **A** / **Neg** (extra negative decades) when many events pile up at the bottom, or click **Defaults** in the ⚙ popover to re-estimate the parameters.

### Gate tool does nothing

**Check:**

1. You are using the left mouse button (right-click cancels a drawing or opens the context menu; right-drag pans)
2. The tool fits the plot: rectangle, polygon, lasso, ellipse and quadrant need a 2-D plot; range needs a histogram (the buttons are greyed out otherwise)
3. You are drawing inside the plot area
4. A polygon needs at least 3 vertices; a rectangle or ellipse must be more than a few pixels wide and tall
5. A dialog is not waiting behind another window (the gate name prompt)

### My gate changed in other samples / did not change in other samples

Check the **gating scope** pill in the toolbar before editing: with a group selected, edits apply to every member that has not tailored the gate; with a sample selected, only that sample changes (and the gate becomes tailored, **T**). Use right-click › **Apply to group…** to push a sample's version back to the group. See [Workspace and samples](usage/workspace.md#tailored-gates).

### Compensated axes are greyed out

The **Comp** checkbox is disabled when compensation is off. Open **Compensation…** and choose **Use FCS matrix** (if the file has a `$SPILLOVER` / `SPILL` keyword) or **Custom matrix**.

### Export fails

**Check:**

1. You have write permissions to the output directory
2. Disk space is available
3. File path is valid
4. A population is selected on a loaded sample ("Select a loaded sample first")
5. For Excel: the population has at most 1,048,576 events (Excel's row limit); use CSV or Parquet for larger populations
6. The target file is not open in Excel (Windows locks open files)

**Solutions:**

- Try a different output location
- Check disk space: `df -h` (Linux/macOS) or check in File Explorer (Windows)
- Ensure output directory exists

### Copy does not put the plot on the clipboard

If the clipboard is not available, **Copy** offers to save the PNG to a file instead. Paste into an application that accepts images (Word, PowerPoint, Keynote, Preview's "New from Clipboard").

## FlowJo Import Issues

### "Missing FCS files"

The `.wsp` file records the absolute path of each FCS file on the computer where it was written. CytoFlow.rs also looks for each file name in the folder of the `.wsp` file and one level below. If they are still not found, the **Missing FCS files** dialog lists them:

- Click **Locate folder…** and choose the folder that contains the FCS files (its sub-folders are searched too). The import is repeated with that folder
- Click **Skip** to import anyway: the samples are kept with their gate trees, marked "file not found"

Tip: keep the `.wsp` file in the same folder as the FCS files.

### "Imported with N warning(s)"

The status bar lists the warnings. Common ones:

- **"unsupported gate type … skipped"** / **"population …: gate could not be imported"**: the gate type (e.g. a CurlyQuad gate) is not supported; the population and its sub-populations are missing
- **"N distinct compensation matrices found; using the first"**: only one matrix is applied to every sample
- **"boolean node …: unknown dependent …"**: a Boolean gate refers to a population that could not be imported

See [Importing FlowJo workspaces](usage/flowjo-import.md#what-is-not-imported).

### Counts differ from FlowJo

- **Ellipse gates on logicle axes** count 2–3 % fewer events than FlowJo; this is a known limitation (see [Validation](validation.md#known-limitations))
- **Polygons on logicle axes** may differ by a few events per thousand at the boundary
- Check that compensation is on (**Comp: custom** under the right column's **Compensation…** button) and that the axes use the imported transforms
- Make sure you compare the same sample: each sample has its own gate tree and transforms after import

If counts differ by more than this on rectangles, quadrants or `biex` axes, please open an issue with the workspace (and, if possible, the FCS files).

## Build Issues

### "no such command: `tauri`"

**Solution:**
```bash
cargo install tauri-cli --version "^2" --locked
```

### "Target not installed"

**Solution:**
```bash
rustup target add <target-triple>
```

Common targets:

- `aarch64-apple-darwin` (macOS ARM64)
- `x86_64-apple-darwin` (macOS Intel)
- `x86_64-pc-windows-msvc` (Windows)
- `x86_64-unknown-linux-gnu` (Linux)

### Build fails with icon errors

**Solution:**

1. Check that all icon files exist in `src-tauri/icons/`:
    - `32x32.png`, `128x128.png`, `128x128@2x.png`
    - `icon.icns` (macOS)
    - `icon.ico` (Windows)
2. Verify `tauri.conf.json` has correct icon paths

### Linux: Missing system dependencies

**Solution:**
Install all required packages:
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

### Windows: Linker errors

**Solution:**

1. Install Visual C++ Build Tools
2. Restart terminal/IDE after installation
3. Ensure Windows SDK is installed

### macOS: Code signing warnings

**Note:** These are normal for local builds and don't prevent the app from running. The release builds are unsigned too.

### "A public key has been found, but no private key"

`tauri.conf.json` enables updater artifacts (`bundle.createUpdaterArtifacts`), so `cargo tauri build` signs them and needs the updater signing key. Without it, build without the updater artifacts:

```bash
cargo tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

The maintainer's release builds get the key from the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` environment variables (see [Releasing](releasing.md#updater-signing-and-latestjson)).

### "the lock file needs to be updated but --locked was passed"

Run `cargo check` locally and commit the updated `Cargo.lock`.

### Tests are skipped ("skipping …: … not found in testdata/large")

Some import tests need large FCS fixtures that are not committed. Download them with `scripts/fetch-testdata.sh`. See [Building from Source](building.md#large-test-fixtures).

### Build is very slow

**Optimizations:**

1. Use `cargo test -p cytoflow-core` or `cargo build -p cytoflow-core` for engine-only changes
2. Use `npm run dev` (browser preview) for UI-only changes
3. Keep the `target/` directory between builds; the first release build compiles all dependencies

### Out of memory during build

**Solution:**

1. Close other applications
2. Increase swap space (Linux)
3. Build on a machine with more RAM
4. Use the GitHub Actions workflows for builds

## Performance Issues

### Application is slow

**Check:**

1. File size and number of samples (each loaded sample is kept in memory)
2. Number of overlay series and the batch view (each plot is computed separately)
3. System resources

**Solutions:**

- Close other applications
- Remove samples you do not need from the workspace
- Clear overlays; switch to the **Graph** view instead of **Batch** while editing gates

### Plot rendering is slow

**Solutions:**

- Use **Density** instead of **Pseudocolor** (pseudocolour draws up to 60,000 dots)
- Reduce the number of overlay series
- Close and reopen the application

## Getting Help

If you're still experiencing issues:

1. **Check the logs:**
    - macOS: Console.app, or `log show --predicate 'process == "cytoflow"' --last 5m`
    - Linux: Run from terminal
    - Windows: Event Viewer

2. **Search existing issues:**
    - [GitHub Issues](https://github.com/luksgrin/CytoFlow.rs/issues)

3. **Create a new issue:**
    - Include your OS and version
    - Describe the problem clearly
    - Include error messages from the status bar
    - Attach relevant files (if possible)

4. **Check documentation:**
    - [Tauri documentation](https://tauri.app/)
    - [FCS file format specification](https://isac-net.org/Standards-and-Publications/ISAC-Data-Standards-Project/FCS-Data-Standards/)

## Reporting Bugs

When reporting a bug, please include:

1. **System information:**
    - OS and version
    - Application version
    - Architecture (ARM64, x86_64, etc.)

2. **Steps to reproduce:**
    - Clear, numbered steps
    - Sample data (if possible)

3. **Expected vs. actual behavior:**
    - What should happen
    - What actually happens

4. **Error messages:**
    - Full error text from the status bar
    - Screenshots if helpful

5. **Additional context:**
    - FCS file format/version and instrument (`$CYT`, see **Keywords…**)
    - File size and number of events
    - For FlowJo imports: the FlowJo version and, if possible, the `.wsp` file
