# Deploying Documentation to GitHub Pages

The documentation is built with [MkDocs](https://www.mkdocs.org/) and the [Material](https://squidfunk.github.io/mkdocs-material/) theme, and is automatically deployed to GitHub Pages when changes are pushed to the `main` branch.

## Automatic Deployment

The workflow (`.github/workflows/docs.yml`, "Deploy Documentation") runs on every push to `main` that touches:

- `docs/**`
- `mkdocs.yml`
- `.readthedocs.yml`
- `.github/workflows/docs.yml`

It:

1. Sets up Python 3.11 and installs `docs/requirements.txt`
2. Builds the site with `mkdocs build` (output in `./site`)
3. Deploys it to GitHub Pages
4. Makes it available at: <https://luksgrin.xyz/CytoFlow.rs/>

## Manual Deployment

To manually trigger a deployment:

1. Go to the Actions tab in GitHub
2. Select the "Deploy Documentation" workflow
3. Click "Run workflow"
4. Select the `main` branch and click "Run workflow"

## Enabling GitHub Pages

GitHub Pages is already enabled for this repository. If it ever needs to be set up again:

1. Go to repository Settings
2. Navigate to Pages
3. Under "Source", select "GitHub Actions"
4. The workflow will handle the rest

## Local Preview

To preview documentation locally before deploying:

```bash
# Create a virtual environment (optional, recommended)
python3 -m venv .venv
source .venv/bin/activate

# Install dependencies
pip install -r docs/requirements.txt

# Serve locally with live reload
mkdocs serve

# Build the static site, failing on any warning (broken links, missing pages)
mkdocs build --strict
```

The documentation will be available at `http://127.0.0.1:8000/CytoFlow.rs/`.

Please run `mkdocs build --strict` before opening a pull request that changes the documentation: it must finish without warnings.

## Writing Documentation

- Pages live in `docs/`; the navigation is defined by `nav:` in `mkdocs.yml`. A new page must be added there
- Screenshots go in `docs/img/` (PNG, under 400 KB each). The current ones were captured from the browser preview (`npm run dev`, synthetic data)
- Link between pages with relative `.md` paths (e.g. `[Gating](usage/gating.md#gate-tools)`) so that MkDocs can check them
- Keyboard keys use the `++key++` syntax (`pymdownx.keys`), notes use admonitions (`!!! note`)
- `docs/ipc.md` is the frontend ⇄ backend contract and is maintained together with the code

## Custom Domain

The site is served under the custom domain of the owner's GitHub Pages user site (`luksgrin.xyz`), so this project's pages appear at `https://luksgrin.xyz/CytoFlow.rs/` without a `CNAME` file in this repository. `site_url` in `mkdocs.yml` must stay in sync with that address.

To use a dedicated domain instead:

1. Add a `CNAME` file in the `docs/` directory with your domain
2. Update `site_url` in `mkdocs.yml`
3. Configure DNS settings for your domain

## Read the Docs

A `.readthedocs.yml` is also provided (Ubuntu 22.04, Python 3.11, `mkdocs.yml`, PDF and ePub formats) in case the documentation is also published on Read the Docs. GitHub Pages is the canonical location.
