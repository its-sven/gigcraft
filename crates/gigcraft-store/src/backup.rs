//! Backups: the whole library (without its trash) as one ZIP file.
//!
//! A library is a plain folder, so restoring is unzipping a backup anywhere
//! and opening the folder. Audio, images and PDFs are stored as they are
//! (already compressed), text is deflated; large files are streamed.

use crate::{Library, StoreError, TRASH_DIR};
use craft_zip::{Compression, Writer};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const PREFIX: &str = "GigCraft-";

/// Year, month, day, hour, minute (UTC) of a Unix time.
fn civil(unix: u64) -> (i64, u32, u32, u32, u32) {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, (secs / 3600) as u32, ((secs % 3600) / 60) as u32)
}

/// File-name safe library name.
fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "Library".into() } else { s }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), StoreError> {
    for entry in std::fs::read_dir(dir).map_err(io(dir))? {
        let entry = entry.map_err(io(dir))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if dir == root && name == TRASH_DIR {
            continue;
        }
        // Temporary files of crash-safe saves and OS clutter.
        if name.starts_with(".tmp") || name == ".DS_Store" {
            continue;
        }
        let kind = entry.file_type().map_err(io(&path))?;
        if kind.is_dir() {
            files(root, &path, out)?;
        } else if kind.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn compression_for(path: &Path) -> Compression {
    let compressed = [
        "mp3", "m4a", "aac", "flac", "ogg", "opus", "pdf", "png", "jpg", "jpeg", "gif", "heic",
        "zip", "mp4",
    ];
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
    {
        Some(e) if compressed.contains(&e.as_str()) => Compression::Stored,
        _ => Compression::Deflated,
    }
}

impl Library {
    /// Write a backup into `dir`; `now_unix` names it
    /// (`GigCraft-<library>-2026-11-26-0130.zip`). Returns its path.
    pub fn backup(&self, dir: &Path, now_unix: u64) -> Result<PathBuf, StoreError> {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
        let (y, m, d, h, min) = civil(now_unix);
        let base = format!(
            "{PREFIX}{}-{y:04}-{m:02}-{d:02}-{h:02}{min:02}",
            slug(&self.info().name)
        );
        let mut path = dir.join(format!("{base}.zip"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{base}-{n}.zip"));
            n += 1;
        }
        let root = self.root().to_path_buf();
        let mut list = Vec::new();
        files(&root, &root, &mut list)?;
        list.sort();
        // Write next to the target and rename, so a half-written backup never looks complete.
        let partial = path.with_extension("zip.partial");
        let file = File::create(&partial).map_err(io(&partial))?;
        let mut zip = Writer::to_writer(BufWriter::new(file));
        let top = slug(&self.info().name);
        for f in &list {
            let rel = f.strip_prefix(&root).unwrap_or(f);
            let name = format!(
                "{top}/{}",
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            );
            let src = f.clone();
            zip.add_with(&name, compression_for(f), move |w| {
                let mut input = File::open(&src)?;
                std::io::copy(&mut input, w).map(|_| ())
            })
            .map_err(|e| StoreError::Format {
                path: f.clone(),
                message: e.to_string(),
            })?;
        }
        zip.finish_into()
            .map_err(|e| StoreError::Format {
                path: partial.clone(),
                message: e.to_string(),
            })?
            .into_inner()
            .map_err(|e| StoreError::Io {
                path: partial.clone(),
                source: e.into_error(),
            })?
            .sync_all()
            .map_err(io(&partial))?;
        std::fs::rename(&partial, &path).map_err(io(&path))?;
        Ok(path)
    }

    /// The daily automatic backup: write one into `dir` unless this library
    /// already has one from today (UTC), and keep only the newest `keep` of
    /// this library's backups there. Returns the new backup, if one was made.
    pub fn auto_backup(
        &self,
        dir: &Path,
        now_unix: u64,
        keep: usize,
    ) -> Result<Option<PathBuf>, StoreError> {
        let mine = format!("{PREFIX}{}-", slug(&self.info().name));
        let (y, m, d, _, _) = civil(now_unix);
        let today = format!("{mine}{y:04}-{m:02}-{d:02}-");
        let existing = |dir: &Path| -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|e| e == "zip")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(&mine))
                })
                .collect();
            v.sort();
            v
        };
        if existing(dir).iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&today))
        }) {
            return Ok(None);
        }
        let made = self.backup(dir, now_unix)?;
        let all = existing(dir);
        if all.len() > keep {
            // Old automatic backups go; the library itself is untouched.
            for old in &all[..all.len() - keep] {
                let _ = std::fs::remove_file(old);
            }
        }
        Ok(Some(made))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0));
        assert_eq!(civil(1795654800), (2026, 11, 26, 1, 0));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0));
        assert_eq!(slug("Demo Gig / 2026"), "Demo-Gig---2026");
        assert_eq!(slug("  "), "Library");
    }

    #[test]
    fn backup_contains_the_library_without_its_trash() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("lib");
        let lib = Library::create(&lib_dir, "Band Book").unwrap();
        std::fs::create_dir_all(lib_dir.join("songs/a")).unwrap();
        std::fs::write(lib_dir.join("songs/a/chart.cho"), "[G]Hello").unwrap();
        std::fs::write(lib_dir.join("songs/a/take.mp3"), vec![7u8; 10_000]).unwrap();
        std::fs::create_dir_all(lib_dir.join(TRASH_DIR)).unwrap();
        std::fs::write(lib_dir.join(TRASH_DIR).join("old.toml"), "x").unwrap();
        let out = dir.path().join("backups");
        let zip = lib.backup(&out, 1795654800).unwrap();
        assert!(zip.ends_with("GigCraft-Band-Book-2026-11-26-0100.zip"));
        let bytes = std::fs::read(&zip).unwrap();
        let mut archive = craft_zip::Archive::open(
            &bytes,
            craft_zip::Limits {
                max_entries: 1000,
                max_total_bytes: 1 << 30,
            },
        )
        .unwrap();
        let names: Vec<String> = archive.names().map(|n| n.to_string()).collect();
        assert!(
            names.contains(&"Band-Book/gigcraft-library.toml".to_string()),
            "{names:?}"
        );
        assert!(names.iter().all(|n| !n.contains(".trash")));
        assert_eq!(
            archive
                .read("Band-Book/songs/a/chart.cho", 1 << 20)
                .unwrap()
                .unwrap(),
            b"[G]Hello"
        );
        assert_eq!(
            archive
                .read("Band-Book/songs/a/take.mp3", 1 << 20)
                .unwrap()
                .unwrap()
                .len(),
            10_000
        );
        // A second backup in the same minute gets its own name.
        let again = lib.backup(&out, 1795654800).unwrap();
        assert!(again.ends_with("GigCraft-Band-Book-2026-11-26-0100-2.zip"));
        assert!(
            !out.join("GigCraft-Band-Book-2026-11-26-0100.zip.partial")
                .exists()
        );
    }

    #[test]
    fn automatic_backups_once_a_day_and_only_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::create(dir.path().join("lib"), "Gig").unwrap();
        let out = dir.path().join("auto");
        let day = 86_400;
        let t0 = 1795654800;
        assert!(lib.auto_backup(&out, t0, 3).unwrap().is_some());
        assert!(lib.auto_backup(&out, t0 + 3600, 3).unwrap().is_none());
        for i in 1..5 {
            assert!(lib.auto_backup(&out, t0 + i * day, 3).unwrap().is_some());
        }
        let mut left: Vec<String> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left.len(), 3);
        assert!(left[0].contains("2026-11-28"), "{left:?}");
        // Another library's backups in the same folder are left alone.
        let other = Library::create(dir.path().join("other"), "Other").unwrap();
        other.backup(&out, t0).unwrap();
        assert!(lib.auto_backup(&out, t0 + 9 * day, 3).unwrap().is_some());
        assert!(std::fs::read_dir(&out).unwrap().count() == 4);
    }
}
