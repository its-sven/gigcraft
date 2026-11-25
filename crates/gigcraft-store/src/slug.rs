/// A readable, portable file name for a title: lowercase ASCII letters,
/// digits and dashes, at most 60 characters. German umlauts become ae/oe/ue/ss.
#[must_use]
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars().flat_map(char::to_lowercase) {
        let mapped: &str = match c {
            'ä' => "ae",
            'ö' => "oe",
            'ü' => "ue",
            'ß' => "ss",
            'æ' => "ae",
            'ø' => "o",
            'œ' => "oe",
            'å' => "a",
            _ => "",
        };
        let pieces: Vec<char> = if mapped.is_empty() {
            let base = crate::search::normalize(&c.to_string());
            base.chars().collect()
        } else {
            mapped.chars().collect()
        };
        for p in pieces {
            if p.is_ascii_alphanumeric() {
                out.push(p);
                dash = false;
            } else if !dash && !out.is_empty() {
                out.push('-');
                dash = true;
            }
        }
        if out.len() >= 60 {
            break;
        }
    }
    out.truncate(60);
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Harbour Lights"), "harbour-lights");
        assert_eq!(slugify("  Über den Wolken!! "), "ueber-den-wolken");
        assert_eq!(slugify("Café del Mar (Live)"), "cafe-del-mar-live");
        assert_eq!(slugify("???"), "untitled");
        assert_eq!(slugify("../../etc/passwd"), "etc-passwd");
        assert_eq!(slugify(&"x".repeat(200)).len(), 60);
        assert_eq!(slugify("日本語"), "untitled");
    }
}
