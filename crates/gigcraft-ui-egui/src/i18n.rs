//! Interface language: English source text, looked up in the catalogs under `i18n/`.
//!
//! Every string the interface shows goes through [`tr!`](crate::tr) or [`trn!`](crate::trn).
//! The English text is the key; a missing translation shows the English. The catalog format is
//! craft-i18n's: `context<TAB>English<TAB>translation`, `@plural` rows for counts. The test in
//! `tests/catalog.rs` checks that every string in the source has a German row.

use std::cell::Cell;
use std::sync::LazyLock;

pub use craft_i18n::Lang;

/// The catalogs, one per language besides English: (code, shared catalog, GigCraft's catalog).
static EMBEDDED: &craft_i18n::Embedded = &[("de", "", include_str!("../i18n/de.tsv"))];

/// Languages the interface is ready for, besides English.
const OFFERED: &[&str] = &["de"];

/// The catalogs, indexed on first use.
pub static I18N: LazyLock<craft_i18n::I18n> =
    LazyLock::new(|| craft_i18n::I18n::new(EMBEDDED, OFFERED).0);

thread_local! {
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::EN) };
}

/// The language setting: follow the system, or one language by code.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Setting {
    #[default]
    System,
    Code(String),
}

impl Setting {
    /// From the stored text (`"auto"` or a code).
    pub fn parse(text: &str) -> Self {
        match text {
            "" | "auto" => Self::System,
            code => Self::Code(code.to_string()),
        }
    }

    /// The stored text.
    pub fn store(&self) -> String {
        match self {
            Self::System => "auto".into(),
            Self::Code(c) => c.clone(),
        }
    }

    /// The language this setting means now.
    pub fn lang(&self) -> Lang {
        match self {
            Self::System => I18N.system_lang(),
            Self::Code(c) => I18N.from_code(c).unwrap_or(Lang::EN),
        }
    }
}

/// Switch the interface language (for this thread, which is egui's).
pub fn set(lang: Lang) {
    CURRENT.set(lang);
}

/// The interface language.
pub fn current() -> Lang {
    CURRENT.get()
}

/// The languages to offer in the picker: English first, then the catalogs.
pub fn languages() -> Vec<(String, &'static str)> {
    let mut out = vec![("en".to_string(), "English")];
    for lang in I18N.languages() {
        let code = I18N.code(lang).to_string();
        if code != "en" {
            out.push((code, I18N.name(lang)));
        }
    }
    out
}

/// `source` in the interface language.
pub fn t(source: &'static str) -> &'static str {
    I18N.t(current(), source)
}

/// `template` in the interface language with its `{name}` placeholders filled in.
pub fn fmt(template: &'static str, args: &[(&str, &str)]) -> String {
    craft_i18n::fmt(t(template), args)
}

/// A count in words (`{n} song` / `{n} songs`).
pub fn count(n: u64, one: &'static str, other: &'static str) -> String {
    I18N.trn(current(), n, one, other)
}

/// Text from the core (rig notices, device states) in the interface language, if the catalog
/// has it; otherwise the English as it is. The fixed texts are listed in [`CORE_TEXTS`].
pub fn dynamic(text: &str) -> String {
    I18N.get(current(), "", text)
        .map_or_else(|| text.to_string(), str::to_string)
}

/// A device state from the rig (`not connected`, `Play Acoustic ready`…).
pub fn state(text: &str) -> String {
    match text.strip_suffix(" ready") {
        Some(model) => fmt("{model} ready", &[("model", model)]),
        None => dynamic(text),
    }
}

/// Fixed English texts looked up at run time (from the core, or from tables
/// like the quick cues); the catalogs must translate them.
pub const CORE_TEXTS: &[&str] = &[
    // gigcraft-rig device states
    "not connected",
    "connecting",
    "not a Play Acoustic",
    "lost",
    "simulated",
    "ready",
    "no port",
    // gigcraft-rig notices
    "no device with this name in the rig settings",
    "only preset changes are supported for this device",
    "no MIDI port; preset not sent",
    // gigcraft-perform action labels
    "Next",
    "Previous",
    "Next song",
    "Previous song",
    "Page forward",
    "Page back",
    "Send presets again",
    "Panic",
    "Exit perform mode",
    "Play/stop backing tracks",
    // quick cues (live_ui::QUICK_CUES)
    "Last chorus",
    "Chorus again",
    "Extend solo",
    "Slower",
    "Faster",
    "Stop after this bar",
];

/// Every problem in the embedded catalogs (for the test).
pub fn problems() -> Vec<String> {
    I18N.check_all()
}

/// Look up `source` in `code` (for the test).
pub fn lookup(code: &str, source: &str) -> Option<String> {
    let lang = I18N.from_code(code)?;
    I18N.get(lang, "", source).map(str::to_string)
}

/// Look up a plural row in `code` (for the test).
pub fn lookup_plural(code: &str, one: &str, other: &str) -> Option<String> {
    let lang = I18N.from_code(code)?;
    // Plural rows are keyed by "one|other".
    let key = format!("{one}|{other}");
    I18N.get(lang, "@plural", &key).map(str::to_string)
}

/// The interface text in the current language: `tr!("Save")`, `tr!("Key {key}", key = k)`.
macro_rules! tr {
    ($s:literal) => {
        $crate::i18n::t($s)
    };
    ($s:literal, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::i18n::fmt($s, &[$((stringify!($k), &($v).to_string())),+])
    };
}

/// A count: `trn!(n, "{n} song", "{n} songs")`.
macro_rules! trn {
    ($n:expr, $one:literal, $other:literal) => {
        $crate::i18n::count(($n) as u64, $one, $other)
    };
}
