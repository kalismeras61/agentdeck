//! Files the user drops or pastes into the message box.
//!
//! Every attachment reaches the agent as a path it can read with its own tools (and its own
//! permission rules). Images also go in as images where the agent takes them: Claude Code gets
//! them inline, Codex by local path. Nothing is copied into the project; pasted images are kept
//! in AgentDeck's own data folder.

use base64::Engine as _;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const MAX_FILES: usize = 20;
/// Claude Code takes images up to 5 MB encoded, about 3.75 MB of file.
const MAX_IMAGE_BYTES: u64 = 3_750_000;
const MAX_PASTE_BYTES: usize = 10_000_000;

#[derive(Debug, Clone, Default)]
pub struct Attach {
    /// What the bubble shows: paths relative to the project when inside it.
    pub shown: Vec<String>,
    /// Absolute paths of every attachment.
    pub paths: Vec<PathBuf>,
    /// The ones that are images small enough to send as images, with their media type.
    pub images: Vec<(PathBuf, &'static str)>,
}

fn media_type(p: &Path) -> Option<&'static str> {
    match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Checks the dropped paths: absolute, existing files or folders, at most 20.
pub fn resolve(paths: &[String], project: &str) -> Result<Attach, String> {
    if paths.len() > MAX_FILES {
        return Err(format!("Attach at most {MAX_FILES} files at a time"));
    }
    let root = std::fs::canonicalize(project).unwrap_or_else(|_| PathBuf::from(project));
    let mut a = Attach::default();
    for raw in paths {
        let p = PathBuf::from(raw);
        if !p.is_absolute() {
            return Err(format!("{raw} isn't a full path"));
        }
        let meta = std::fs::metadata(&p).map_err(|_| format!("{raw} isn't there anymore"))?;
        let canon = std::fs::canonicalize(&p).unwrap_or(p.clone());
        let shown = canon
            .strip_prefix(&root)
            .ok()
            .filter(|r| !r.as_os_str().is_empty())
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_else(|| canon.to_string_lossy().into_owned());
        if a.paths.contains(&canon) {
            continue;
        }
        if meta.is_file() && meta.len() <= MAX_IMAGE_BYTES {
            if let Some(mt) = media_type(&canon) {
                a.images.push((canon.clone(), mt));
            }
        }
        a.shown.push(if meta.is_dir() {
            format!("{shown}/")
        } else {
            shown
        });
        a.paths.push(canon);
    }
    Ok(a)
}

impl Attach {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    fn is_image(&self, p: &Path) -> bool {
        self.images.iter().any(|(i, _)| i == p)
    }

    /// The lines added to the message so the agent knows where the files are. With
    /// `images_inline`, images already sent as images are left out of the list.
    pub fn note(&self, images_inline: bool) -> String {
        let listed: Vec<String> = self
            .paths
            .iter()
            .zip(&self.shown)
            .filter(|(p, _)| !(images_inline && self.is_image(p)))
            .map(|(_, s)| format!("- {s}"))
            .collect();
        if listed.is_empty() {
            return String::new();
        }
        format!(
            "\n\nAttached files (paths are relative to the project folder unless they start with /):\n{}",
            listed.join("\n")
        )
    }

    /// Claude Code content blocks for the images.
    pub fn claude_images(&self) -> Vec<Value> {
        self.images
            .iter()
            .filter_map(|(p, mt)| {
                let bytes = std::fs::read(p).ok()?;
                let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                Some(json!({ "type": "image", "source": { "type": "base64", "media_type": mt, "data": data } }))
            })
            .collect()
    }

    /// Codex input items for the images.
    pub fn codex_images(&self) -> Vec<Value> {
        self.images
            .iter()
            .map(|(p, _)| json!({ "type": "localImage", "path": p.to_string_lossy() }))
            .collect()
    }
}

/// Saves a pasted image (base64) in AgentDeck's data folder and returns its path.
pub fn stash(dir: &Path, name: &str, data_b64: &str) -> Result<String, String> {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ["png", "jpg", "jpeg", "gif", "webp"].contains(&e.as_str()))
        .ok_or("Only PNG, JPEG, GIF and WebP images can be pasted")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64.trim())
        .map_err(|_| "That image couldn't be read")?;
    if bytes.len() > MAX_PASTE_BYTES {
        return Err("That image is larger than 10 MB".into());
    }
    save(dir, &ext, &bytes)
}

fn save(dir: &Path, ext: &str, bytes: &[u8]) -> Result<String, String> {
    let dir = dir.join("attachments");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!(
        "pasted-{}.{ext}",
        &uuid::Uuid::new_v4().to_string()[..8]
    ));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// The image on the system clipboard, saved as PNG in AgentDeck's data folder. For web views
/// that don't hand pasted images to the page. `None` when the clipboard holds no image.
pub fn clipboard_image(dir: &Path) -> Result<Option<String>, String> {
    let Ok(mut cb) = arboard::Clipboard::new() else {
        return Ok(None);
    };
    let Ok(img) = cb.get_image() else {
        return Ok(None);
    };
    if img.width * img.height * 4 > 120_000_000 {
        return Err("That image is too large to attach".into());
    }
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, img.width as u32, img.height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&img.bytes).map_err(|e| e.to_string())?;
    }
    if png_bytes.len() > MAX_PASTE_BYTES {
        return Err("That image is larger than 10 MB".into());
    }
    save(dir, "png", &png_bytes).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_inside_the_project_are_shown_relative_and_images_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::write(proj.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(proj.join("shot.png"), [0x89, b'P', b'N', b'G']).unwrap();
        let outside = dir.path().join("notes.txt");
        std::fs::write(&outside, "hi").unwrap();
        let p = |x: &Path| x.to_string_lossy().into_owned();
        let a = resolve(
            &[
                p(&proj.join("src/main.rs")),
                p(&proj.join("shot.png")),
                p(&outside),
                p(&proj.join("src")),
                p(&proj.join("src/main.rs")),
            ],
            &p(&proj),
        )
        .unwrap();
        assert_eq!(a.shown[0], "src/main.rs");
        assert_eq!(a.shown[1], "shot.png");
        assert!(a.shown[2].ends_with("/notes.txt") && a.shown[2].starts_with('/'));
        assert_eq!(a.shown[3], "src/");
        assert_eq!(a.paths.len(), 4, "duplicates dropped");
        assert_eq!(a.images.len(), 1);
        let inline = a.note(true);
        assert!(inline.contains("- src/main.rs") && !inline.contains("shot.png"));
        assert!(a.note(false).contains("- shot.png"));
        assert_eq!(a.claude_images()[0]["source"]["media_type"], "image/png");
        assert_eq!(a.codex_images()[0]["type"], "localImage");

        assert!(resolve(&["relative.txt".into()], &p(&proj)).is_err());
        assert!(resolve(&[p(&proj.join("gone.txt"))], &p(&proj)).is_err());
        assert!(resolve(&vec![p(&outside); 21], &p(&proj)).is_err());
    }

    #[test]
    fn pasted_images_are_kept_in_the_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        let path = stash(dir.path(), "image.png", &b64).unwrap();
        assert!(path.contains("/attachments/pasted-") && path.ends_with(".png"));
        assert_eq!(std::fs::read(&path).unwrap(), vec![1, 2, 3]);
        assert!(stash(dir.path(), "x.exe", &b64).is_err());
        assert!(stash(dir.path(), "x.png", "%%%").is_err());
    }
}
