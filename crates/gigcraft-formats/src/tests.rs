//! Tests over the public API. All song texts are written for these tests.

use super::*;
use gigcraft_model::{Entry, MidiMessage, Set, SongEntry, SongId};
use std::collections::BTreeMap;

const ONSONG: &str = "Harbour Lights\nThe Demo Band\nKey: G\nTempo: 96\nTime: 3/4\nDuration: 3:30\n\
Capo: 2\nTags: Folk, Ballad\nCopyright: 2026 The Demo Band\n\n\
Verse 1:\nG          C\nLanterns on the water\nD             G\nboats are coming home\n\n\
Chorus:\n[Em]Sing it [C]low\n\nChorus:\n";

fn chords_of(chart: &str) -> Vec<String> {
    let parsed = gigcraft_chordpro::parse(chart, 0);
    parsed
        .chart
        .blocks
        .iter()
        .flat_map(|b| b.lines.iter())
        .filter_map(|l| match l {
            gigcraft_chordpro::ChartLine::Lyrics { segments } => Some(segments),
            _ => None,
        })
        .flatten()
        .filter_map(|s| s.chord.clone())
        .collect()
}

// --- ChordPro --------------------------------------------------------------

#[test]
fn chordpro_fills_the_record_and_keeps_the_chart() {
    let text = "{title: Harbour Lights}\r\n{subtitle: The Demo Band}\r\n{key: G}\r\n{tempo: 96}\r\n\
{time: 4/4}\r\n{capo: 2}\r\n{duration: 245}\r\n{tag: Folk}\r\n\r\n[G]Lanterns on the [C]water\r\n";
    let imported = import_chordpro(text).unwrap_or_else(|e| panic!("{e}"));
    let s = &imported.song;
    assert_eq!(s.title, "Harbour Lights");
    assert_eq!(
        s.artist, "The Demo Band",
        "subtitle stands in for a missing artist"
    );
    assert_eq!(s.key.as_deref(), Some("G"));
    assert_eq!(s.tempo_bpm, Some(96.0));
    assert_eq!(s.time_signature.as_deref(), Some("4/4"));
    assert_eq!(s.capo, Some(2));
    assert_eq!(s.duration_secs, Some(245));
    assert_eq!(s.tags, ["Folk"]);
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    assert_eq!(imported.chart, text.replace("\r\n", "\n"));
}

#[test]
fn chordpro_warns_about_bad_values() {
    let i = import_chordpro("{tempo: fast}\n{capo: x}\n{duration: soon}\nla la\n")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.song.title, "Untitled");
    assert_eq!(i.song.tempo_bpm, None);
    assert_eq!(i.warnings.len(), 4, "{:?}", i.warnings);
    assert_eq!(import_chordpro("  \n"), Err(FormatError::Empty));
}

#[test]
fn chordpro_export_reflects_the_record_without_duplicates() {
    let chart = "{title: Old}\n{t: Older}\n{subtitle: Live}\n{key: D}\n{tempo: 80}\n{tag: Old tag}\n\n\
{start_of_verse}\n[D]Hello\n{end_of_verse}\n{key: E}\n[E]Higher\n{title: stray}\n";
    let mut song = Song::new("Harbour Lights");
    song.artist = "The Demo Band".into();
    song.key = Some("G".into());
    song.tempo_bpm = Some(92.5);
    song.time_signature = Some("6/8".into());
    song.capo = Some(3);
    song.duration_secs = Some(185);
    song.tags = vec!["Folk".into(), "Encore".into()];
    let out = export_chordpro(&song, chart);
    assert_eq!(
        out,
        "{title: Harbour Lights}\n{artist: The Demo Band}\n{key: G}\n{tempo: 92.5}\n{time: 6/8}\n\
{capo: 3}\n{duration: 3:05}\n{tag: Folk}\n{tag: Encore}\n{subtitle: Live}\n\n\
{start_of_verse}\n[D]Hello\n{end_of_verse}\n{key: E}\n[E]Higher\n"
    );
    // export → import gives the record back
    let back = import_chordpro(&out).unwrap_or_else(|e| panic!("{e}"));
    let mut expected = song.clone();
    expected.id = back.song.id;
    assert_eq!(back.song, expected);
    // exporting again changes nothing
    assert_eq!(export_chordpro(&back.song, &out), out);
}

#[test]
fn chordpro_export_sanitises_values() {
    let mut song = Song::new("A {b}\nc");
    song.key = Some(String::new());
    let out = export_chordpro(&song, "");
    assert_eq!(out, "{title: A b c}\n");
}

// --- OnSong ----------------------------------------------------------------

#[test]
fn onsong_metadata_sections_and_chords() {
    let i = import_onsong(&ONSONG.replace('\n', "\r\n")).unwrap_or_else(|e| panic!("{e}"));
    let s = &i.song;
    assert_eq!(s.title, "Harbour Lights");
    assert_eq!(s.artist, "The Demo Band");
    assert_eq!(s.key.as_deref(), Some("G"));
    assert_eq!(s.tempo_bpm, Some(96.0));
    assert_eq!(s.time_signature.as_deref(), Some("3/4"));
    assert_eq!(s.duration_secs, Some(210));
    assert_eq!(s.capo, Some(2));
    assert_eq!(s.tags, ["Folk", "Ballad"]);
    assert_eq!(
        s.custom.get("Copyright").map(String::as_str),
        Some("2026 The Demo Band")
    );
    assert!(
        i.chart.contains(
            "{start_of_verse: 1}\n[G]Lanterns on[C] the water\n[D]boats are comi[G]ng home\n{end_of_verse}"
        ),
        "{}",
        i.chart
    );
    assert!(
        i.chart
            .contains("{start_of_chorus}\n[Em]Sing it [C]low\n{end_of_chorus}")
    );
    assert!(i.chart.trim_end().ends_with("{chorus}"), "{}", i.chart);
    assert_eq!(chords_of(&i.chart), ["G", "C", "D", "G", "Em", "C"]);
    let parsed = gigcraft_chordpro::parse(&i.chart, 0);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.chart.title.as_deref(), Some("Harbour Lights"));
}

#[test]
fn onsong_with_named_title_and_german_text() {
    let text = "Title: Grüße vom Hafen\nArtist: Die Demo-Band\nKey: Am\n\nStrophe 1:\nAm       G\nÜber die Brücke\n";
    let i = import_onsong(text).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.song.title, "Grüße vom Hafen");
    assert_eq!(i.song.artist, "Die Demo-Band");
    assert!(
        i.chart
            .contains("{start_of_verse: 1}\n[Am]Über die [G]Brücke\n{end_of_verse}"),
        "{}",
        i.chart
    );
}

// --- Plain text ------------------------------------------------------------

#[test]
fn plain_text_chords_over_lyrics() {
    let text = "Night Ferry\nby Anna Beispiel\n\n[Intro]\nC  G  Am  F\n\n[Verse]\nC           G\nThe ferry leaves at nine\n  Am         F\nwe wave from the pier\n\nChorus\nF      G\nAll aboard\n";
    let i = import_plain_text(text, "").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.song.title, "Night Ferry");
    assert_eq!(i.song.artist, "Anna Beispiel");
    assert!(
        i.chart
            .contains("{start_of_intro}\n[C]  [G]  [Am]  [F]\n{end_of_intro}"),
        "{}",
        i.chart
    );
    assert!(
        i.chart.contains("[C]The ferry le[G]aves at nine"),
        "{}",
        i.chart
    );
    assert!(
        i.chart.contains("we[Am] wave from [F]the pier"),
        "{}",
        i.chart
    );
    assert!(
        i.chart.contains("{start_of_chorus}\n[F]All abo[G]ard"),
        "{}",
        i.chart
    );
    let parsed = gigcraft_chordpro::parse(&i.chart, 0);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
}

#[test]
fn plain_lyrics_with_title_hint() {
    let i = import_plain_text(
        "A man walks down\nthe road at dawn\n",
        "Songs/Dawn Road.txt",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.song.title, "Dawn Road");
    assert!(
        i.chart.ends_with("A man walks down\nthe road at dawn\n"),
        "{}",
        i.chart
    );
    assert!(chords_of(&i.chart).is_empty());
    let untitled = import_plain_text("G C\n", "").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(untitled.song.title, "Untitled");
    assert_eq!(untitled.warnings.len(), 1);
}

// --- OpenSong --------------------------------------------------------------

#[test]
fn opensong_xml() {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<song>\n<title>Harbour &amp; Lights</title>\n\
<author>The Demo Band</author>\n<key>G</key>\n<tempo>96</tempo>\n<time_sig>4/4</time_sig>\n<capo print=\"false\">2</capo>\n\
<theme>Folk; Ballad</theme>\n<copyright>2026 Demo</copyright>\n<lyrics>[V1]\n.G       C\n Lanterns on the water\n;quietly\n\n[C]\n Sing it low</lyrics>\n</song>";
    let i = import_opensong(xml).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.song.title, "Harbour & Lights");
    assert_eq!(i.song.artist, "The Demo Band");
    assert_eq!(i.song.capo, Some(2));
    assert_eq!(i.song.tags, ["Folk", "Ballad"]);
    assert_eq!(
        i.song.custom.get("Copyright").map(String::as_str),
        Some("2026 Demo")
    );
    assert!(
        i.chart
            .contains("{start_of_verse: 1}\n[G]Lanterns[C] on the water\n{comment: quietly}"),
        "{}",
        i.chart
    );
    assert!(
        i.chart
            .contains("{start_of_chorus}\nSing it low\n{end_of_chorus}"),
        "{}",
        i.chart
    );
    assert!(import_opensong("<songs/>").is_err());
    assert!(import_opensong("<song><title>x").is_ok());
}

// --- BandHelper TSV --------------------------------------------------------

fn tsv_row(cells: &[&str]) -> String {
    cells.join("\t")
}

#[test]
fn bandhelper_tsv_without_header() {
    let mut cells = vec![""; 18];
    cells[0] = "Harbour Lights";
    cells[1] = "The Demo Band";
    cells[2] = "Folk|Ballad";
    cells[3] = "G";
    cells[4] = "3/4";
    cells[5] = "96";
    cells[6] = "3:30";
    cells[7] = "D";
    cells[8] = "lights.pdf|lights-bass.pdf";
    cells[10] = "Clean|Solo";
    cells[11] = "Lanterns on the water\\nboats are coming home";
    cells[12] = "{start_of_verse}\\n[G]Lanterns on the [C]water\\n{end_of_verse}";
    cells[13] = "Watch the ending";
    cells[14] = "12";
    cells[16] = "Anna";
    let text = format!("{}\r\n\r\n", tsv_row(&cells));
    let songs = import_bandhelper_tsv(&text).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(songs.len(), 1);
    let s = &songs[0].song;
    assert_eq!(s.title, "Harbour Lights");
    assert_eq!(s.tags, ["Folk", "Ballad"]);
    assert_eq!(s.tempo_bpm, Some(96.0));
    assert_eq!(s.duration_secs, Some(210));
    assert_eq!(s.notes, "Watch the ending");
    assert_eq!(
        s.custom.get("Starting Pitch").map(String::as_str),
        Some("D")
    );
    assert_eq!(s.custom.get("Column 17").map(String::as_str), Some("Anna"));
    assert_eq!(
        s.extra["bandhelper"]["documents"],
        serde_json::json!(["lights.pdf", "lights-bass.pdf"])
    );
    assert_eq!(s.extra["bandhelper"]["midi_song_number"], "12");
    assert_eq!(songs[0].warnings.len(), 2, "{:?}", songs[0].warnings);
    assert!(
        songs[0]
            .chart
            .contains("{start_of_verse}\n[G]Lanterns on the [C]water\n{end_of_verse}")
    );
}

#[test]
fn bandhelper_tsv_header_lyrics_only_and_quotes() {
    let mut header: Vec<&str> = BANDHELPER_COLUMNS.to_vec();
    header.push("Singer");
    let mut row = vec![""; 17];
    row[0] = "\"Grüße \"\"live\"\"\"";
    row[11] = "Über die Brücke\\nund zurück";
    row[16] = "Anna";
    let text = format!("{}\n{}\n", tsv_row(&header), tsv_row(&row));
    let songs = import_bandhelper_tsv(&text).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(songs.len(), 1);
    assert_eq!(songs[0].song.title, "Grüße \"live\"");
    assert_eq!(
        songs[0].song.custom.get("Singer").map(String::as_str),
        Some("Anna")
    );
    assert!(
        songs[0].chart.ends_with("\nÜber die Brücke\nund zurück\n"),
        "{}",
        songs[0].chart
    );
    assert!(chords_of(&songs[0].chart).is_empty());
}

#[test]
fn bandhelper_tsv_round_trip() {
    let mut a = Song::new("Harbour Lights");
    a.artist = "The Demo Band".into();
    a.tags = vec!["Folk".into(), "Ballad".into()];
    a.key = Some("G".into());
    a.time_signature = Some("3/4".into());
    a.tempo_bpm = Some(96.0);
    a.duration_secs = Some(210);
    a.notes = "Watch the ending\nslow down".into();
    a.custom.insert("Starting Pitch".into(), "D".into());
    a.custom.insert("Singer".into(), "Anna".into());
    a.extra.insert(
        "bandhelper".into(),
        serde_json::json!({"documents": ["lights.pdf"], "midi_song_number": "12"}),
    );
    let chart_a = export_chordpro(
        &a,
        "{start_of_verse: 1}\n[G]Lanterns on the [C]water\n{end_of_verse}\n\n{chorus}\n",
    );
    let mut b = Song::new("\"Quoted\"");
    b.custom.insert("Drummer".into(), "Ben".into());
    let chart_b = export_chordpro(&b, "Grüße\taus\nder Ferne\n");
    let tsv = export_bandhelper_tsv(&[(a.clone(), chart_a.clone()), (b.clone(), chart_b.clone())]);
    let first_line = tsv.lines().next().unwrap_or_default();
    assert!(first_line.starts_with("Title\tArtist\tTags\tKey"));
    assert!(first_line.ends_with("\tSinger\tDrummer"));
    assert_eq!(tsv.lines().count(), 3);
    let back = import_bandhelper_tsv(&tsv).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(back.len(), 2);
    let mut expect_a = a.clone();
    expect_a.id = back[0].song.id;
    assert_eq!(back[0].song, expect_a);
    assert_eq!(back[0].chart, chart_a);
    let mut expect_b = b.clone();
    expect_b.id = back[1].song.id;
    assert_eq!(back[1].song, expect_b);
    assert_eq!(back[1].chart, chart_b.replace('\t', " "));
    // export → import → export is stable
    let again = export_bandhelper_tsv(
        &back
            .iter()
            .map(|i| (i.song.clone(), i.chart.clone()))
            .collect::<Vec<_>>(),
    );
    assert_eq!(again, tsv);
}

// --- CSV -------------------------------------------------------------------

#[test]
fn csv_german_headers_semicolons_and_quotes() {
    let text = "\u{FEFF}Titel;Interpret;Tonart;BPM;Dauer;Kapo;Schlagworte;Bemerkung;Sänger\r\n\
\"Grüße; vom Hafen\";Die Demo-Band;Am;92,5;4:05;3;\"Folk, Ballade\";\"Zeile 1\r\nZeile \"\"2\"\"\";Anna\r\n\
;;;;;;;;\r\n\
Ohne Tempo;;;schnell;;;;;\r\n";
    let table = read_csv(text, None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(table.delimiter, ';');
    let mapping = guess_mapping(&table.rows[0]);
    assert!(mapping.has_header);
    assert_eq!(mapping.columns.get(&0), Some(&CsvField::Title));
    assert_eq!(mapping.columns.get(&4), Some(&CsvField::Duration));
    assert_eq!(
        mapping.columns.get(&8),
        Some(&CsvField::Custom("Sänger".into()))
    );
    let songs = import_csv(text, &mapping).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(songs.len(), 2, "the empty row is skipped");
    let s = &songs[0].song;
    assert_eq!(s.title, "Grüße; vom Hafen");
    assert_eq!(s.artist, "Die Demo-Band");
    assert_eq!(s.key.as_deref(), Some("Am"));
    assert_eq!(s.tempo_bpm, Some(92.5));
    assert_eq!(s.duration_secs, Some(245));
    assert_eq!(s.capo, Some(3));
    assert_eq!(s.tags, ["Folk", "Ballade"]);
    assert_eq!(s.notes, "Zeile 1\nZeile \"2\"");
    assert_eq!(s.custom.get("Sänger").map(String::as_str), Some("Anna"));
    assert_eq!(songs[1].warnings.len(), 1);
}

#[test]
fn csv_custom_mapping_with_chart_and_links() {
    let text = "Night Ferry,https://tabs.ultimate-guitar.com/tab/x/night-ferry-chords-1 https://example.com/v,\"G   C\nThe ferry\"\n";
    let mapping = CsvMapping {
        has_header: false,
        delimiter: Some(','),
        columns: BTreeMap::from([
            (0, CsvField::Title),
            (1, CsvField::Link),
            (2, CsvField::Chart),
            (9, CsvField::Artist),
        ]),
    };
    let songs = import_csv(text, &mapping).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(songs.len(), 1);
    assert_eq!(songs[0].song.links.len(), 2);
    assert!(
        songs[0].chart.contains("[G]The [C]ferry"),
        "{}",
        songs[0].chart
    );
    let no_title = CsvMapping::default();
    assert!(import_csv(text, &no_title).is_err());
}

#[test]
fn csv_delimiters_and_unbalanced_quotes() {
    assert_eq!(read_csv("a,b\n1,2", None).map(|t| t.delimiter), Ok(','));
    assert_eq!(read_csv("a\tb;c\td", None).map(|t| t.delimiter), Ok('\t'));
    let t =
        read_csv("title,notes\nx,\"never closed\ny,z\n", None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1][1], "never closed\ny,z\n");
    assert_eq!(t.warnings.len(), 1);
    let t = read_csv("a,b\"c\"\n", None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.rows[0], ["a", "b\"c\""]);
    let unknown = guess_mapping(&["foo", "bar"]);
    assert!(!unknown.has_header);
    assert_eq!(unknown.columns, BTreeMap::from([(0, CsvField::Title)]));
    let english = guess_mapping(&[
        "Song",
        "Artist",
        "Time Signature",
        "Length",
        "Chords",
        "URL",
        "",
    ]);
    assert_eq!(
        english.columns.values().cloned().collect::<Vec<_>>(),
        [
            CsvField::Title,
            CsvField::Artist,
            CsvField::TimeSignature,
            CsvField::Duration,
            CsvField::Chart,
            CsvField::Link,
            CsvField::Ignore
        ]
    );
}

// --- Cuebook ---------------------------------------------------------------

const CUEBOOK: &str = r#"{
  "format" : 1,
  "setlists" : [
    { "date" : "2026-11-25T10:00:00.000Z", "entries" : [
        { "id" : "6F1C9E7A-3333-4C8B-9E55-1A2B3C4D5E6F", "notes" : "Intro drums only", "songID" : "6F1C9E7A-1111-4C8B-9E55-1A2B3C4D5E6F" },
        { "id" : "6F1C9E7A-4444-4C8B-9E55-1A2B3C4D5E6F", "songID" : "6F1C9E7A-2222-4C8B-9E55-1A2B3C4D5E6F" },
        { "id" : "6F1C9E7A-5555-4C8B-9E55-1A2B3C4D5E6F", "songID" : "00000000-0000-0000-0000-000000000000" }
      ], "id" : "6F1C9E7A-9999-4C8B-9E55-1A2B3C4D5E6F", "modified" : "2026-11-25T10:00:00.000Z", "name" : "Friday", "notes" : "Bring capo" }
  ],
  "songs" : [
    { "artist" : "The Demo Band", "chordsURL" : "https:\/\/tabs.ultimate-guitar.com\/tab\/the-demo-band\/harbour-lights-chords-1",
      "id" : "6F1C9E7A-1111-4C8B-9E55-1A2B3C4D5E6F", "key" : "G", "modified" : "2026-11-25T10:00:00.000Z", "notes" : "Capo 2",
      "parts" : [
        { "id" : "6F1C9E7A-AAAA-4C8B-9E55-1A2B3C4D5E6F", "name" : "Verse", "preset" : { "name" : "Coffee House Duet", "number" : 3 } },
        { "id" : "6F1C9E7A-BBBB-4C8B-9E55-1A2B3C4D5E6F", "name" : "", "preset" : { "number" : 4 } }
      ], "tempo" : 96, "title" : "Harbour Lights" },
    { "id" : "6F1C9E7A-2222-4C8B-9E55-1A2B3C4D5E6F", "title" : "Old File", "preset" : { "number" : 1, "name" : "First" } },
    { "id" : "6F1C9E7A-2222-4C8B-9E55-1A2B3C4D5E6F", "title" : "Twin", "parts" : [ { "name" : "x", "preset" : { "number" : 0 } } ] },
    { "title" : "No id" }
  ]
}"#;

#[test]
fn cuebook_library() {
    let cb = import_cuebook(CUEBOOK.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cb.songs.len(), 3, "the song without id is skipped");
    let a = &cb.songs[0].song;
    assert_eq!(a.id.to_string(), "6f1c9e7a-1111-4c8b-9e55-1a2b3c4d5e6f");
    assert_eq!(a.title, "Harbour Lights");
    assert_eq!(a.tempo_bpm, Some(96.0));
    assert_eq!(a.notes, "Capo 2");
    assert_eq!(
        a.links[0].url,
        "https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-1"
    );
    assert_eq!(a.parts.len(), 2);
    assert_eq!(a.parts[0].name, "Verse");
    assert_eq!(a.parts[0].notes, "Preset 3 · Coffee House Duet");
    assert_eq!(a.parts[0].actions[0].device, CUEBOOK_DEVICE);
    assert_eq!(
        a.parts[0].actions[0].message,
        MidiMessage::ProgramChange {
            channel: 1,
            program: 2,
            bank_msb: None,
            bank_lsb: None
        }
    );
    assert_eq!(a.parts[1].name, "Part 2");
    assert_eq!(a.parts[1].notes, "Preset 4");
    // Cuebook 0.1: one preset per song
    let old = &cb.songs[1].song;
    assert_eq!(old.parts.len(), 1);
    assert_eq!(old.parts[0].name, "Part 1");
    assert!(matches!(
        old.parts[0].actions[0].message,
        MidiMessage::ProgramChange { program: 0, .. }
    ));
    // duplicate id and preset 0
    let twin = &cb.songs[2];
    assert_ne!(twin.song.id, old.id);
    assert!(twin.song.parts[0].actions.is_empty());
    assert_eq!(twin.warnings.len(), 2, "{:?}", twin.warnings);
    // setlist
    let list = &cb.setlists[0];
    assert_eq!(list.id.to_string(), "6f1c9e7a-9999-4c8b-9e55-1a2b3c4d5e6f");
    assert_eq!(list.date.as_deref(), Some("2026-11-25"));
    assert_eq!(list.notes, "Bring capo");
    let entries: Vec<&SongEntry> = list.song_entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].song, a.id);
    assert_eq!(entries[0].notes, "Intro drums only");
    assert!(cb.warnings.iter().any(|w| w.contains("song 4")));
    assert!(cb.warnings.iter().any(|w| w.contains("1 entries skipped")));
    // importing again yields the same ids
    let again = import_cuebook(CUEBOOK.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(again.songs[0].song.id, a.id);
    assert_eq!(again.setlists[0].id, list.id);
}

#[test]
fn cuebook_hostile_input() {
    assert_eq!(import_cuebook(b""), Err(FormatError::Empty));
    assert!(matches!(import_cuebook(b"{"), Err(FormatError::Invalid(_))));
    assert!(matches!(
        import_cuebook(b"[1,2]"),
        Err(FormatError::Invalid(_))
    ));
    let deep = format!("{{\"songs\":{}{}}}", "[".repeat(5_000), "]".repeat(5_000));
    assert!(import_cuebook(deep.as_bytes()).is_err());
    let odd = br#"{"songs":[{"id":"6F1C9E7A-1111-4C8B-9E55-1A2B3C4D5E6F","tempo":"fast"},
        {"id":"6F1C9E7A-1111-4C8B-9E55-1A2B3C4D5E6E","parts":[{"preset":{"number":-9223372036854775808}}]}],
        "setlists":[{"id":"x","entries":[1,{"songID":"nope"}]}], "format": 7}"#;
    let cb = import_cuebook(odd).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cb.songs.len(), 1);
    assert!(cb.songs[0].song.parts[0].actions.is_empty());
    assert_eq!(cb.setlists.len(), 1);
    assert_eq!(cb.warnings.len(), 4, "{:?}", cb.warnings);
    let huge = vec![b' '; MAX_INPUT_BYTES + 1];
    assert!(matches!(
        import_cuebook(&huge),
        Err(FormatError::TooLarge { .. })
    ));
}

// --- Setlist text ----------------------------------------------------------

#[test]
fn setlist_text() {
    let mut a = Song::new("Harbour Lights");
    a.artist = "The Demo Band".into();
    a.key = Some("G".into());
    a.tempo_bpm = Some(96.0);
    a.duration_secs = Some(245);
    let mut b = Song::new("Night Ferry");
    b.duration_secs = Some(200);
    let missing = SongId::new();
    let mut list = Setlist::new("Summer Fest");
    list.date = Some("2026-07-04".into());
    list.notes = "Load-in 17:00".into();
    list.sets[0].entries = vec![
        Entry::Song(SongEntry {
            key: Some("A".into()),
            notes: "Intro drums only\ncapo 2".into(),
            ..SongEntry::new(a.id)
        }),
        Entry::Break {
            label: "Change".into(),
            minutes: 15,
        },
        Entry::Song(SongEntry {
            optional: true,
            ..SongEntry::new(b.id)
        }),
    ];
    list.sets.push(Set {
        name: "Encore".into(),
        entries: vec![
            Entry::Song(SongEntry::new(missing)),
            Entry::Song(SongEntry::new(b.id)),
        ],
    });
    let songs = [a.clone(), b.clone()];
    let text = export_setlist_text(&list, |id| songs.iter().find(|s| s.id == id));
    assert_eq!(
        text,
        "Summer Fest – 2026-07-04\n\nLoad-in 17:00\n\nSet 1\n 1. Harbour Lights – The Demo Band  [A, 96 BPM, 4:05]\n    Intro drums only\n    capo 2\n    — Break: Change (15 min) —\n 2. Night Ferry  [3:20, optional]\n\nEncore\n 3. (missing song)\n 4. Night Ferry  [3:20]\n\nSet 1: 19:05\nEncore: 3:20\nTotal: 22:25 (+ 3:20 optional), 1 song without a duration\n"
    );
}

// --- Detection and import_bytes --------------------------------------------

#[test]
fn detects_formats() {
    assert_eq!(detect("x.cho", b""), Format::ChordPro);
    assert_eq!(detect("x.onsong", b""), Format::OnSong);
    assert_eq!(detect("x.CSV", b""), Format::Csv);
    assert_eq!(detect("x.tsv", b""), Format::BandHelperTsv);
    assert_eq!(detect("x.txt", b"{title: A}\n[G]la\n"), Format::ChordPro);
    assert_eq!(detect("x.txt", ONSONG.as_bytes()), Format::OnSong);
    assert_eq!(
        detect("x.txt", b"Night Ferry\n\nG  C\nla la\n"),
        Format::PlainText
    );
    assert_eq!(
        detect("Cuebook Library.json", CUEBOOK.as_bytes()),
        Format::CuebookJson
    );
    assert_eq!(
        detect("lib", b"{\"format\":1,\"songs\":[]}"),
        Format::CuebookJson
    );
    assert_eq!(detect("x.json", b"{\"other\":1}"), Format::Unknown);
    assert_eq!(
        detect(
            "song",
            b"<?xml version=\"1.0\"?><song><title>A</title></song>"
        ),
        Format::OpenSong
    );
    assert_eq!(
        detect("export.txt", b"A\tB\tC\nD\tE\tF\n"),
        Format::BandHelperTsv
    );
    assert_eq!(
        detect("list.txt", b"Title;Artist;Key\nA;B;C\n"),
        Format::Csv
    );
    assert_eq!(detect("x.bin", b"\x00\x01\x02\x03PK"), Format::Unknown);
    assert_eq!(detect("empty.txt", b"   \n"), Format::Unknown);
}

#[test]
fn import_bytes_dispatches() {
    let i = import_bytes("Songs/Dawn Road.txt", b"la la\nG  C\nla la\n")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.format, Format::PlainText);
    assert_eq!(i.songs[0].song.title, "Dawn Road");
    let i = import_bytes("x.cho", b"[G]untitled chart\n").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.songs[0].song.title, "x");
    let i =
        import_bytes("Cuebook Library.json", CUEBOOK.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((i.songs.len(), i.setlists.len()), (3, 1));
    let i = import_bytes("songs.csv", "Titel,Künstler\nA,B\n".as_bytes())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(i.songs[0].song.artist, "B");
    assert!(import_bytes("songs.csv", b"foo,bar\n1,2\n").is_err());
    assert_eq!(
        import_bytes("x.bin", b"\x00\x00\x00"),
        Err(FormatError::Unsupported)
    );
    let latin1 = import_bytes("", b"Gr\xFC\xDFe\n\nla la\n").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(latin1.songs[0].song.title, "Grüße");
}

#[test]
fn custom_sections_display_with_their_names() {
    let i = import_plain_text("Intro:\nG  C\n\nPre-Chorus\nD\nla\n\nSolo 2\nEm\n", "Demo")
        .unwrap_or_else(|e| panic!("{e}"));
    let parsed = gigcraft_chordpro::parse(&i.chart, 0);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let labels: Vec<Option<String>> = parsed
        .chart
        .blocks
        .iter()
        .map(|b| b.label.clone())
        .collect();
    assert_eq!(
        labels,
        [
            Some("Intro".to_string()),
            Some("Prechorus".to_string()),
            Some("Solo: 2".to_string())
        ],
        "{}",
        i.chart
    );
}

// --- Hostile input ---------------------------------------------------------

#[test]
fn hostile_input_never_panics() {
    let binary: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let binary_text = String::from_utf8_lossy(&binary).into_owned();
    let samples: Vec<String> = vec![
        String::new(),
        "\n\n\n".into(),
        "{".into(),
        "}{".into(),
        "[[[[".into(),
        "]]]]".into(),
        "\"".into(),
        "\"\"\"".into(),
        "\t\t\t\t".into(),
        ":".into(),
        "Verse 1:".into(),
        "(Chorus)\n(Chorus)".into(),
        "[".repeat(10_000),
        "{start_of_chorus".repeat(1_000),
        "<song><lyrics>".repeat(1_000),
        "&#xFFFFFF;&#;&".into(),
        "G\nG\nG\nG".into(),
        "Ä Ö Ü ß\n  é   ñ\n".into(),
        "\r\r\r\n\r".into(),
        "x\u{0}y".into(),
        binary_text,
        format!("{}\n", "G ".repeat(50_000)),
        format!("{}\n", "\t".repeat(100_000)),
        format!("\"{}", "a,".repeat(100_000)),
    ];
    for s in &samples {
        let _ = import_chordpro(s);
        let _ = import_onsong(s);
        let _ = import_plain_text(s, "");
        let _ = import_opensong(s);
        let _ = import_bandhelper_tsv(s);
        let _ = read_csv(s, None);
        let _ = import_csv(s, &guess_mapping(&["Title"]));
        let _ = import_cuebook(s.as_bytes());
        let _ = import_bytes("x", s.as_bytes());
        let _ = detect("x", s.as_bytes());
        let _ = ultimate_guitar::parse_playlist_html(s);
        let _ = ultimate_guitar::parse_link(s);
        let song = Song::new(s.clone());
        let _ = export_chordpro(&song, s);
        let _ = export_bandhelper_tsv(&[(song, s.clone())]);
    }
}

#[test]
fn inputs_are_bounded() {
    let big = "a".repeat(MAX_CHART_BYTES + 1);
    assert!(matches!(
        import_chordpro(&big),
        Err(FormatError::TooLarge { .. })
    ));
    assert!(matches!(
        import_onsong(&big),
        Err(FormatError::TooLarge { .. })
    ));
    assert!(matches!(
        import_plain_text(&big, ""),
        Err(FormatError::TooLarge { .. })
    ));
    let huge = "a\n".repeat(MAX_INPUT_BYTES / 2 + 1);
    assert!(matches!(
        import_bandhelper_tsv(&huge),
        Err(FormatError::TooLarge { .. })
    ));
    assert!(matches!(
        read_csv(&huge, None),
        Err(FormatError::TooLarge { .. })
    ));
    assert!(matches!(
        decode_text(huge.as_bytes()),
        Err(FormatError::TooLarge { .. })
    ));
    let many = "x\n".repeat(MAX_RECORDS + 2);
    assert_eq!(
        import_bandhelper_tsv(&many),
        Err(FormatError::TooManyRecords { limit: MAX_RECORDS })
    );
    assert_eq!(
        read_csv(&many, None).map(|t| t.rows.len()),
        Err(FormatError::TooManyRecords { limit: MAX_RECORDS })
    );
    let wide = format!("{}\n", "x;".repeat(MAX_COLUMNS + 10));
    let t = read_csv(&wide, None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.rows[0].len(), MAX_COLUMNS);
    assert_eq!(t.warnings.len(), 1);
}
