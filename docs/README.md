# Documentation

This directory contains the documentation for CytoFlow.rs, published at <https://luksgrin.xyz/CytoFlow.rs/>.

## Structure

- `index.md` - Main documentation index
- `installation.md` - Installation guide (including macOS Gatekeeper and Windows SmartScreen)
- `usage/` - User guide
    - `index.md` - Overview of the window and a first analysis
    - `workspace.md` - Samples, groups, population tree, tailored gates, keywords, saving
    - `graph.md` - Channels, axis scales, compensation, plot types, zoom/pan, overlays, copying plots, themes
    - `gating.md` - Gate tools, editing, Boolean gates, naming
    - `statistics-export.md` - Statistics tables and CSV / XLSX / Parquet export
    - `layout.md` - Batch view and layout editor (PNG / SVG export)
    - `flowjo-import.md` - FlowJo `.wsp` import and its accuracy
    - `shortcuts.md` - Keyboard shortcuts
- `migration.md` - Coming from Rodrigolab's Flow Cytometry Analyzer
- `formats.md` - FCS support, workspace files, Gating-ML
- `validation.md` - How the engine is tested, agreement numbers, known limitations
- `ipc.md` - The frontend ⇄ backend IPC contract (maintained with the code)
- `building.md` - Building from source and running the tests
- `releasing.md` - The manual release workflow (mirrors `RELEASING.md`)
- `troubleshooting.md` - Troubleshooting guide
- `DEPLOYMENT.md` - How the documentation is deployed
- `img/` - Screenshots
- `requirements.txt` - Python packages needed to build the site

This `README.md` is excluded from the site (`exclude_docs` in `mkdocs.yml`).

## Building Documentation

### MkDocs

The site is configured by `mkdocs.yml` at the repository root.

1. Install MkDocs and the plugins:
    ```bash
    pip install -r docs/requirements.txt
    ```

2. Build and serve:
    ```bash
    mkdocs serve
    ```

3. Check for warnings (broken links, pages missing from the navigation):
    ```bash
    mkdocs build --strict
    ```

### GitHub Pages

`.github/workflows/docs.yml` builds and deploys the site on every push to `main` that touches the documentation. See `DEPLOYMENT.md`.

### Read the Docs

`.readthedocs.yml` at the repository root configures a Read the Docs build (Ubuntu 22.04, Python 3.11, PDF and ePub formats).

## Contributing

To update documentation:

1. Edit the relevant Markdown file (and `mkdocs.yml` if you add a page)
2. Test locally with `mkdocs build --strict`
3. Submit a pull request
