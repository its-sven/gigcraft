//! Every interface string has a German translation, and the catalogs are well formed.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a failure is a test failure"
)]

use gigcraft_ui_egui::i18n;
use std::path::Path;

/// The string literal starting at `text[0] == '"'`, unescaped, and the rest after it.
fn literal(text: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = text.strip_prefix('"')?.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &text[i + 2..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// (`tr!` strings, `trn!` pairs) in the crate's source.
fn strings() -> (Vec<String>, Vec<(String, String)>) {
    let mut singles = Vec::new();
    let mut plurals = Vec::new();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(src)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("tr!(") {
            let after = rest[at + 4..].trim_start();
            if !after.starts_with('"') {
                rest = after;
                continue;
            }
            let (s, after) = literal(after).unwrap();
            // Skip the macro's own documentation examples.
            if !path.ends_with("i18n.rs") {
                singles.push(s);
            }
            rest = after;
        }
        let mut rest = text.as_str();
        while let Some(at) = rest.find("trn!(") {
            let after = &rest[at + 5..];
            let Some(q) = after.find('"') else { break };
            let (one, after) = literal(&after[q..]).unwrap();
            let q = after.find('"').unwrap();
            let (other, after) = literal(&after[q..]).unwrap();
            if !path.ends_with("i18n.rs") {
                plurals.push((one, other));
            }
            rest = after;
        }
    }
    (singles, plurals)
}

#[test]
fn catalogs_have_no_problems() {
    assert_eq!(i18n::problems(), Vec::<String>::new());
}

#[test]
fn every_string_is_translated_to_german() {
    let (singles, plurals) = strings();
    assert!(singles.len() > 100, "found only {} strings", singles.len());
    let mut missing: Vec<String> = singles
        .iter()
        .map(String::as_str)
        .chain(i18n::CORE_TEXTS.iter().copied())
        .chain(["{model} ready"])
        .filter(|s| i18n::lookup("de", s).is_none())
        .map(|s| format!("\t{}\t", s.replace('\n', "\\n")))
        .collect();
    missing.extend(
        plurals
            .iter()
            .filter(|(one, other)| i18n::lookup_plural("de", one, other).is_none())
            .map(|(one, other)| format!("@plural\t{one}|{other}\t")),
    );
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "missing German rows:\n{}",
        missing.join("\n")
    );
}

#[test]
fn german_is_offered() {
    let langs = i18n::languages();
    assert_eq!(langs.first().map(|l| l.0.as_str()), Some("en"));
    assert!(
        langs
            .iter()
            .any(|(code, name)| code == "de" && *name == "Deutsch")
    );
}

#[test]
fn lookups_fall_back_to_english() {
    i18n::set(i18n::Lang::EN);
    assert_eq!(i18n::t("Save"), "Save");
    assert_eq!(
        i18n::dynamic("something the catalog does not know"),
        "something the catalog does not know"
    );
    i18n::set(i18n::I18N.from_code("de").unwrap());
    assert_eq!(i18n::t("Save"), "Speichern");
    assert_eq!(i18n::state("Play Acoustic ready"), "Play Acoustic bereit");
    assert_eq!(i18n::count(3, "{n} song", "{n} songs"), "3 Songs");
    assert_eq!(i18n::count(1, "{n} song", "{n} songs"), "1 Song");
    assert_eq!(
        i18n::t("a string nobody translated"),
        "a string nobody translated"
    );
}
