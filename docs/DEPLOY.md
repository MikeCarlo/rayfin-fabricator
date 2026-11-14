# Fabricator deployment

`deploy.ps1` provisions the Azure telemetry infrastructure for the app and wires the Application Insights GitHub Actions secret. Binary distribution is handled by GitHub Releases via CI.

## Prerequisites

- PowerShell 7 (`pwsh`)
- Azure CLI (`az`) signed in to the Azure subscription that should host the telemetry resources (pass `-SubscriptionId` to override; otherwise the current `az` subscription is used)
- GitHub CLI (`gh`) authenticated to `spatney/rayfin-fabricator` for automatic `APPINSIGHTS_CONNECTION_STRING` secret wiring
- Node.js/npm for local builds
- Rust toolchain (stable) to build the `fabricator` executable locally. Running a downloaded executable does not need the Visual Studio C++ compiler or WebView2.

## What it provisions

- Resource group `rayfin-fabricator`
- Log Analytics workspace `rayfin-fabricator-logs`
- Workspace-based Application Insights `rayfin-fabricator-insights`
- Resource-group monthly Azure budget alerts at 80% and 100%

## Cost posture

The intended steady-state cost is near $0 for light usage: Application Insights has a 5 GB/month free grant and the script sets a strict 0.1 GB/day ingestion cap; the default $5 monthly budget sends alert emails.

## Run

From the repo root:

```powershell
pwsh -NoProfile -File .\deploy.ps1
```

To provision and wire endpoints and build a local Windows installer:

```powershell
pwsh -NoProfile -File .\deploy.ps1 -BuildLocal
```

The script writes `resources/telemetry.json` and `.deploy.state.json`. If `gh` is unavailable or not authenticated, it prints the Actions secret to add manually.

## Cut a release

Push a version tag to trigger CI, which builds `fabricator.exe` and publishes it on the GitHub Release:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

## npm during project creation

New projects are created with `npm create @microsoft/rayfin@latest -- <folder> -t universal-app`, the Universal App template built into the Rayfin CLI (its data service starts off, so no database is created until the app needs one). Fabricator doesn't bundle a template or an npm cache: npm uses its normal cache and the registry.

- **Fresh scaffold metadata.** `npm create @microsoft/rayfin@latest` explicitly disables `prefer-offline` and enables `prefer-online` for itself and its children. npm gives `prefer-offline` precedence: enabling both can resolve the latest scaffolder against an older cached CLI manifest and fail with `ETARGET` even though that CLI version is published (issue #28). The fix keeps npm's cache and the selected package versions; it does not clear caches or silently substitute an older CLI.
- **Regression coverage.** `cargo test --manifest-path src-tauri\Cargo.toml --lib npm` (PowerShell, from the repo root) checks the npm environment policy and reproduces stale-metadata `ETARGET` against a loopback registry. Requires Node.js/npm; no packages are installed, and the user's npm configuration and cache are not used.

## Auto-update

**Settings → Check for updates** asks GitHub for the latest release of this repository. When it is newer, Download saves `fabricator.exe` (or `fabricator-macos` / `fabricator-linux`) and Install replaces the running executable after it exits, then starts the new one. The release assets are the files `.github/workflows/ci.yml` attaches to tags named `v*`.

There is no minisign updater manifest. The executable is whatever that GitHub Release published.
