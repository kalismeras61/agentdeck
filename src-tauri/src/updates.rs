//! Updating AgentDeck itself: asks the release feed (GitHub Releases) for a newer version,
//! downloads it, checks its signature against the public key built into the app, installs it
//! and restarts. Builds made without that key (local builds) don't update themselves.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::UpdaterExt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    /// Whether this build can update itself (it was released with an update key).
    pub enabled: bool,
    /// The newer version, if there is one.
    pub available: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
    done: bool,
}

/// Whether this build carries the key that update files are checked against.
pub fn enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.trim().is_empty())
}

fn friendly(e: impl std::fmt::Display) -> String {
    let e = e.to_string();
    if e.contains("404") || e.to_lowercase().contains("not found") {
        "No release has been published yet".into()
    } else if e.to_lowercase().contains("signature") {
        "The update's signature didn't match, so it wasn't installed".into()
    } else {
        format!("Couldn't reach the update server: {e}")
    }
}

pub async fn check<R: Runtime>(app: &AppHandle<R>) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    if !enabled(app) {
        return Ok(UpdateInfo {
            current,
            enabled: false,
            available: None,
            notes: None,
            date: None,
        });
    }
    let update = app
        .updater()
        .map_err(friendly)?
        .check()
        .await
        .map_err(friendly)?;
    Ok(UpdateInfo {
        current,
        enabled: true,
        available: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()),
        date: update
            .as_ref()
            .and_then(|u| u.date.map(|d| d.date().to_string())),
    })
}

/// Downloads and installs the newer version, reporting progress as `update-progress`
/// events, then restarts into it.
pub async fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if !enabled(app) {
        return Err(
            "This build doesn't update itself. Download the newest version instead.".into(),
        );
    }
    let update = app
        .updater()
        .map_err(friendly)?
        .check()
        .await
        .map_err(friendly)?
        .ok_or("AgentDeck is up to date")?;
    let mut downloaded: u64 = 0;
    let a = app.clone();
    let b = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = a.emit(
                    "update-progress",
                    Progress {
                        downloaded,
                        total,
                        done: false,
                    },
                );
            },
            move || {
                let _ = b.emit(
                    "update-progress",
                    Progress {
                        downloaded: 0,
                        total: None,
                        done: true,
                    },
                );
            },
        )
        .await
        .map_err(friendly)?;
    app.restart();
}
