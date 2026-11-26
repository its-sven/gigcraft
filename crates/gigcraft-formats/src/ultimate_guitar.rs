//! Ultimate Guitar links and shared playlist pages (ported from Cuebook).
//!
//! Only links are handled, never chord text: a tab link names the song in its
//! path (`/tab/oasis/wonderwall-chords-27596` → "Wonderwall" by "Oasis"), and a
//! shared playlist page lists tab links in data it embeds. Fetching the page is
//! the caller's job; [`parse_playlist_html`] only reads the HTML. The page
//! layout belongs to Ultimate Guitar and may change.

use crate::opensong::entity;
use serde_json::{Map, Value};

/// Largest page read by [`parse_playlist_html`] (8 MiB).
pub const MAX_HTML_BYTES: usize = crate::MAX_INPUT_BYTES;
/// Most links taken from one page or text.
pub const MAX_LINKS: usize = 2_000;

const HOSTS: [&str; 3] = [
    "tabs.ultimate-guitar.com",
    "www.ultimate-guitar.com",
    "ultimate-guitar.com",
];
/// Kinds in the link's last path segment, longest first ("guitar-pro" before "pro").
const KINDS: [&str; 11] = [
    "guitar-pro",
    "official",
    "ukulele",
    "chords",
    "drums",
    "power",
    "video",
    "tabs",
    "bass",
    "tab",
    "pro",
];
/// Lists on tab pages that are not the playlist.
const UNRELATED: [&str; 13] = [
    "recommended_tabs",
    "top_tabs",
    "latest_tabs",
    "artist_top_tabs",
    "tabchartmonthly",
    "tabchartyearly",
    "versions",
    "brothers_by_type",
    "recently_viewed",
    "recentry_viewed",
    "popular_tabs",
    "hot_tabs",
    "related_tabs",
];

/// A tab on Ultimate Guitar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UgLink {
    pub url: String,
    pub title: String,
    pub artist: String,
    /// "chords", "tabs", "ukulele", "guitar-pro"…; empty if the link has none.
    pub kind: String,
}

/// A shared playlist: its name and tabs in order, without duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UgPlaylist {
    pub name: String,
    pub links: Vec<UgLink>,
}

/// Whether `url` is an http(s) link on an Ultimate Guitar host.
#[must_use]
pub fn is_ultimate_guitar(url: &str) -> bool {
    split_url(url).is_some_and(|(host, _)| HOSTS.contains(&host.as_str()))
}

/// Read a tab link (`https://tabs.ultimate-guitar.com/tab/<artist>/<song>-<kind>-<id>`).
/// Other hosts and paths give `None`.
#[must_use]
pub fn parse_link(url: &str) -> Option<UgLink> {
    let url = url.trim();
    let (host, path) = split_url(url)?;
    if !HOSTS.contains(&host.as_str()) {
        return None;
    }
    let parts: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .collect();
    if parts.len() < 3 || parts[0] != "tab" {
        return None;
    }
    let mut slug = parts[2].as_str();
    let mut kind = "";
    for k in KINDS {
        if let Some((before, id)) = slug.rsplit_once('-')
            && !id.is_empty()
            && id.chars().all(|c| c.is_ascii_digit())
            && let Some(song) = before.strip_suffix(k).and_then(|s| s.strip_suffix('-'))
        {
            kind = k;
            slug = song;
            break;
        }
    }
    let title = words(slug);
    if title.is_empty() {
        return None;
    }
    Some(UgLink {
        url: url.to_string(),
        title,
        artist: words(&parts[1]),
        kind: kind.to_string(),
    })
}

/// A link without query and fragment, lowercased: Ultimate Guitar adds
/// tracking parameters when sharing, so this is what identifies a tab.
#[must_use]
pub fn link_key(url: &str) -> String {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    url[..end].trim().to_lowercase()
}

/// The Ultimate Guitar tab links in pasted text, in order, without duplicates.
#[must_use]
pub fn links_in_text(text: &str) -> Vec<UgLink> {
    let candidates = text
        .split(|c: char| c.is_whitespace() || "<>\"'()[]{}".contains(c))
        .filter(|t| t.starts_with("http://") || t.starts_with("https://"));
    unique(candidates.filter_map(parse_link))
}

/// What a tab page says about its song (never the tab text itself).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TabDetails {
    pub title: String,
    pub artist: String,
    /// The key the tab is written in ("Em").
    pub key: Option<String>,
    pub capo: Option<u8>,
    /// "E A D G B E", "Half Step Down"…
    pub tuning: Option<String>,
    /// "Chords", "Tabs"…
    pub kind: String,
    /// The tab's own address, if the page names it.
    pub url: Option<String>,
}

/// Read a tab page's song details from the data it embeds. `None` if the page
/// has none (layout changed, not a tab page). The tab content is not read.
#[must_use]
pub fn parse_tab_html(html: &str) -> Option<TabDetails> {
    if html.len() > MAX_HTML_BYTES {
        return None;
    }
    let data = embedded_data(html)?;
    let page = data.pointer("/store/page/data")?;
    let tab = page.get("tab")?;
    let text = |v: Option<&Value>| {
        v.and_then(Value::as_str)
            .map(|s| unescape_html(s.trim()))
            .filter(|s| !s.is_empty())
    };
    let title = text(tab.get("song_name"))?;
    let meta = page.pointer("/tab_view/meta");
    let capo = meta
        .and_then(|m| m.get("capo"))
        .and_then(|c| {
            c.as_u64()
                .or_else(|| c.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .and_then(|c| u8::try_from(c).ok())
        .filter(|c| *c > 0 && *c <= 12);
    let tuning = meta.and_then(|m| m.get("tuning")).and_then(|t| {
        text(t.get("name"))
            .or_else(|| text(t.get("value")))
            .or_else(|| text(Some(t)))
    });
    Some(TabDetails {
        title,
        artist: text(tab.get("artist_name")).unwrap_or_default(),
        key: text(tab.get("tonality_name")).or_else(|| text(meta.and_then(|m| m.get("tonality")))),
        capo,
        tuning,
        kind: text(tab.get("type")).unwrap_or_default(),
        url: text(tab.get("tab_url"))
            .or_else(|| canonical_link(html))
            .filter(|u| parse_link(u).is_some()),
    })
}

/// The page's own address (`<link rel="canonical">` or `og:url`).
fn canonical_link(html: &str) -> Option<String> {
    for (marker, attr) in [
        ("rel=\"canonical\"", "href=\""),
        ("property=\"og:url\"", "content=\""),
    ] {
        let Some(at) = html.find(marker) else {
            continue;
        };
        let tag_start = html[..at].rfind('<')?;
        let tag_end = at + html[at..].find('>')?;
        let tag = &html[tag_start..tag_end];
        if let Some(i) = tag.find(attr) {
            let rest = &tag[i + attr.len()..];
            if let Some(end) = rest.find('"') {
                return Some(unescape_html(&rest[..end]));
            }
        }
    }
    None
}

/// The link in a saved web shortcut: macOS `.webloc` (a property list) or a
/// Windows `.url` file.
#[must_use]
pub fn link_in_shortcut(text: &str) -> Option<String> {
    if let Some(line) = text.lines().find(|l| l.trim_start().starts_with("URL=")) {
        return Some(line.trim_start()["URL=".len()..].trim().to_string());
    }
    let at = text.find("<key>URL</key>")?;
    let rest = &text[at..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")? + start;
    Some(unescape_html(rest[start..end].trim()))
}

/// The first Ultimate Guitar link in `text` that is not a tab – a shared
/// playlist (`https://www.ultimate-guitar.com/user/playlist/shared?h=…`) or
/// another page that lists tabs. The app loads it once and reads it with
/// [`parse_playlist_html`].
#[must_use]
pub fn page_link_in_text(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || "<>\"'()[]{}".contains(c))
        .filter(|t| t.starts_with("http://") || t.starts_with("https://"))
        .find(|t| is_ultimate_guitar(t) && parse_link(t).is_none())
        .map(str::to_string)
}

/// (lowercased host, path) of an http(s) URL.
fn split_url(url: &str) -> Option<(String, &str)> {
    let lower_scheme = url
        .get(..8)
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let rest = if lower_scheme.starts_with("https://") {
        &url[8..]
    } else if lower_scheme.starts_with("http://") {
        &url[7..]
    } else {
        return None;
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let after = &rest[end..];
    let path_end = after.find(['?', '#']).unwrap_or(after.len());
    Some((host, &after[..path_end]))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// "the-beatles" → "The Beatles".
fn words(slug: &str) -> String {
    slug.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn unique(links: impl Iterator<Item = UgLink>) -> Vec<UgLink> {
    let mut seen = std::collections::HashSet::new();
    links
        .filter(|l| seen.insert(l.url.clone()))
        .take(MAX_LINKS)
        .collect()
}

/// Read a shared playlist page. Tabs are taken from the data the page embeds
/// (`<div class="js-store" data-content="…">`): a list named as a playlist,
/// else the longest list of tabs that is not a recommendation or chart. A page
/// without embedded data yields its tab links in order.
#[must_use]
pub fn parse_playlist_html(html: &str) -> UgPlaylist {
    if html.len() > MAX_HTML_BYTES {
        return UgPlaylist::default();
    }
    let title = page_title(html);
    let Some(json) = embedded_data(html) else {
        let raw = html.replace("\\/", "/");
        let mut found = Vec::new();
        let needle = "https://tabs.ultimate-guitar.com/tab/";
        let mut rest = raw.as_str();
        while let Some(i) = rest.find(needle) {
            let tail = &rest[i..];
            let end = tail[needle.len()..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/'))
                .map_or(tail.len(), |e| e + needle.len());
            let candidate = &tail[..end];
            // only /tab/<artist>/<song>
            let segments = candidate[needle.len()..]
                .split('/')
                .filter(|s| !s.is_empty())
                .count();
            if segments >= 2 {
                let cut: Vec<&str> = candidate.splitn(7, '/').take(6).collect();
                found.push(cut.join("/"));
            }
            rest = &tail[end.max(1)..];
            if found.len() >= MAX_LINKS {
                break;
            }
        }
        return UgPlaylist {
            name: title,
            links: unique(found.iter().filter_map(|u| parse_link(u))),
        };
    };
    let mut candidates: Vec<Candidate> = Vec::new();
    collect(&json, &[], None, &mut candidates, 0);
    candidates.retain(|c| {
        !c.path
            .iter()
            .any(|p| UNRELATED.contains(&p.to_lowercase().as_str()))
    });
    let best_index = candidates
        .iter()
        .position(|c| c.path.iter().any(|p| p.to_lowercase().contains("playlist")))
        .or_else(|| {
            candidates
                .iter()
                .enumerate()
                .max_by_key(|(i, c)| (c.links.len(), std::cmp::Reverse(*i)))
                .map(|(i, _)| i)
        });
    let Some(best) = best_index.and_then(|i| candidates.into_iter().nth(i)) else {
        return UgPlaylist {
            name: title,
            links: Vec::new(),
        };
    };
    let name = ["name", "title"]
        .iter()
        .filter_map(|k| {
            best.owner
                .as_ref()
                .and_then(|o| o.get(*k))
                .and_then(Value::as_str)
        })
        .find(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| playlist_name(&json, false, 0))
        .unwrap_or(title);
    UgPlaylist {
        name,
        links: unique(best.links.into_iter()),
    }
}

struct Candidate {
    path: Vec<String>,
    links: Vec<UgLink>,
    owner: Option<Map<String, Value>>,
}

const MAX_DEPTH: usize = 64;

/// Arrays whose items are mostly tabs (`tab_url` directly or one level down).
fn collect(
    value: &Value,
    path: &[String],
    owner: Option<&Map<String, Value>>,
    out: &mut Vec<Candidate>,
    depth: usize,
) {
    if depth > MAX_DEPTH || out.len() > 1_000 {
        return;
    }
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let mut p = path.to_vec();
                p.push(k.clone());
                collect(v, &p, Some(map), out, depth + 1);
            }
        }
        Value::Array(items) => {
            let links: Vec<UgLink> = items.iter().filter_map(|i| tab_in(i, 0)).collect();
            if !links.is_empty() && links.len() * 2 >= items.len() {
                out.push(Candidate {
                    path: path.to_vec(),
                    links,
                    owner: owner.cloned(),
                });
            } else {
                for v in items {
                    collect(v, path, owner, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

fn tab_in(item: &Value, depth: usize) -> Option<UgLink> {
    let map = item.as_object()?;
    if let Some(mut link) = map
        .get("tab_url")
        .and_then(Value::as_str)
        .and_then(parse_link)
    {
        let text = |k: &str| {
            map.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        if let Some(t) = text("song_name") {
            link.title = t;
        }
        if let Some(a) = text("artist_name") {
            link.artist = a;
        }
        return Some(link);
    }
    if depth >= 4 {
        return None;
    }
    map.values()
        .filter(|v| v.is_object())
        .find_map(|v| tab_in(v, depth + 1))
}

/// A `name`/`title` in an object under a key that mentions "playlist".
fn playlist_name(value: &Value, under: bool, depth: usize) -> Option<String> {
    let map = value.as_object()?;
    if depth > MAX_DEPTH {
        return None;
    }
    if under
        && let Some(n) = map
            .get("name")
            .or_else(|| map.get("title"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    {
        return Some(n.to_string());
    }
    map.iter().find_map(|(k, v)| {
        playlist_name(v, under || k.to_lowercase().contains("playlist"), depth + 1)
    })
}

fn embedded_data(html: &str) -> Option<Value> {
    let at = html.find("class=\"js-store\"")?;
    let tag_end = at + html[at..].find('>')?;
    let attr = "data-content=\"";
    let start = at + html[at..tag_end].find(attr)? + attr.len();
    let end = start + html[start..].find('"')?;
    serde_json::from_str(&unescape_html(&html[start..end])).ok()
}

fn page_title(html: &str) -> String {
    let Some(open) = html.find("<title") else {
        return String::new();
    };
    let Some(gt) = html[open..].find('>').map(|g| open + g + 1) else {
        return String::new();
    };
    let Some(end) = html[gt..].find('<').map(|e| gt + e) else {
        return String::new();
    };
    if !html[end..].starts_with("</title") {
        return String::new();
    }
    let mut t = unescape_html(&html[gt..end]);
    for tail in [
        " @ Ultimate-Guitar.Com",
        " | Ultimate-Guitar.com",
        " | Ultimate Guitar",
        " - Ultimate Guitar",
    ] {
        if let Some(i) = t.to_lowercase().rfind(&tail.to_lowercase())
            && t.is_char_boundary(i)
        {
            t.truncate(i);
        }
    }
    t.trim().to_string()
}

/// Decode `&quot;`, `&amp;`, `&#39;`, `&#x27;`…; unknown entities stay as written.
#[must_use]
pub fn unescape_html(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let window = &tail[..tail.len().min(12)];
        let window = &window[..(0..=window.len())
            .rev()
            .find(|i| window.is_char_boundary(*i))
            .unwrap_or(0)];
        if let Some(semi) = window.find(';')
            && let Some(c) = entity(&tail[1..semi])
        {
            out.push(c);
            rest = &tail[semi + 1..];
        } else {
            out.push('&');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_links() {
        let l = parse_link(
            "https://tabs.ultimate-guitar.com/tab/demo-band/harbour-lights-chords-27596",
        );
        assert_eq!(
            l,
            Some(UgLink {
                url: "https://tabs.ultimate-guitar.com/tab/demo-band/harbour-lights-chords-27596"
                    .into(),
                title: "Harbour Lights".into(),
                artist: "Demo Band".into(),
                kind: "chords".into(),
            })
        );
        let gp = parse_link(
            "https://tabs.ultimate-guitar.com/tab/the-demo-band/night-ferry-guitar-pro-224577?app_utm_campaign=x",
        );
        assert_eq!(gp.as_ref().map(|l| l.title.as_str()), Some("Night Ferry"));
        assert_eq!(gp.as_ref().map(|l| l.kind.as_str()), Some("guitar-pro"));
        assert_eq!(gp.map(|l| l.artist), Some("The Demo Band".into()));
        let uke = parse_link("HTTPS://TABS.Ultimate-Guitar.com/tab/x/y-ukulele-1");
        assert_eq!(uke.map(|l| l.kind), Some("ukulele".into()));
        let umlaut = parse_link(
            "https://tabs.ultimate-guitar.com/tab/die-b%C3%A4nd/gr%C3%BC%C3%9Fe-chords-5",
        );
        assert_eq!(umlaut.map(|l| l.title), Some("Grüße".into()));
        assert_eq!(
            parse_link("https://tabs.ultimate-guitar.com/tab/a/no-kind").map(|l| l.title),
            Some("No Kind".into())
        );
        for bad in [
            "https://example.com/tab/a/b-chords-1",
            "https://www.ultimate-guitar.com/search.php?value=x",
            "https://notultimate-guitar.com/tab/a/b-chords-1",
            "https://ultimate-guitar.com.evil.example/tab/a/b-chords-1",
            "ftp://tabs.ultimate-guitar.com/tab/a/b-chords-1",
            "https://tabs.ultimate-guitar.com/tab/a",
            "",
            "https://",
        ] {
            assert_eq!(parse_link(bad), None, "{bad}");
        }
        assert!(is_ultimate_guitar(
            "https://user@tabs.ultimate-guitar.com:443/x"
        ));
        assert_eq!(
            link_key("https://Tabs.ultimate-guitar.com/tab/a/b-chords-1?utm=1#top"),
            "https://tabs.ultimate-guitar.com/tab/a/b-chords-1"
        );
    }

    #[test]
    fn links_in_pasted_text() {
        let text = "Ferry https://tabs.ultimate-guitar.com/tab/demo/ferry-chords-1\n\
                    (https://tabs.ultimate-guitar.com/tab/demo/lights-chords-2) and \
                    https://example.com/x https://tabs.ultimate-guitar.com/tab/demo/ferry-chords-1";
        let titles: Vec<String> = links_in_text(text).into_iter().map(|l| l.title).collect();
        assert_eq!(titles, ["Ferry", "Lights"]);
    }

    fn page(data: &str, title: &str) -> String {
        let escaped = data.replace('&', "&amp;").replace('"', "&quot;");
        format!(
            "<html><head><title>{title}</title></head><body><div class=\"js-store\" data-content=\"{escaped}\"></div></body></html>"
        )
    }

    #[test]
    fn reads_tab_details_but_not_the_tab() {
        let data = r#"{"store":{"page":{"data":{
          "tab":{"song_name":"Harbour Lights","artist_name":"The Demo Band","type":"Chords","tonality_name":"G"},
          "tab_view":{"meta":{"capo":2,"tuning":{"name":"Standard","value":"E A D G B E"}},
                      "wiki_tab":{"content":"[ch]G[/ch] secret words"}}}}}}"#;
        let d = parse_tab_html(&page(data, "x")).unwrap();
        assert_eq!(d.title, "Harbour Lights");
        assert_eq!(d.artist, "The Demo Band");
        assert_eq!(d.key.as_deref(), Some("G"));
        assert_eq!(d.capo, Some(2));
        assert_eq!(d.tuning.as_deref(), Some("Standard"));
        assert_eq!(d.kind, "Chords");
        assert_eq!(d.url, None);
        let saved = format!(
            "<link rel=\"canonical\" href=\"https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-7\">{}",
            page(data, "x")
        );
        assert_eq!(
            parse_tab_html(&saved).unwrap().url.as_deref(),
            Some("https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-7")
        );
        assert!(parse_tab_html("<html>no data</html>").is_none());
        let none = r#"{"store":{"page":{"data":{"tab":{"song_name":"X","capo":"0"},"tab_view":{"meta":{"capo":"0"}}}}}}"#;
        assert_eq!(parse_tab_html(&page(none, "x")).unwrap().capo, None);
    }

    #[test]
    fn reads_web_shortcuts() {
        let webloc = "<?xml version=\"1.0\"?><plist><dict><key>URL</key><string>https://tabs.ultimate-guitar.com/tab/a/b-chords-1?x=1&amp;y=2</string></dict></plist>";
        assert_eq!(
            link_in_shortcut(webloc).as_deref(),
            Some("https://tabs.ultimate-guitar.com/tab/a/b-chords-1?x=1&y=2")
        );
        let url = "[InternetShortcut]\r\nURL=https://www.ultimate-guitar.com/user/playlist/shared?h=Q\r\n";
        assert_eq!(
            link_in_shortcut(url).as_deref(),
            Some("https://www.ultimate-guitar.com/user/playlist/shared?h=Q")
        );
        assert_eq!(link_in_shortcut("just text"), None);
    }

    #[test]
    fn finds_the_playlist_link() {
        let text = "look: https://www.ultimate-guitar.com/user/playlist/shared?h=Ab12 and https://tabs.ultimate-guitar.com/tab/x/y-chords-1";
        assert_eq!(
            page_link_in_text(text).as_deref(),
            Some("https://www.ultimate-guitar.com/user/playlist/shared?h=Ab12")
        );
        assert_eq!(
            page_link_in_text("https://tabs.ultimate-guitar.com/tab/x/y-chords-1"),
            None
        );
        assert_eq!(page_link_in_text("https://example.com/playlist"), None);
    }

    #[test]
    fn playlist_page_ignores_recommendations() {
        let data = r#"{"store":{"page":{"data":{
          "recommended_tabs":[{"song_name":"R1","artist_name":"X","tab_url":"https://tabs.ultimate-guitar.com/tab/x/r1-chords-1"},
                              {"song_name":"R2","artist_name":"X","tab_url":"https://tabs.ultimate-guitar.com/tab/x/r2-chords-2"},
                              {"song_name":"R3","artist_name":"X","tab_url":"https://tabs.ultimate-guitar.com/tab/x/r3-chords-3"}],
          "playlist":{"name":"Friday & Saturday","items":[
            {"id":1,"tab":{"song_name":"Harbour Lights","artist_name":"The Demo Band","tab_url":"https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-10"}},
            {"id":2,"tab":{"song_name":"Night Ferry","artist_name":"Anna Beispiel","tab_url":"https://tabs.ultimate-guitar.com/tab/anna-beispiel/night-ferry-chords-11"}}
          ]}}}}}"#;
        let p = parse_playlist_html(&page(data, "My Gig @ Ultimate-Guitar.Com"));
        assert_eq!(p.name, "Friday & Saturday");
        let t: Vec<&str> = p.links.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(t, ["Harbour Lights", "Night Ferry"]);
        assert_eq!(p.links[1].artist, "Anna Beispiel");
    }

    #[test]
    fn playlist_without_name_uses_page_title() {
        let data =
            r#"{"a":{"list":[{"tab_url":"https://tabs.ultimate-guitar.com/tab/x/one-chords-1"}]}}"#;
        let p = parse_playlist_html(&page(data, "Wedding set | Ultimate Guitar"));
        assert_eq!(p.name, "Wedding set");
        assert_eq!(p.links.len(), 1);
    }

    #[test]
    fn page_without_data_takes_links_in_order() {
        let html = r#"<title>Set @ Ultimate-Guitar.Com</title>
            <a href="https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-17427">x</a>
            <script>{"u":"https:\/\/tabs.ultimate-guitar.com\/tab\/demo\/night-ferry-chords-27596"}</script>
            <a href="https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-17427">again</a>"#;
        let p = parse_playlist_html(html);
        assert_eq!(p.name, "Set");
        let t: Vec<&str> = p.links.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(t, ["Harbour Lights", "Night Ferry"]);
    }

    #[test]
    fn hostile_pages() {
        for html in [
            "",
            "<title>",
            "<title>x</titl",
            "class=\"js-store\" data-content=\"{",
            "<div class=\"js-store\" data-content=\"&#xFFFFFFFF;&#99999999999;&\">",
            "&&&&;;;&#;&#x;",
        ] {
            let _ = parse_playlist_html(html);
        }
        let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(parse_playlist_html(&page(&deep, "Deep")).links.is_empty());
        let huge = "a".repeat(MAX_HTML_BYTES + 1);
        assert_eq!(parse_playlist_html(&huge), UgPlaylist::default());
        let many: String = (0..5_000)
            .map(|i| format!(" https://tabs.ultimate-guitar.com/tab/a/s{i}-chords-{i}"))
            .collect();
        assert_eq!(parse_playlist_html(&many).links.len(), MAX_LINKS);
    }

    #[test]
    fn html_entities() {
        assert_eq!(
            unescape_html("&quot;a&quot; &amp; &#039;b&#x27; &lt;3 &unknown; &"),
            "\"a\" & 'b' <3 &unknown; &"
        );
        assert_eq!(unescape_html("Grüße &amp; ä"), "Grüße & ä");
    }
}
