//! A GigCraft library on disk.
//!
//! A library is a folder of readable files (see ADR-0003):
//!
//! ```text
//! <library>/
//!   gigcraft-library.toml        manifest (id, name, schema)
//!   songs/<slug>/song.toml       song metadata, parts, MIDI actions
//!   songs/<slug>/chart.cho       ChordPro chart (optional)
//!   setlists/<slug>.toml         setlists
//!   .trash/<time>-<slug>/        deleted songs and setlists, never removed by GigCraft
//! ```
//!
//! Loading never fails because of one bad file: it is skipped and reported in
//! [`Library::problems`]. Saves are atomic (`craft-atomic`). A song keeps its
//! folder when its title changes, so links and sync tools see stable paths.

mod search;
mod slug;

pub use search::normalize;
pub use slug::slugify;

mod backup;

use gigcraft_model::{LibraryInfo, Setlist, SetlistId, Song, SongId};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MANIFEST: &str = "gigcraft-library.toml";
pub const SONGS_DIR: &str = "songs";
pub const SETLISTS_DIR: &str = "setlists";
pub const TRASH_DIR: &str = ".trash";
pub const SONG_FILE: &str = "song.toml";
pub const CHART_FILE: &str = "chart.cho";

/// Files larger than this are not loaded (protects against accidents and hostile input).
pub const MAX_FILE_BYTES: u64 = 4 << 20;

/// Largest attachment copied into a library (500 MB; backing tracks can be big).
pub const MAX_ATTACHMENT_BYTES: u64 = 500 << 20;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Format { path: PathBuf, message: String },
    #[error("{0} is not a GigCraft library (no {MANIFEST})")]
    NotALibrary(PathBuf),
    #[error("{0} already contains a GigCraft library")]
    AlreadyALibrary(PathBuf),
    #[error("no song with id {0}")]
    UnknownSong(SongId),
    #[error("no setlist with id {0}")]
    UnknownSetlist(SetlistId),
    #[error("{0} was changed by another device or program; reload the library first")]
    Conflict(PathBuf),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// A file that could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
struct StoredSong {
    song: Song,
    /// Folder name under `songs/`.
    slug: String,
    chart: String,
    /// Fingerprint of the files on disk when last read or written.
    disk: u64,
}

#[derive(Debug, Clone)]
struct StoredSetlist {
    setlist: Setlist,
    /// File stem under `setlists/`.
    slug: String,
    disk: u64,
}

/// An open library: everything is held in memory, saves go straight to disk.
#[derive(Debug)]
pub struct Library {
    root: PathBuf,
    info: LibraryInfo,
    songs: BTreeMap<SongId, StoredSong>,
    setlists: BTreeMap<SetlistId, StoredSetlist>,
    problems: Vec<Problem>,
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn read_text(path: &Path) -> Result<String> {
    let meta = fs::metadata(path).map_err(io(path))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(StoreError::Format {
            path: path.to_path_buf(),
            message: format!("file is larger than {MAX_FILE_BYTES} bytes"),
        });
    }
    fs::read_to_string(path).map_err(io(path))
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = read_text(path)?;
    toml::from_str(&text).map_err(|e| StoreError::Format {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

fn write_toml<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = toml::to_string_pretty(value).map_err(|e| StoreError::Format {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    write_file(path, text.as_bytes())
}

/// Fingerprint of the current contents of `paths` (a missing file counts as such).
fn fingerprint(paths: &[PathBuf]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in paths {
        match fs::read(p) {
            Ok(bytes) => {
                1u8.hash(&mut h);
                bytes.hash(&mut h);
            }
            Err(_) => 0u8.hash(&mut h),
        }
    }
    h.finish()
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io(parent))?;
    }
    craft_atomic::atomic_write(path, bytes).map_err(io(path))
}

impl Library {
    /// Create a new, empty library in `root` (created if missing).
    pub fn create(root: impl Into<PathBuf>, name: &str) -> Result<Self> {
        let root = root.into();
        let manifest = root.join(MANIFEST);
        if manifest.exists() {
            return Err(StoreError::AlreadyALibrary(root));
        }
        for dir in [SONGS_DIR, SETLISTS_DIR] {
            let p = root.join(dir);
            fs::create_dir_all(&p).map_err(io(&p))?;
        }
        let info = LibraryInfo::new(name);
        write_toml(&manifest, &info)?;
        Ok(Self {
            root,
            info,
            songs: BTreeMap::new(),
            setlists: BTreeMap::new(),
            problems: Vec::new(),
        })
    }

    /// Open an existing library and load all songs and setlists.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let manifest = root.join(MANIFEST);
        if !manifest.is_file() {
            return Err(StoreError::NotALibrary(root));
        }
        let info: LibraryInfo = read_toml(&manifest)?;
        let mut lib = Self {
            root,
            info,
            songs: BTreeMap::new(),
            setlists: BTreeMap::new(),
            problems: Vec::new(),
        };
        lib.load_all();
        Ok(lib)
    }

    /// Read everything from disk again (after changes by another device or tool).
    pub fn reload(&mut self) -> Result<()> {
        self.info = read_toml(&self.root.join(MANIFEST))?;
        self.load_all();
        Ok(())
    }

    fn load_all(&mut self) {
        self.songs.clear();
        self.setlists.clear();
        self.problems.clear();
        for (slug, dir) in self.list_dir(SONGS_DIR, true) {
            let path = dir.join(SONG_FILE);
            if !path.is_file() {
                continue;
            }
            let song: Song = match read_toml(&path) {
                Ok(s) => s,
                Err(e) => {
                    self.problem(&path, e);
                    continue;
                }
            };
            if let Some(other) = self.songs.get(&song.id) {
                let msg = format!(
                    "duplicate song id {} (also in songs/{})",
                    song.id, other.slug
                );
                self.problems.push(Problem { path, message: msg });
                continue;
            }
            let chart_path = dir.join(CHART_FILE);
            let chart = if chart_path.is_file() {
                read_text(&chart_path).unwrap_or_else(|e| {
                    self.problem(&chart_path, e);
                    String::new()
                })
            } else {
                String::new()
            };
            let disk = fingerprint(&[path.clone(), chart_path.clone()]);
            self.songs.insert(
                song.id,
                StoredSong {
                    song,
                    slug,
                    chart,
                    disk,
                },
            );
        }
        for (slug, path) in self.list_dir(SETLISTS_DIR, false) {
            let Some(stem) = slug.strip_suffix(".toml") else {
                continue;
            };
            match read_toml::<Setlist>(&path) {
                Ok(setlist) if self.setlists.contains_key(&setlist.id) => {
                    let msg = format!("duplicate setlist id {}", setlist.id);
                    self.problems.push(Problem { path, message: msg });
                }
                Ok(setlist) => {
                    let disk = fingerprint(std::slice::from_ref(&path));
                    self.setlists.insert(
                        setlist.id,
                        StoredSetlist {
                            setlist,
                            slug: stem.to_string(),
                            disk,
                        },
                    );
                }
                Err(e) => self.problem(&path, e),
            }
        }
    }

    fn problem(&mut self, path: &Path, e: StoreError) {
        let message = match e {
            StoreError::Format { message, .. } => message,
            other => other.to_string(),
        };
        self.problems.push(Problem {
            path: path.to_path_buf(),
            message,
        });
    }

    /// Visible entries of `root/sub`, sorted by name: directories or files.
    fn list_dir(&mut self, sub: &str, dirs: bool) -> Vec<(String, PathBuf)> {
        let dir = self.root.join(sub);
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                self.problem(
                    &dir,
                    StoreError::Io {
                        path: dir.clone(),
                        source: e,
                    },
                );
                return Vec::new();
            }
        };
        let mut out: Vec<(String, PathBuf)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_str()?.to_string();
                let is_dir = e.file_type().ok()?.is_dir();
                (!name.starts_with('.') && is_dir == dirs).then(|| (name, e.path()))
            })
            .collect();
        out.sort();
        out
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn info(&self) -> &LibraryInfo {
        &self.info
    }

    /// Files that could not be loaded at the last open or reload.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// All songs, sorted by title, then artist.
    #[must_use]
    pub fn songs(&self) -> Vec<&Song> {
        let mut v: Vec<&Song> = self.songs.values().map(|s| &s.song).collect();
        v.sort_by_cached_key(|s| (normalize(&s.title), normalize(&s.artist)));
        v
    }

    #[must_use]
    pub fn song(&self, id: SongId) -> Option<&Song> {
        self.songs.get(&id).map(|s| &s.song)
    }

    /// The ChordPro chart of a song ("" if it has none).
    #[must_use]
    pub fn chart(&self, id: SongId) -> Option<&str> {
        self.songs.get(&id).map(|s| s.chart.as_str())
    }

    /// Folder of a song, relative to the library root.
    #[must_use]
    pub fn song_path(&self, id: SongId) -> Option<PathBuf> {
        self.songs
            .get(&id)
            .map(|s| Path::new(SONGS_DIR).join(&s.slug))
    }

    /// Songs matching all words of `query` (see [`search::matches`]), sorted like [`Self::songs`].
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&Song> {
        let q = search::Query::parse(query);
        self.songs().into_iter().filter(|s| q.matches(s)).collect()
    }

    /// Insert or update a song. `chart: None` keeps the current chart.
    pub fn save_song(&mut self, song: Song, chart: Option<&str>) -> Result<()> {
        let slug = match self.songs.get(&song.id) {
            Some(existing) => existing.slug.clone(),
            None => self.free_slug(&song.title, SONGS_DIR, |l| {
                l.songs.values().map(|s| &s.slug)
            }),
        };
        let dir = self.root.join(SONGS_DIR).join(&slug);
        let files = [dir.join(SONG_FILE), dir.join(CHART_FILE)];
        if let Some(existing) = self.songs.get(&song.id)
            && fingerprint(&files) != existing.disk
        {
            return Err(StoreError::Conflict(dir));
        }
        write_toml(&dir.join(SONG_FILE), &song)?;
        let chart_text = match chart {
            Some(text) => {
                write_file(&dir.join(CHART_FILE), text.as_bytes())?;
                text.to_string()
            }
            None => self
                .songs
                .get(&song.id)
                .map(|s| s.chart.clone())
                .unwrap_or_default(),
        };
        let disk = fingerprint(&files);
        self.songs.insert(
            song.id,
            StoredSong {
                song,
                slug,
                chart: chart_text,
                disk,
            },
        );
        Ok(())
    }

    /// How many songs and setlists differ between memory and disk (changed,
    /// added or removed by another device or program). Cheap enough to poll
    /// every few seconds for libraries of a few thousand songs.
    #[must_use]
    pub fn external_changes(&self) -> usize {
        let mut changed = 0;
        let songs_dir = self.root.join(SONGS_DIR);
        let mut seen = BTreeSet::new();
        if let Ok(entries) = fs::read_dir(&songs_dir) {
            for e in entries.flatten() {
                let Some(name) = e.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if name.starts_with('.') || !e.path().join(SONG_FILE).is_file() {
                    continue;
                }
                seen.insert(name);
            }
        }
        let known: BTreeSet<&str> = self.songs.values().map(|s| s.slug.as_str()).collect();
        let problem_paths: BTreeSet<&Path> =
            self.problems.iter().map(|p| p.path.as_path()).collect();
        for slug in &seen {
            let dir = songs_dir.join(slug);
            if !known.contains(slug.as_str())
                && !problem_paths.contains(dir.join(SONG_FILE).as_path())
            {
                changed += 1;
            }
        }
        for s in self.songs.values() {
            let dir = songs_dir.join(&s.slug);
            if !seen.contains(&s.slug)
                || fingerprint(&[dir.join(SONG_FILE), dir.join(CHART_FILE)]) != s.disk
            {
                changed += 1;
            }
        }
        let lists_dir = self.root.join(SETLISTS_DIR);
        let known_lists: BTreeSet<String> = self
            .setlists
            .values()
            .map(|s| format!("{}.toml", s.slug))
            .collect();
        if let Ok(entries) = fs::read_dir(&lists_dir) {
            for e in entries.flatten() {
                let Some(name) = e.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if name.ends_with(".toml")
                    && !name.starts_with('.')
                    && !known_lists.contains(&name)
                    && !problem_paths.contains(e.path().as_path())
                {
                    changed += 1;
                }
            }
        }
        for s in self.setlists.values() {
            if fingerprint(&[lists_dir.join(format!("{}.toml", s.slug))]) != s.disk {
                changed += 1;
            }
        }
        changed
    }

    /// Absolute path of an attachment file of a song, if the name is safe.
    #[must_use]
    pub fn attachment_path(&self, id: SongId, file: &str) -> Option<PathBuf> {
        let stored = self.songs.get(&id)?;
        gigcraft_model::Attachment::is_safe_name(file)
            .then(|| self.root.join(SONGS_DIR).join(&stored.slug).join(file))
    }

    /// Copy `source` into the song's folder under a free, safe name (the
    /// record is not changed; the caller adds the attachment). Returns the name.
    pub fn copy_into_song(&mut self, id: SongId, source: &Path) -> Result<String> {
        let stored = self.songs.get(&id).ok_or(StoreError::UnknownSong(id))?;
        let meta = fs::metadata(source).map_err(io(source))?;
        if !meta.is_file() {
            return Err(StoreError::Format {
                path: source.to_path_buf(),
                message: "not a file".into(),
            });
        }
        if meta.len() > MAX_ATTACHMENT_BYTES {
            return Err(StoreError::Format {
                path: source.to_path_buf(),
                message: format!("file is larger than {} MB", MAX_ATTACHMENT_BYTES >> 20),
            });
        }
        let original = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        let (stem, ext) = match original.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s, format!(".{}", slugify(e))),
            _ => (original, String::new()),
        };
        let base = slugify(stem);
        let dir = self.root.join(SONGS_DIR).join(&stored.slug);
        let mut name = format!("{base}{ext}");
        let mut n = 2;
        while dir.join(&name).exists() || !gigcraft_model::Attachment::is_safe_name(&name) {
            name = format!("{base}-{n}{ext}");
            n += 1;
        }
        let bytes = fs::read(source).map_err(io(source))?;
        write_file(&dir.join(&name), &bytes)?;
        Ok(name)
    }

    /// Move an attachment file of a song to `.trash/` (the record is not changed).
    pub fn trash_attachment(&mut self, id: SongId, file: &str) -> Result<()> {
        let path = self
            .attachment_path(id, file)
            .ok_or(StoreError::UnknownSong(id))?;
        if path.exists() {
            let slug = self
                .songs
                .get(&id)
                .map(|s| s.slug.clone())
                .unwrap_or_default();
            self.move_to_trash(&path, &format!("{slug}-{file}"))?;
        }
        Ok(())
    }

    /// Move a song's folder to `.trash/`. Setlist entries pointing to it stay
    /// (shown as missing) so nothing else changes silently.
    pub fn trash_song(&mut self, id: SongId) -> Result<()> {
        let stored = self.songs.get(&id).ok_or(StoreError::UnknownSong(id))?;
        let from = self.root.join(SONGS_DIR).join(&stored.slug);
        self.move_to_trash(&from, &stored.slug.clone())?;
        self.songs.remove(&id);
        Ok(())
    }

    /// All setlists, sorted by date (newest first, undated last), then name.
    #[must_use]
    pub fn setlists(&self) -> Vec<&Setlist> {
        let mut v: Vec<&Setlist> = self.setlists.values().map(|s| &s.setlist).collect();
        v.sort_by(|a, b| {
            let key = |s: &Setlist| s.date.clone().unwrap_or_default();
            key(b)
                .cmp(&key(a))
                .then_with(|| normalize(&a.name).cmp(&normalize(&b.name)))
        });
        v
    }

    #[must_use]
    pub fn setlist(&self, id: SetlistId) -> Option<&Setlist> {
        self.setlists.get(&id).map(|s| &s.setlist)
    }

    /// Insert or update a setlist.
    pub fn save_setlist(&mut self, setlist: Setlist) -> Result<()> {
        let slug = match self.setlists.get(&setlist.id) {
            Some(existing) => existing.slug.clone(),
            None => {
                let base = match &setlist.date {
                    Some(d) => format!("{d} {}", setlist.name),
                    None => setlist.name.clone(),
                };
                self.free_slug(&base, SETLISTS_DIR, |l| {
                    l.setlists.values().map(|s| &s.slug)
                })
            }
        };
        let path = self.root.join(SETLISTS_DIR).join(format!("{slug}.toml"));
        if let Some(existing) = self.setlists.get(&setlist.id)
            && fingerprint(std::slice::from_ref(&path)) != existing.disk
        {
            return Err(StoreError::Conflict(path));
        }
        write_toml(&path, &setlist)?;
        let disk = fingerprint(std::slice::from_ref(&path));
        self.setlists.insert(
            setlist.id,
            StoredSetlist {
                setlist,
                slug,
                disk,
            },
        );
        Ok(())
    }

    /// Move a setlist file to `.trash/`.
    pub fn trash_setlist(&mut self, id: SetlistId) -> Result<()> {
        let stored = self
            .setlists
            .get(&id)
            .ok_or(StoreError::UnknownSetlist(id))?;
        let file = format!("{}.toml", stored.slug);
        let from = self.root.join(SETLISTS_DIR).join(&file);
        self.move_to_trash(&from, &file)?;
        self.setlists.remove(&id);
        Ok(())
    }

    fn move_to_trash(&self, from: &Path, name: &str) -> Result<()> {
        let trash = self.root.join(TRASH_DIR);
        fs::create_dir_all(&trash).map_err(io(&trash))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut to = trash.join(format!("{stamp}-{name}"));
        let mut n = 2;
        while to.exists() {
            to = trash.join(format!("{stamp}-{n}-{name}"));
            n += 1;
        }
        fs::rename(from, &to).map_err(io(from))
    }

    /// A slug for `title` not used in memory nor on disk under `sub`.
    fn free_slug<'a, F, I>(&'a self, title: &str, sub: &str, used: F) -> String
    where
        F: Fn(&'a Self) -> I,
        I: Iterator<Item = &'a String>,
    {
        let base = slugify(title);
        let taken: BTreeSet<&str> = used(self).map(String::as_str).collect();
        let free = |s: &str| {
            !taken.contains(s)
                && !self.root.join(sub).join(s).exists()
                && !self.root.join(sub).join(format!("{s}.toml")).exists()
        };
        if free(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|s| free(s))
            .unwrap_or(base)
    }
}

#[cfg(test)]
mod tests;
