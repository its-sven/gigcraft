use serde::Serialize;

/// Description of a command, for `commands --json`, MCP tool lists and help.
#[derive(Debug, Clone, Serialize)]
pub struct CommandSpec {
    pub id: &'static str,
    pub summary: &'static str,
    pub mutates: bool,
    pub params: &'static [ParamSpec],
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamSpec {
    pub name: &'static str,
    /// JSON type: string, integer, number, boolean, object, array.
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub required: bool,
    pub doc: &'static str,
}

const fn p(name: &'static str, kind: &'static str, required: bool, doc: &'static str) -> ParamSpec {
    ParamSpec {
        name,
        kind,
        required,
        doc,
    }
}

const ID: ParamSpec = p("id", "string", true, "id of the record");

const SONG_FIELDS: &[ParamSpec] = &[
    p("title", "string", true, "song title"),
    p("artist", "string", false, "artist"),
    p("key", "string", false, "musical key, e.g. G or F#m"),
    p("tempo_bpm", "number", false, "tempo in BPM"),
    p("time_signature", "string", false, "e.g. 4/4"),
    p("duration", "string", false, "length as m:ss or seconds"),
    p("capo", "integer", false, "capo fret"),
    p("tags", "array", false, "list of tags"),
    p("notes", "string", false, "notes"),
    p(
        "parts",
        "array",
        false,
        "parts with MIDI actions: [{name, actions:[{device, type, channel, ...}]}]",
    ),
    p("links", "array", false, "links: [{url, label}]"),
    p("custom", "object", false, "custom fields as name → text"),
    p("chart", "string", false, "ChordPro chart text"),
];

/// All commands, in a stable order.
#[must_use]
pub fn specs() -> &'static [CommandSpec] {
    SPECS
}

static SPECS: &[CommandSpec] = &[
    CommandSpec {
        id: "library.info",
        summary: "Name, location, counts and load problems of the library",
        mutates: false,
        params: &[],
    },
    CommandSpec {
        id: "library.reload",
        summary: "Read the library from disk again (clears undo)",
        mutates: false,
        params: &[],
    },
    CommandSpec {
        id: "song.list",
        summary: "List songs, optionally filtered (words, \"phrases\", -exclusions)",
        mutates: false,
        params: &[p("query", "string", false, "search text")],
    },
    CommandSpec {
        id: "song.get",
        summary: "A song with its chart and validation issues",
        mutates: false,
        params: &[ID],
    },
    CommandSpec {
        id: "song.create",
        summary: "Create a song",
        mutates: true,
        params: SONG_FIELDS,
    },
    CommandSpec {
        id: "song.update",
        summary: "Change fields of a song (null clears a field) and/or its chart",
        mutates: true,
        params: &[
            ID,
            p(
                "fields",
                "object",
                false,
                "fields to change, same names as song.create",
            ),
            p("chart", "string", false, "new ChordPro chart"),
        ],
    },
    CommandSpec {
        id: "song.trash",
        summary: "Move a song to the library's trash",
        mutates: true,
        params: &[ID],
    },
    CommandSpec {
        id: "song.chart",
        summary: "The song's chart as blocks and chord/lyric segments, optionally transposed",
        mutates: false,
        params: &[ID, p("transpose", "integer", false, "semitones, −11…11")],
    },
    CommandSpec {
        id: "setlist.list",
        summary: "List setlists with date, song count and running time",
        mutates: false,
        params: &[],
    },
    CommandSpec {
        id: "setlist.get",
        summary: "A setlist with sets, resolved songs, breaks and timing",
        mutates: false,
        params: &[ID],
    },
    CommandSpec {
        id: "setlist.create",
        summary: "Create a setlist with one empty set",
        mutates: true,
        params: &[
            p("name", "string", true, "name"),
            p("date", "string", false, "gig date YYYY-MM-DD"),
            p("notes", "string", false, "notes"),
        ],
    },
    CommandSpec {
        id: "setlist.update",
        summary: "Change name, date or notes of a setlist",
        mutates: true,
        params: &[ID, p("fields", "object", true, "name, date, notes")],
    },
    CommandSpec {
        id: "setlist.trash",
        summary: "Move a setlist to the library's trash",
        mutates: true,
        params: &[ID],
    },
    CommandSpec {
        id: "setlist.add_set",
        summary: "Append a set (e.g. Encore)",
        mutates: true,
        params: &[ID, p("name", "string", false, "set name")],
    },
    CommandSpec {
        id: "setlist.rename_set",
        summary: "Rename a set",
        mutates: true,
        params: &[
            ID,
            p("set", "integer", true, "set index from 0"),
            p("name", "string", true, "new name"),
        ],
    },
    CommandSpec {
        id: "setlist.remove_set",
        summary: "Remove a set and its entries",
        mutates: true,
        params: &[ID, p("set", "integer", true, "set index from 0")],
    },
    CommandSpec {
        id: "setlist.add_song",
        summary: "Add a song to a set (same song may appear twice)",
        mutates: true,
        params: &[
            ID,
            p("song", "string", true, "song id"),
            p("set", "integer", false, "set index, default last"),
            p(
                "position",
                "integer",
                false,
                "index in the set, default end",
            ),
            p("key", "string", false, "key for this gig"),
            p("tempo_bpm", "number", false, "tempo for this gig"),
            p("notes", "string", false, "note for this gig"),
            p("optional", "boolean", false, "play only if there is time"),
        ],
    },
    CommandSpec {
        id: "setlist.add_break",
        summary: "Add a break with a duration",
        mutates: true,
        params: &[
            ID,
            p("minutes", "integer", false, "length, default 15"),
            p("label", "string", false, "label"),
            p("set", "integer", false, "set index, default last"),
            p(
                "position",
                "integer",
                false,
                "index in the set, default end",
            ),
        ],
    },
    CommandSpec {
        id: "setlist.move_entry",
        summary: "Move an entry within or between sets",
        mutates: true,
        params: &[
            ID,
            p("from_set", "integer", true, "source set"),
            p("from", "integer", true, "source index"),
            p("to_set", "integer", false, "target set, default same"),
            p("to", "integer", true, "target index"),
        ],
    },
    CommandSpec {
        id: "setlist.remove_entry",
        summary: "Remove an entry from a set",
        mutates: true,
        params: &[
            ID,
            p("set", "integer", true, "set index"),
            p("index", "integer", true, "entry index"),
        ],
    },
    CommandSpec {
        id: "edit.undo",
        summary: "Undo the last change",
        mutates: true,
        params: &[],
    },
    CommandSpec {
        id: "edit.redo",
        summary: "Redo the last undone change",
        mutates: true,
        params: &[],
    },
    CommandSpec {
        id: "commands.list",
        summary: "This list (answered by the automation layer)",
        mutates: false,
        params: &[],
    },
];
