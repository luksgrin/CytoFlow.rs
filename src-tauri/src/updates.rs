//! In-app updates (`update_check`, `update_install`; see `docs/ipc.md`).
//!
//! Thin wrapper over `tauri-plugin-updater`. The version policy lives here, not
//! in the UI, and the webview is not granted the plugin's own `updater:*`
//! commands, so it cannot install anything this module refuses:
//!
//! * only a release with the **same major version** as the running app is
//!   offered (a new major is reported with `reason: "major"` and a link);
//! * pre-releases are never offered unless `includePrerelease` is set (the
//!   release workflow never puts them into `latest.json` anyway);
//! * a Debian-package install is not self-updated (`reason: "deb"`): the
//!   manifest's Linux entry is the AppImage, and the package belongs to dpkg/apt.

use std::sync::atomic::{AtomicBool, Ordering};

use semver::Version;
use serde::Serialize;
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Where users download a version the app will not install itself.
pub const RELEASES_URL: &str = "https://github.com/luksgrin/CytoFlow.rs/releases";

static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// An update can be installed in place (same major, not skipped, supported bundle).
    pub available: bool,
    /// The newest published version, when it is newer than the running one.
    pub version: Option<String>,
    pub current_version: String,
    pub notes: Option<String>,
    /// Publication date, RFC 3339.
    pub date: Option<String>,
    /// The newest version equals the `skipVersion` argument.
    pub skipped: bool,
    /// Why a newer version is not `available`: `"major"`, `"prerelease"` or `"deb"`.
    pub reason: Option<String>,
    /// Releases page, for updates the app does not install itself.
    pub url: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    /// From `Content-Length`; `null` when the server does not send it.
    pub total: Option<u64>,
    /// `true` once the download is complete and verified installation starts.
    pub finished: bool,
}

/// Why `candidate` must not be installed over `current`, if anything.
pub fn refusal(current: &Version, candidate: &Version, include_prerelease: bool, deb: bool) -> Option<&'static str> {
    if candidate.major != current.major {
        Some("major")
    } else if !candidate.pre.is_empty() && !include_prerelease {
        Some("prerelease")
    } else if deb {
        Some("deb")
    } else {
        None
    }
}

fn is_deb() -> bool {
    matches!(tauri::utils::platform::bundle_type(), Some(BundleType::Deb | BundleType::Rpm))
}

fn describe(update: &Update, include_prerelease: bool, skip_version: Option<&str>) -> Result<UpdateInfo, String> {
    let current = Version::parse(&update.current_version).map_err(|e| format!("bad current version: {e}"))?;
    let candidate = Version::parse(&update.version).map_err(|e| format!("bad update version {}: {e}", update.version))?;
    let reason = refusal(&current, &candidate, include_prerelease, is_deb());
    let skipped = skip_version.is_some_and(|s| s == update.version);
    Ok(UpdateInfo {
        available: reason.is_none() && !skipped,
        version: Some(update.version.clone()),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
        date: update.date.and_then(|d| d.format(&time::format_description::well_known::Rfc3339).ok()),
        skipped,
        reason: reason.map(str::to_owned),
        url: reason.map(|_| format!("{RELEASES_URL}/tag/v{}", update.version)),
    })
}

/// Asks the update endpoint (`latest.json` of the latest GitHub release) for a newer version.
#[tauri::command]
pub async fn update_check<R: Runtime>(
    app: AppHandle<R>,
    include_prerelease: Option<bool>,
    skip_version: Option<String>,
) -> Result<UpdateInfo, String> {
    let include_prerelease = include_prerelease.unwrap_or(false);
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await.map_err(|e| format!("could not check for updates: {e}"))? {
        Some(update) => describe(&update, include_prerelease, skip_version.as_deref()),
        None => Ok(UpdateInfo {
            available: false,
            version: None,
            current_version: app.package_info().version.to_string(),
            notes: None,
            date: None,
            skipped: false,
            reason: None,
            url: None,
        }),
    }
}

/// Downloads, verifies and installs `version`, then relaunches the app.
/// Emits `update-progress` events while downloading. Re-checks the policy, so
/// only what `update_check` reports as `available` can be installed.
#[tauri::command]
pub async fn update_install<R: Runtime>(app: AppHandle<R>, version: String, include_prerelease: Option<bool>) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("updates are not installed into a development build".into());
    }
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("an update is already being installed".into());
    }
    let result = install(&app, &version, include_prerelease.unwrap_or(false)).await;
    INSTALLING.store(false, Ordering::SeqCst);
    result?;
    // Windows: the installer has already closed the app and restarts it itself.
    app.restart()
}

async fn install<R: Runtime>(app: &AppHandle<R>, version: &str, include_prerelease: bool) -> Result<(), String> {
    let deb = is_deb();
    let updater = app
        .updater_builder()
        .version_comparator(move |current, release| {
            release.version > current && refusal(&current, &release.version, include_prerelease, deb).is_none()
        })
        .build()
        .map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("could not check for updates: {e}"))?
        .ok_or_else(|| "no installable update is available".to_string())?;
    if update.version != version {
        return Err(format!("the available update is {}, not {version}; check again", update.version));
    }
    let mut downloaded: u64 = 0;
    let emitter = app.clone();
    let finisher = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = emitter.emit("update-progress", UpdateProgress { downloaded, total, finished: false });
            },
            move || {
                let _ = finisher.emit("update-progress", UpdateProgress { downloaded: 0, total: None, finished: true });
            },
        )
        .await
        .map_err(|e| format!("update failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn same_major_only() {
        assert_eq!(refusal(&v("1.2.3"), &v("1.3.0"), false, false), None);
        assert_eq!(refusal(&v("1.2.3"), &v("2.0.0"), false, false), Some("major"));
        assert_eq!(refusal(&v("0.1.0"), &v("0.2.0"), false, false), None);
        assert_eq!(refusal(&v("0.9.0"), &v("1.0.0"), false, false), Some("major"));
    }

    #[test]
    fn prerelease_and_deb() {
        assert_eq!(refusal(&v("1.0.0"), &v("1.1.0-1"), false, false), Some("prerelease"));
        assert_eq!(refusal(&v("1.0.0"), &v("1.1.0-1"), true, false), None);
        assert_eq!(refusal(&v("1.0.0"), &v("1.1.0"), false, true), Some("deb"));
        // A new major is reported as such even for deb installs.
        assert_eq!(refusal(&v("1.0.0"), &v("2.0.0"), false, true), Some("major"));
    }
}
