use gigcraft_model::Song;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Lowercase without accents, for sorting and searching ("Café" → "cafe").
#[must_use]
pub fn normalize(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// A search query: every word must match; `-word` must not match;
/// `"two words"` matches as a phrase. Words are looked up in title, artist,
/// key, tags and notes.
#[derive(Debug, Default)]
pub(crate) struct Query {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl Query {
    pub(crate) fn parse(text: &str) -> Self {
        let mut q = Self::default();
        let mut rest = text;
        while let Some(start) = rest.find(|c: char| !c.is_whitespace()) {
            rest = &rest[start..];
            let negate = rest.starts_with('-') && rest.len() > 1;
            if negate {
                rest = &rest[1..];
            }
            let (word, next) = if let Some(after) = rest.strip_prefix('"') {
                match after.find('"') {
                    Some(end) => (&after[..end], &after[end + 1..]),
                    None => (after, ""),
                }
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (&rest[..end], &rest[end..])
            };
            let w = normalize(word);
            if !w.is_empty() && w != "-" {
                if negate {
                    q.exclude.push(w)
                } else {
                    q.include.push(w)
                }
            }
            rest = next;
        }
        q
    }

    pub(crate) fn matches(&self, song: &Song) -> bool {
        let hay = normalize(&format!(
            "{}\n{}\n{}\n{}\n{}",
            song.title,
            song.artist,
            song.key.as_deref().unwrap_or_default(),
            song.tags.join("\n"),
            song.notes
        ));
        self.include.iter().all(|w| hay.contains(w.as_str()))
            && !self.exclude.iter().any(|w| hay.contains(w.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_words_phrases_and_exclusions() {
        let mut s = Song::new("Café del Mar");
        s.artist = "Energy 52".into();
        s.tags = vec!["chill".into(), "encore".into()];
        let m = |q: &str| Query::parse(q).matches(&s);
        assert!(m("cafe"));
        assert!(m("CAFÉ energy"));
        assert!(m("\"del mar\""));
        assert!(!m("\"mar del\""));
        assert!(m("-rock chill"));
        assert!(!m("-encore"));
        assert!(m(""));
        assert!(m("-"));
    }
}
