use super::*;
use gigcraft_model::{Part, Set, SongEntry};

fn song(title: &str, parts: &[&str]) -> Song {
    let mut s = Song::new(title);
    s.key = Some("G".into());
    s.parts = parts
        .iter()
        .map(|p| Part {
            name: (*p).into(),
            actions: Vec::new(),
            notes: String::new(),
        })
        .collect();
    s
}

fn show() -> (Vec<Song>, Show) {
    let songs = vec![
        song("A", &["Intro", "Verse", "Solo"]),
        song("B", &[]),
        song("C", &["Only"]),
    ];
    let mut list = Setlist::new("Gig");
    list.sets[0].entries = vec![
        Entry::Song(SongEntry {
            key: Some("A".into()),
            ..SongEntry::new(songs[0].id)
        }),
        Entry::Break {
            label: "Tune".into(),
            minutes: 2,
        },
        Entry::Song(SongEntry::new(songs[1].id)),
        Entry::Song(SongEntry::new(SongId::new())),
    ];
    list.sets.push(Set {
        name: "Encore".into(),
        entries: vec![Entry::Song(SongEntry::new(songs[2].id))],
    });
    let sh = Show::new(&list, |id| songs.iter().find(|s| s.id == id));
    (songs, sh)
}

#[test]
fn flattens_with_numbers_overrides_and_missing() {
    let (_, sh) = show();
    assert_eq!(sh.items.len(), 5);
    assert!(matches!(&sh.items[0], Item::Song { key: Some(k), number: 1, .. } if k == "A"));
    assert!(matches!(&sh.items[1], Item::Break { minutes: 2, .. }));
    assert!(matches!(&sh.items[2], Item::Song { number: 2, .. }));
    assert!(matches!(&sh.items[3], Item::Missing { .. }));
    assert!(matches!(
        &sh.items[4],
        Item::Song {
            number: 1,
            set: 1,
            ..
        }
    ));
    assert_eq!(sh.set_names, ["Set 1", "Encore"]);
}

#[test]
fn next_steps_through_parts_then_songs() {
    let (songs, mut sh) = show();
    assert_eq!(
        sh.start(),
        vec![
            Effect::Show { index: 0 },
            Effect::SendPart {
                song: songs[0].id,
                part: 0
            }
        ]
    );
    assert_eq!(
        sh.apply(Action::Next),
        vec![Effect::SendPart {
            song: songs[0].id,
            part: 1
        }]
    );
    sh.apply(Action::Next);
    assert_eq!(sh.part, 2);
    assert_eq!(
        sh.apply(Action::Next),
        vec![Effect::Show { index: 1 }, Effect::Break { minutes: 2 }]
    );
    assert_eq!(
        sh.apply(Action::Next),
        vec![
            Effect::Show { index: 2 },
            Effect::SendPart {
                song: songs[1].id,
                part: 0
            }
        ]
    );
    // Previous goes back to the break, then to the last part of A.
    sh.apply(Action::Previous);
    assert_eq!(
        sh.apply(Action::Previous),
        vec![
            Effect::Show { index: 0 },
            Effect::SendPart {
                song: songs[0].id,
                part: 2
            }
        ]
    );
    // On the first song, "previous song" goes back to its first part.
    assert_eq!(
        sh.apply(Action::PreviousSong),
        vec![Effect::SendPart {
            song: songs[0].id,
            part: 0
        }]
    );
    assert_eq!(sh.apply(Action::Previous), vec![Effect::Edge]);
}

#[test]
fn edges_and_other_actions() {
    let (_, mut sh) = show();
    sh.goto(99, 99);
    assert_eq!(sh.index, 4);
    assert_eq!(sh.part, 0);
    assert_eq!(sh.apply(Action::Next), vec![Effect::Edge]);
    assert_eq!(sh.apply(Action::NextSong), vec![Effect::Edge]);
    assert_eq!(sh.apply(Action::Panic), vec![Effect::Panic]);
    assert_eq!(sh.apply(Action::PageForward), vec![Effect::PageForward]);
    assert_eq!(sh.apply(Action::Exit), vec![Effect::Exit]);
    let mut empty = Show::new(
        &Setlist {
            sets: Vec::new(),
            ..Setlist::new("x")
        },
        |_| None,
    );
    assert_eq!(empty.start(), vec![Effect::Edge]);
    assert_eq!(empty.apply(Action::Next), vec![Effect::Edge]);
    assert_eq!(empty.goto(3, 3), vec![Effect::Edge]);
}

#[test]
fn keymap_defaults_learn_and_serde() {
    let mut km = Keymap::default();
    assert_eq!(km.action("arrowright", Gesture::Press), Some(Action::Next));
    assert_eq!(
        km.action("ArrowRight", Gesture::Hold),
        Some(Action::NextSong)
    );
    assert!(km.has_extended_gesture("ArrowRight"));
    assert!(!km.has_extended_gesture("Space"));
    km.learn("B", Gesture::Press, Action::Panic);
    km.learn("b", Gesture::Press, Action::Resend);
    assert_eq!(km.action("B", Gesture::Press), Some(Action::Resend));
    let text = toml::to_string(&km).expect("toml");
    let back: Keymap = toml::from_str(&text).expect("parse");
    assert_eq!(back, km);
    let json = serde_json::to_value(Action::PreviousSong).expect("json");
    assert_eq!(json, "previous_song");
}
