# ADR-0003: Plain files as the data format

- Status: proposed

## Decision
A library is a folder: songs as ChordPro (`.cho`) plus a TOML sidecar, setlists/events/members/presets as TOML, attachments as ordinary files. Stable UUIDs inside, readable slugs as file names, unknown fields preserved.

## Why
Readable, diff-able, works with any file sync (iCloud Drive, Nextcloud, Syncthing, Git), easy import/export, no lock-in, which is a top user complaint about the existing apps.

## Consequences
- One file per record keeps conflicts small.
- An index cache (rebuildable) for fast search on large libraries.
- A schema version per file and migrations in `gigcraft-store`.
