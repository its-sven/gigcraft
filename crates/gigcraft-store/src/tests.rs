use super::*;
use gigcraft_model::{Entry, SongEntry};

fn lib() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().expect("tempdir");
    let lib = Library::create(dir.path().join("lib"), "Test").expect("create");
    (dir, lib)
}

#[test]
fn create_save_reopen() {
    let (_d, mut l) = lib();
    let mut song = Song::new("Harbour Lights");
    song.artist = "Demo".into();
    let id = song.id;
    l.save_song(song.clone(), Some("{title: Harbour Lights}\n[G]Hello"))
        .expect("save");
    let mut list = Setlist::new("Gig");
    list.date = Some("2026-12-31".into());
    list.sets[0].entries.push(Entry::Song(SongEntry::new(id)));
    l.save_setlist(list.clone()).expect("save setlist");

    assert!(l.root().join("songs/harbour-lights/song.toml").is_file());
    assert!(l.root().join("songs/harbour-lights/chart.cho").is_file());
    assert!(l.root().join("setlists/2026-12-31-gig.toml").is_file());

    let again = Library::open(l.root()).expect("open");
    assert!(again.problems().is_empty(), "{:?}", again.problems());
    assert_eq!(again.song(id), Some(&song));
    assert_eq!(again.chart(id), Some("{title: Harbour Lights}\n[G]Hello"));
    assert_eq!(again.setlist(list.id), Some(&list));
}

#[test]
fn create_refuses_existing_and_open_refuses_plain_folder() {
    let (d, l) = lib();
    assert!(matches!(
        Library::create(l.root(), "x"),
        Err(StoreError::AlreadyALibrary(_))
    ));
    assert!(matches!(
        Library::open(d.path()),
        Err(StoreError::NotALibrary(_))
    ));
}

#[test]
fn same_title_gets_new_folder_and_rename_keeps_folder() {
    let (_d, mut l) = lib();
    let a = Song::new("Encore");
    let mut b = Song::new("Encore");
    l.save_song(a.clone(), None).expect("a");
    l.save_song(b.clone(), None).expect("b");
    assert_eq!(l.song_path(b.id), Some(PathBuf::from("songs/encore-2")));
    b.title = "Something else".into();
    l.save_song(b.clone(), None).expect("rename");
    assert_eq!(l.song_path(b.id), Some(PathBuf::from("songs/encore-2")));
    assert_eq!(l.chart(a.id), Some(""));
}

#[test]
fn bad_files_become_problems() {
    let (_d, mut l) = lib();
    let good = Song::new("Good");
    l.save_song(good.clone(), None).expect("good");
    let bad_dir = l.root().join("songs/broken");
    fs::create_dir_all(&bad_dir).expect("dir");
    fs::write(bad_dir.join(SONG_FILE), "title = [").expect("write");
    // A copied folder with the same id.
    let copy = l.root().join("songs/zz-copy");
    fs::create_dir_all(&copy).expect("dir");
    fs::copy(l.root().join("songs/good/song.toml"), copy.join(SONG_FILE)).expect("copy");
    fs::write(l.root().join("setlists/nope.toml"), "name = 1").expect("write");
    fs::write(l.root().join("setlists/readme.txt"), "ignored").expect("write");

    l.reload().expect("reload");
    assert_eq!(l.songs().len(), 1);
    assert_eq!(l.problems().len(), 3, "{:?}", l.problems());
}

#[test]
fn trash_moves_instead_of_deleting() {
    let (_d, mut l) = lib();
    let s = Song::new("Gone");
    l.save_song(s.clone(), Some("x")).expect("save");
    l.trash_song(s.id).expect("trash");
    assert!(l.song(s.id).is_none());
    assert!(!l.root().join("songs/gone").exists());
    let trashed: Vec<_> = fs::read_dir(l.root().join(TRASH_DIR))
        .expect("trash")
        .collect();
    assert_eq!(trashed.len(), 1);
    // A new song may reuse the slug.
    l.save_song(Song::new("Gone"), None).expect("again");
    assert!(l.root().join("songs/gone").exists());
    assert!(matches!(
        l.trash_song(s.id),
        Err(StoreError::UnknownSong(_))
    ));
}

#[test]
fn search_and_sorting() {
    let (_d, mut l) = lib();
    for t in ["zebra", "Äpfel", "apple"] {
        l.save_song(Song::new(t), None).expect("save");
    }
    let titles: Vec<&str> = l.songs().iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Äpfel", "apple", "zebra"], "Ä sorts as A");
    assert_eq!(l.search("apf").len(), 1);
}

#[test]
fn attachments_are_copied_with_safe_names() {
    let (d, mut l) = lib();
    let s = Song::new("Charted");
    l.save_song(s.clone(), None).expect("save");
    let src = d.path().join("My Chart (v2).PDF");
    fs::write(&src, b"%PDF-1.4 test").expect("write");
    let a = l.copy_into_song(s.id, &src).expect("copy");
    let b = l.copy_into_song(s.id, &src).expect("copy again");
    assert_eq!(a, "my-chart-v2.pdf");
    assert_eq!(b, "my-chart-v2-2.pdf");
    assert!(l.attachment_path(s.id, &a).is_some_and(|p| p.is_file()));
    assert!(l.attachment_path(s.id, "../song.toml").is_none());
    assert!(l.attachment_path(s.id, "song.toml").is_none());
    l.trash_attachment(s.id, &a).expect("trash");
    assert!(l.attachment_path(s.id, &a).is_some_and(|p| !p.exists()));
    assert!(
        l.copy_into_song(s.id, d.path()).is_err(),
        "folders are refused"
    );
}
