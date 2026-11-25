# Research: an open-source alternative to BandHelper

*Status: research notes for a possible future project. Compiled 2026-11-25 from public sources only (official websites, public tutorials, App Store listings, public GitHub repos). Not legal advice. Prices and features change; re-check before relying on them.*

**Clean-room rule for this document:** it describes *what* products do, based on their public documentation and store listings. No app was decompiled, no private API inspected, and no product text is copied beyond very short attributed quotes. Any future implementation should be written from this kind of functional description, not from another product's code, UI assets or help texts.

*Copied from the Cuebook repo (`docs/research/bandhelper-alternative.md`, 2026-11-25) as the starting point for GigCraft. Stage Traxx: [stagetraxx.md](stagetraxx.md); further tools and the wishlist: [more-tools-and-wishlist.md](more-tools-and-wishlist.md).*

Context at the time: Cuebook (SwiftUI, iPhone/iPad) already covers songs, setlists, a MIDI program change per song (for a TC-Helicon Play Acoustic), Ultimate Guitar links, a perform mode and iCloud Drive file sync. This document is about what could come later.

---

## 1. BandHelper feature inventory

BandHelper is made by Arlo Leach (copyright notice on the site; "BandHelper" is a registered trademark) and grew out of the older Set List Maker app [1][2][3]. Legend: **Core** = matters to a gigging musician on stage or in rehearsal; **Admin** = band-business extra.

### 1.1 Repertoire (songs, setlists, charts)

| Feature (public description) | Tier | Core/Admin |
|---|---|---|
| Songs with standard fields (artist, key, tempo, duration, starting pitch, notes, lyrics, chords) plus unlimited custom fields; color labels; full-text search; sort by any field [2] | Basic | Core |
| "Smart copies" that inherit data from an original song unless overridden; band-member ratings [2] | Basic | Core (copies) / Admin (ratings) |
| Setlists: drag-and-drop, shuffle, set breaks and pauses with countdown timers, duration and projected-finish calculation, copy parts of earlier setlists [2] | Basic | Core |
| Smart lists (saved filter + sort over songs) [2] | Basic | Core |
| Share setlists as PDF/HTML, print, one-page PDF lead sheets, embeddable website widget [2] | Basic | Core (print) / Admin (widget) |
| Lyrics and chords with ChordPro-style bracket chords, auto chord highlighting, one-button transposition plus a per-user personal transpose, one- or two-column view, section markers [2] | Basic | Core |
| Auto-scroll based on song duration; automation tracks (timed scroll, lyric highlight, start/stop audio, send presets) [2][4] | Basic | Core |
| Attached documents (PDF, JPG, DOCX, DOC, RTF, TXT; Word/RTF converted to PDF) with annotations and saved zoom positions [2] | Basic | Core |
| Attached videos [2] | Basic | Nice-to-have |

### 1.2 Stage control: MIDI, OSC, presets, pedals

| Feature | Core/Admin |
|---|---|
| **MIDI out:** bank MSB/LSB + program change per device, multiple control changes per preset, raw hex data, song select (off by default), beat clock tied to the tempo button [5] | Core |
| **Preset model:** presets attached to songs (first preset = default), "pinned" presets available everywhere, paired presets (toggle on/off), child presets triggered with a parent; send-on-song-select is opt-in, with order and delay configurable [5] | Core |
| **Ports:** CoreMIDI on iOS/macOS (class-compliant USB, network/RTP MIDI, Bluetooth LE MIDI), USB/Bluetooth on Android, a virtual port that loops back into the app [5] | Core |
| **MIDI in / app control:** start/stop, song select, program change, CC and note messages can trigger actions (next/previous song or page, start recordings, scroll, seek); channel filter; Learn mode; optional background reception on iOS [5][6] | Core |
| **Bluetooth page turners / footswitches:** treated as keyboards sending arrow keys; click / double-click / hold can map to different actions; Learn button; foreground only (except AirTurn's AirDirect mode); tip to turn off iOS Full Keyboard Access [6] | Core |
| **OSC out:** devices registered by name, IP and port on the same LAN; one int/float/string argument per message [5] | Nice-to-have |
| Standard MIDI File playback, optionally synced with audio [2] | Nice-to-have |
| DMX lighting is named on the homepage under integrations [1]; the feature list itself does not detail it [2] | Nice-to-have |

### 1.3 Audio

Backing tracks and reference recordings in many formats (mp3, m4a, wav, aif, flac, ogg, mid ...), loop sections, speed/pitch change, fade-out, in-app recording; click track with downbeat accent and subdivisions; starting-pitch button; chromatic tuner; separate outputs for tracks/click/pitch over an audio interface (iOS); sending tempo to a Soundbrenner Pulse or BeatBuddy [2]. **Core for bands with tracks, optional for a solo vocal/guitar act.**

### 1.4 Band sync (two separate mechanisms)

1. **Account sync (cloud):** all data goes through BandHelper's servers; devices need not be online at the same time; the app syncs on open, on save and on demand. Data is kept on the device for offline viewing; the tutorial advises keeping pending offline edits under roughly 100 to reduce conflict risk. File auto-download can be limited per device. There is a 90-day edit history with per-change rollback [7]. Fields can be marked *personal* (synced to your own devices only), and admins can set members to read-only or "add only" [7].
2. **Live sharing (local, on stage):** a *lead* device broadcasts either its screen or its actions (song selection, page position, lyric highlight, setlist edits, chat...) to *following* devices. iOS-to-iOS needs no router (iOS picks Bluetooth or Wi-Fi itself); Android or mixed setups need a shared Wi-Fi network, which does not need internet access. Discovery is by local network broadcast. The tutorial gives about 0.5 s latency for actions and 1–3 s for screen sharing and says it is not suited to precise timing [8]. The features page states up to 15 devices [2].

**Core:** both. For a small band, live sharing ("everyone's screen follows the leader") is the on-stage killer feature; account sync is the rehearsal/preparation feature.

### 1.5 Band administration (Plus/Pro)

| Area | Tier | Highlights [2] |
|---|---|---|
| Schedule / gigs | Plus | Event categories, repeating events, availability requests, email/SMS reminders, iCalendar feed, public event widget |
| Contacts | Plus | Categories, interaction log, follow-up reminders, linked to events |
| Finances | Pro | Income/expense categories, invoices, savings funds, per-member balance and settle-up reports |
| Checklists | Pro | Hierarchical, assignable, due dates |
| Stage plots | Pro | Drag-and-drop builder, input list, equipment list, PDF export |
| Files, practice log | Pro | Band file sharing; practice goals and time reports |
| Apple Watch | n/a | View setlists/schedule, select songs, trigger actions [2] |

All of these are **Admin** extras from a solo/duo gigging perspective (calendar is the most useful one).

### 1.6 Import / export

- **Import (website):** tab-delimited UTF-8 text files for songs (documented column order: title, artist, tags, key, time signature, tempo, duration, starting pitch, document/recording filenames, MIDI preset names, lyrics, chords, notes, MIDI song number, incoming program change, then custom fields; pipe `|` separates multiple values, `\n` encodes line breaks), MIDI presets, events and contacts [9].
- **Import (apps):** ChordPro and OnSong files (`.chordpro .chopro .cho .pro .onsong`) each create a song; other documents/recordings create a song with an attachment [9].
- **Export:** a project export file (songs, setlists, MIDI presets, but no attachments) that is meant for re-import into another BandHelper account; zip export of documents/recordings; setlists as PDF/HTML [9][3]. A CSV export or documented exchange format is **not** described in the public tutorials.

### 1.7 Platforms and pricing

- **Platforms:** iOS/iPadOS, Android, Mac, PC (web), Kindle, Apple Watch, web browser [2][10].
- **Pricing (website, USD) [11]:** no permanent free tier; 30-day trial. Price depends on tier and number of users (one subscription covers the whole band):

| Users | Basic (repertoire) | Plus (+ schedule, contacts) | Pro (everything) |
|---|---|---|---|
| Solo | $2.25/mo · $16/yr | $3/mo · $24/yr | $3.75/mo · $32/yr |
| 2–5 | $4/mo · $40/yr | $6/mo · $60/yr | $8/mo · $80/yr |
| 6–20 | $6/mo | $9/mo | $12/mo |

The pricing page notes that in-app purchases may cost more because of app-store fees [11]. In the tier comparison against Set List Maker, automatic multi-device sync and web editing are listed from Plus up [3].

---

## 2. Competitors

| Product | Platforms | MIDI | Chords / charts | Band sync | Open source / license | Price model |
|---|---|---|---|---|---|---|
| **BandHelper** [2][11] | iOS, Android, Mac, web, Watch | Out (PC/CC/raw/clock/song select) + in, OSC | ChordPro-style lyrics, PDFs, annotations | Cloud account sync + local leader/follower | Proprietary | Subscription by tier × band size |
| **Set List Maker** [3][12] | iPhone/iPad (+ Mac version) | In-app purchase ($4.99) | Lyrics, documents | Database export/import; Live Sharing IAP ($4.99) | Proprietary | $19.99 + IAPs. Listing: "no longer adding new features", recommends BandHelper; still receives fixes. Migration guide exists [3][9] |
| **OnSong** [13] | iOS, Mac, Android, Kindle, web | Premium tier ("Stage Lighting and Automation with MIDI") | Very strong: OnSong and ChordPro formats, transpose, capo | OnSong Connect / OnCue (wireless), OnSong Drive | Proprietary | Free app; Essentials $29.99/yr, Premium $59.99/yr, Group plans |
| **Setlister** ("Setlists by BandsManager") [14] | iPhone, iPad, Mac | PC + CC from the setlist | Lyrics, chords, transpose, autoscroll | Invite members, push notifications, offline + auto sync | Proprietary | Listed as free (no IAP shown at time of check) |
| **Bandfix** [15] | iPhone, iPad, visionOS, web app | Send on in-app events + remote control via MIDI | Lyrics/chords, PDF, MusicXML, Nashville numbers | Cloud sync, teams, setlist-change notifications | Proprietary | Freemium; subscriptions $1.99–$9.99/mo |
| **Setlist MIDI** [16] | iPhone, iPad | MIDI clock focus, PC/CC per song part, CC remote in | Chart images/PDFs per section | "Sync Devices" in beta | Proprietary | Paid upfront |
| **MIDI Set List** [17] | iPad, iPhone | PC + other messages per song section; pedal/MIDI next/prev | Not described | Not described | Proprietary | Free + $8.99 "Full Performance" IAP |
| **forScore** [18][19] | iPad, iPhone, Mac, visionOS | Per-score PC, song select, raw hex; send on open; Learn; incoming MIDI as shortcuts | PDF sheet music (ChordPro not mentioned in listing) | "Cue": leader drives followers' page turns and program changes; iCloud sync | Proprietary | $24.99 + optional Pro $14.99/yr |
| **Open Setlist** (ParriauxMaxime/open-setlist) [20] | Web PWA (offline) | Via a Rust companion program on a stage computer (LAN/Bluetooth) | ChordPro editor, transpose, per-instrument parts | GitHub repo or Google Drive sync, encrypted invite links; CSV import from Setlist Helper | **No LICENSE file**; GitHub detects no license (README says "MIT", but without a license file the terms are unclear, so treat the code as not reusable until that is fixed) | Free |
| **OpenSetList** (OpenSetList/opensetlist-app) [21] | Android, iOS, desktop (Kotlin Multiplatform) | Not listed | ChordPro editor, transpose, autoscroll, Bluetooth pedal | File backup/export only; imports Setlist Helper `.db` and JustChords files | MIT | Free (Portuguese-language README) |
| **OpenSongApp** (thebigg73/OpenSongTablet) [22][23] | Android (web port in progress) | Sends song MIDI messages on load; listens for note on/off (pedals) | OpenSong XML, ChordPro, OnSong | "Connect devices": host pushes songs to clients via Google Nearby | GPL-3.0 | Free |

**Takeaways**

- Nobody in the open-source space combines **native iOS + MIDI per song + local leader sync**. The open projects are web/PWA, Kotlin or Android-only.
- Proprietary competitors cluster into (a) all-in-one band suites (BandHelper, OnSong, Bandfix, Setlister) on subscriptions, and (b) narrow MIDI setlist tools (Setlist MIDI, MIDI Set List). A free, open "(b) plus good charts plus leader sync" fills a real gap.
- License hygiene: OpenSongApp is GPL-3.0 and Open Setlist currently has no license file, so neither is a source of reusable code for an Apache-2.0 App Store app. Learn from their documented *behaviour* only.

---

## 3. Interoperability

### 3.1 Song and setlist formats

| Format | What it is | Who reads/writes it | Priority |
|---|---|---|---|
| **ChordPro** (`.cho`, `.chopro`, `.chordpro`, `.pro`) | Plain text, chords in `[brackets]` before the syllable, `{directives}` such as `{title:}`, `{start_of_chorus}` [24] | BandHelper (import), OnSong, Setlist Helper (export), Open Setlist, OpenSetList, OpenSongApp | **Must** (native chart format) |
| **OnSong format** (`.onsong`) | Plain text: metadata block of `Name: value` lines first, then sections split by blank lines; bracket chords or chords-over-lyrics [25] | OnSong, BandHelper import | Should (import) |
| **OpenSong XML** | XML `<song>` root with `title`, `author`, `key`, `tempo`, `capo`, `lyrics`...; within lyrics, lines starting with `.` are chord lines [26][27] | OpenSong, OpenSongApp, OnSong | Could |
| **BandHelper tab-delimited import layout** | Documented column order (see 1.6) [9] | BandHelper website import | Should: read *and* write this layout, so users can move in both directions |
| **BandHelper project export** | Undocumented file for BandHelper-to-BandHelper transfer [9] | BandHelper | Avoid parsing; prefer the documented TSV layout and ChordPro |
| **Setlist Helper** | Official help documents ChordPro export to an `exportedChordPro` folder [28]; a forum post mentions a CSV export (format undocumented) [29] | Setlist Helper | Via ChordPro |
| **PDF** | Charts and lead sheets | Everyone | Must (view) / Should (export setlist) |

Recommendation: store songs as one ChordPro-compatible text file each, plus a small JSON/YAML sidecar for app-specific data (MIDI presets, UG link, durations), and setlists as JSON. This keeps data human-readable, diff-able and friendly to file-based sync.

### 3.2 Ultimate Guitar links

Ultimate Guitar's `apple-app-site-association` file (fetched from `https://tabs.ultimate-guitar.com/.well-known/apple-app-site-association`) lists `/tab/*` (and older `*_crd.htm`/`*_tab.htm` patterns) as universal-link paths for its iPhone, iPad and UG Pro apps [30]. Opening such a URL from Cuebook therefore opens the UG app when it is installed and falls back to Safari otherwise. `UIApplication.open(_:options:)` with `.universalLinksOnly` can tell whether an app handled the link. Storing **links only** (no scraped chord text) avoids copyright and terms-of-service issues. OpenSetList's "import charts from UG/CifraClub" [21] is a feature to avoid copying.

### 3.3 Bluetooth page turners and footswitches

Most pedals (AirTurn, iRig BlueTurn, Coda Stomp...) pair as a **Bluetooth HID keyboard** and send arrow keys or Page Up/Down [6]. In SwiftUI this means handling hardware key events (`.onKeyPress` on iOS 17+, or `UIKeyCommand`), with a "Learn" screen and click/double-click/hold mapping. Known pitfalls from BandHelper's docs: iOS Full Keyboard Access can swallow the keys, and input only works while the app is in the foreground [6]. BLE-MIDI footswitches (e.g. iRig BlueBoard) arrive through CoreMIDI instead and can share the same action-mapping layer as incoming program changes and CCs.

---

## 4. Sync options

### 4.1 Comparison

| Option | How it works | Pros | Cons |
|---|---|---|---|
| **MultipeerConnectivity** [31] | Apple framework for discovering nearby devices and sending data over infrastructure Wi-Fi, peer-to-peer Wi-Fi or Bluetooth | No router, no server, no account; simple API; good for "leader broadcasts song index" | Apple-only (no Android/web); sessions are limited to a small number of peers (8, per `kMCSessionMaximumNumberOfPeers`); needs the local-network permission prompt |
| **Bonjour + TCP/WebSocket on local Wi-Fi** (Network.framework) [32] | Leader advertises a Bonjour service; followers connect | Cross-platform (Android/desktop can join); same model BandHelper uses for mixed setups [8] | Needs a shared Wi-Fi network (a travel router or hotspot works); some routers block client-to-client traffic [8]; local-network privacy prompt and `NSBonjourServices` entries [32] |
| **Bluetooth LE (Core Bluetooth)** | Custom GATT service for small messages | Works with no Wi-Fi at all, cross-platform | Low bandwidth (fine for "song 7, page 2", poor for files); more code; flakier at range |
| **iCloud Drive via document picker** (current Cuebook) | User picks a folder in Files; app reads/writes files with security-scoped bookmarks | Works with a **free Personal Team**; provider-agnostic (iCloud Drive, Nextcloud, Dropbox... anything with a Files provider); data is plain files | No push notifications for changes; whole-file conflicts (use one file per song); sharing a band folder depends on the provider |
| **CloudKit** (incl. `CKSyncEngine`, `CKShare`) | Apple's cloud database tied to Apple IDs | Free quota, robust sync, record-level changes, sharing between Apple IDs | **Requires the paid Apple Developer Program**: Apple's capability table lists CloudKit, iCloud Documents, Push Notifications and Associated Domains for ADP/ADEP members only, not for free Apple Developer accounts [33]. Apple-only; no self-hosting |
| **WebDAV / Nextcloud** | Direct WebDAV client, or via the Nextcloud Files provider + document picker | Self-hostable, open, cross-platform | Direct WebDAV means writing a client plus auth handling; still file-level conflicts |
| **CouchDB replication** [34][35] | Each device has a local DB; the documented replication protocol syncs with a CouchDB server (PouchDB in browsers) | Proven offline-first model; self-hostable; Apache-2.0 server | No first-party Swift client: write a replicator or evaluate third-party libraries and their licenses; document-level conflicts still need app logic |
| **Small REST server** (e.g. Swift/Vapor, Go) | Per-record `updatedAt` + last-writer-wins per field; delta pulls | Simple, fully under control, easy to self-host in Docker | You design and secure it; conflict handling is basic |
| **CRDTs: Automerge / Yjs** [36][37] | Each document is a CRDT; merges are automatic; transport-agnostic sync messages | Real offline multi-writer merge; works over any transport (MultipeerConnectivity, WebSocket, files); Automerge has MIT-licensed Swift bindings [38] | Larger storage and binary formats; a learning curve; still needs a relay for remote sync |

### 4.2 Recommended layering

1. **On stage:** a *live leader protocol* separate from data sync. It carries tiny messages ("setlist X, song 7, page 2, send preset"). Start with MultipeerConnectivity (Apple-only, zero setup), and keep the message format transport-neutral so Bonjour/WebSocket can be added for Android/web later. Followers fire their *own* MIDI locally. Do not relay MIDI over the network, because of latency [8].
2. **Data at rest:** keep the current file-based approach (one file per song/setlist in a user-chosen folder). It covers iCloud Drive and Nextcloud without the paid program.
3. **Later:** if a server is wanted, either a small self-hostable REST service or Automerge documents synced through a relay. Both can import/export the same files.

---

## 5. Legal notes for a clean-room alternative

*Factual notes, not legal advice. Talk to a lawyer before commercial release.*

- **Trademark:** "BANDHELPER" is a registered US trademark (Reg. No. 5,026,778, owner Arlo B. Leach, Class 9 software) per third-party trademark databases mirroring USPTO records [39]; the BandHelper site also states the name is registered [10]. Do not use the name, logo, colors or icon style in the product name, App Store keywords or marketing. Plain, descriptive compatibility statements ("imports BandHelper's documented tab-delimited song format") are a different thing from branding, but keep them factual and logo-free. Pick a distinct name (Cuebook) and check it in the USPTO/EUIPO databases.
- **Copyright:** protects *expression* (code, icons, screenshots, help texts, distinctive UI artwork), not ideas, features or functional data formats. Do not copy tutorial wording, UI strings or graphics; write your own docs. Cloning a look closely enough to confuse users also risks App Store rejection under the copycat and intellectual-property guidelines [40].
- **Data portability:** importing files that users exported themselves from their own accounts (ChordPro, OnSong, documented TSV layouts, PDFs) is ordinary interoperability. BandHelper's terms say it "claims no ownership" of uploaded files [10]. Avoid scraping competitor services or reverse-engineering undocumented formats or private APIs.
- **Third-party content:** lyrics and chord charts are usually copyrighted by publishers. Ship no song database, store UG *links* rather than scraped tab text, and let users import their own material.
- **License and App Store:** **Apache-2.0** is permissive and includes an explicit patent grant; it is widely used in App Store apps, and Apache-2.0 code can be sold. **GPL** apps have a history of friction with App Store terms: VLC for iOS was removed in January 2011 after a GPL contributor complained [41]. Consequence: avoid GPL/LGPL dependencies (e.g. do not borrow OpenSongApp code). Do not reuse code from repos without a license file (Open Setlist at the time of writing), since "no license" means all rights reserved by default. MIT/BSD/Apache dependencies are fine with attribution (e.g. an acknowledgements screen).
- **Contributor hygiene:** use a CONTRIBUTING note asking contributors not to submit code or text derived from proprietary competitors, plus a DCO sign-off. Keep this research file as a record of public sources used.
- **Not researched:** software patents in this space.

---

## 6. Suggested phased roadmap

### MVP (where Cuebook is now)

- Songs and setlists; perform mode
- MIDI program change per song over CoreMIDI (Play Acoustic)
- Ultimate Guitar universal links
- iCloud Drive sync of plain files via the document picker (works with a free Personal Team)

### v1 (the gigging-musician core of BandHelper)

| Item | Notes |
|---|---|
| Charts in ChordPro | Viewer with transpose, capo, font size, two-column layout, auto-scroll by duration; PDF attachments |
| Richer MIDI presets | Bank MSB/LSB + PC + CC list per song, optional per-section presets, send-on-select toggle, delay between messages |
| Pedal and MIDI-in actions | Bluetooth HID page turners and incoming PC/CC/note mapped to next/prev song, page and scroll, with a Learn screen |
| Band leader sync (local) | MultipeerConnectivity leader/follower: song selection, page, setlist changes; followers send their own MIDI |
| Import | ChordPro/OnSong files; BandHelper documented TSV song layout; generic CSV with column mapping; setlist from pasted text |
| Export | ChordPro, TSV (BandHelper layout), setlist PDF |
| Stage polish | Set breaks with timers, remaining-time estimate, dark/high-contrast theme, keep screen awake |

### Later

- Self-hostable sync server (REST or Automerge relay) and Docker image; optional Nextcloud/WebDAV direct sync
- CloudKit sync as an option once a paid developer account exists (App Store release needs it anyway)
- Calendar/gigs (events with setlists, iCal export), then optional band-admin modules (contacts, simple income/expense split)
- Backing tracks / click and per-song tempo (MIDI clock)
- Bonjour/WebSocket leader protocol so Android/web clients can follow
- Android client (e.g. Kotlin) or a web/PWA client reading the same file format
- Localization (DE/EN first)

---

## Sources

1. BandHelper homepage – https://www.bandhelper.com/
2. BandHelper feature list – https://www.bandhelper.com/main/features.html
3. Set List Maker vs. BandHelper comparison – https://www.bandhelper.com/main/SLM_comparison.html
4. BandHelper tutorials index (Getting Started) – https://www.bandhelper.com/tutorials/getting_started.html
5. BandHelper: Sending MIDI or OSC – https://www.bandhelper.com/tutorials/sending_MIDI.html
6. BandHelper: App Control – https://www.bandhelper.com/tutorials/app_control.html
7. BandHelper: Synchronizing Devices – https://www.bandhelper.com/tutorials/synchronizing_devices.html
8. BandHelper: Live Sharing – https://www.bandhelper.com/tutorials/live_sharing.html
9. BandHelper: Importing Data – https://www.bandhelper.com/tutorials/importing_data.html
10. BandHelper terms of use – https://www.bandhelper.com/misc/terms.html ; App Store listing – https://apps.apple.com/us/app/bandhelper/id552012927
11. BandHelper pricing – https://www.bandhelper.com/main/pricing.html
12. Set List Maker (App Store) – https://apps.apple.com/us/app/set-list-maker/id410496349
13. OnSong pricing – https://onsongapp.com/pricing ; App Store – https://apps.apple.com/app/id1330350343
14. Setlister – https://apps.apple.com/app/setlister/id6757764995
15. Bandfix – https://apps.apple.com/app/bandfix/id6499341499
16. Setlist MIDI – https://apps.apple.com/app/id6760603790
17. MIDI Set List – https://apps.apple.com/app/midi-set-list/id1658309333
18. forScore (App Store) – https://apps.apple.com/us/app/forscore/id363738376
19. forScore MIDI features – https://forscore.co/midi-scores ; https://forscore.co/midi-buttons/
20. Open Setlist – https://github.com/ParriauxMaxime/open-setlist (GitHub API reports `license: null`, checked 2026-11-25)
21. OpenSetList app – https://github.com/OpenSetList/opensetlist-app
22. OpenSongApp source – https://github.com/thebigg73/OpenSongTablet
23. OpenSongApp manual (MIDI, connect devices) – https://sites.google.com/site/opensongtabletmusicviewer/user-manual/midi/midi-settings ; https://sites.google.com/site/opensongtabletmusicviewer/user-manual/connect-devices
24. ChordPro – https://www.chordpro.org/chordpro/chordpro-introduction/
25. OnSong file format (metadata) – https://onsongapp.com/docs/features/formats/onsong/metadata
26. OpenSong file formats – https://opensong.org/development/file-formats/
27. OnSong manual on OpenSong format – https://onsongapp.com/docs/features/formats/opensong
28. Setlist Helper import/export (iOS) – https://www.setlisthelper.com/Help/ImportExportIos
29. MobileSheets forum thread mentioning Setlist Helper CSV – https://www.zubersoft.com/mobilesheets/forum/showthread.php?tid=3441
30. Ultimate Guitar apple-app-site-association – https://tabs.ultimate-guitar.com/.well-known/apple-app-site-association
31. Apple MultipeerConnectivity – https://developer.apple.com/documentation/multipeerconnectivity
32. Apple TN3179: Understanding local network privacy – https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy
33. Apple: Supported capabilities (iOS) – https://developer.apple.com/help/account/reference/supported-capabilities-ios
34. CouchDB replication protocol – https://docs.couchdb.org/en/stable/replication/protocol.html
35. PouchDB – https://pouchdb.com/
36. Automerge – https://automerge.org/
37. Yjs – https://github.com/yjs/yjs
38. automerge-swift (MIT) – https://github.com/automerge/automerge-swift
39. BANDHELPER trademark record (Justia mirror of USPTO data) – https://trademark.justia.com/868/74/bandhelper-86874797.html
40. App Store Review Guidelines (4.1 Copycats, 5.2 Intellectual Property) – https://developer.apple.com/app-store/review/guidelines/
41. VLC removed from the App Store over GPL dispute – https://9to5mac.com/2011/01/07/vlc-for-ios-removed-from-the-app-store/ ; https://www.computerworld.com/article/1347653/gplv2-blocks-vlc-from-apple-s-app-store.html
