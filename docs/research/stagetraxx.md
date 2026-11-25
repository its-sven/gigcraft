# Research: Stage Traxx 4 feature inventory

*Compiled 2026-11-25 from public sources only (official site, v4 manual, App Store listing). Paraphrased; no product text copied. Same clean-room rule as [bandhelper-and-landscape.md](bandhelper-and-landscape.md): describe what the product does, never copy code, UI assets or help texts.*

**Identity:** "Stage Traxx" by Peter Dikant (Munich) [2][36]. Current app: **Stage Traxx 4** (App Store id 6753278694). 4.0.0 on 2026-01-28, 4.1.0 on 2026-08-19, latest 4.1.12 on 2026-11-20 [2][4]. Stage Traxx 3 is no longer sold [20].

**In one line:** a backing-track player with a real multitrack mixer, audio regions, timed lyrics/chords/PDF and MIDI automation, offline, one-time purchase, Apple-only. No band management, no DMX, no video.

## 1. Audio playback

- **Tracks:** up to 32 audio tracks per song (v3: 8), CPU meter [1][2][6]. Batch add, reorder, replace, relink missing files [6].
- **Track settings from filenames:** a pattern such as `%name%_%color%_%bus%` sets name, colour, output bus, transpose flag and mute groups on import [6][35].
- **Per track:** name, colour, solo, mute, volume, pan, output bus, 4-band EQ (2 shelves + 2 parametric), "Transpose" flag (keeps drums/click out of pitch shifting), mute groups 1–6, loudness normalisation, "apply to similar tracks" across songs [7].
- **Routing:** 16 output buses → hardware outputs; per bus name, colour, volume, mute, 4-band EQ; stereo / mono-sum / mono-to-stereo; routing stored per audio interface and restored on reconnect [8][33].
- **Multi-route audio (iOS):** USB interface + headphone jack/HDMI at once; no two USB interfaces, no Bluetooth receivers [8].
- **Mixer:** Song view and Outputs view, post-fader meters, two DCA masters, additive mute groups, up to 4 AUv3 inserts per bus (iOS only) [33].
- **Click and count-in:** metronome (tap tempo; modes disabled/stopped/playing/both; visual flash and/or audio), click as its own mixer channel → in-ear bus; per-song beat anchor; play waits for the next beat; count-in N clicks; pre-roll N beats [9][11][33][35].
- **Reliability:** track preparation time 5–50 ms, continuous drift correction (4.1.8), realtime clock engine with "classic" fallback, sample-rate/buffer settings [4][8].
- **Transitions:** autoplay without gaps, crossfades (separate in/out times, automatic or manual), play-button fades, trim start/end (auto-trim of silence), fade-in/out up to 10 s, start delay [10][27][28].
- **Safety:** double-tap required for play/stop/crossfade (also from remotes), seek lock during playback, **failover tone** on a chosen bus so external failover hardware detects a dropout [1][8][9].
- **Pitch/tempo:** ±6 semitones ±50 cents (only flagged tracks), speed 50–150 % with BPM display following, chords follow pitch [6][16].
- **Audio regions:** non-overlapping sections snapped to the beat grid; modes normal, loop, skip, repeat once, repeat-until-advanced; 6 cue slots per song ("jump now" or "arm for end of region"); 12 region presets synced via iCloud; **marker import from Logic, Reaper, Cubase** (WAV/AIF/CAF markers) [11].
- **Formats:** WAV, AIF, CAF confirmed; Music-library imports transcoded to M4A; ID3 tags read (title, artist, BPM, year, lyrics). A full format list is not documented [25].

## 2. MIDI

- **MIDI file per song** in sync with audio, to one global output port; latency compensation; built-in synth fallback [4][21][30].
- **MIDI tags inside lyrics** fire on load, play, stop or at a timestamp: PC (+bank), CC, note on/off, SysEx, song select, start/stop/continue; chains allowed; MIDI Learn records timed incoming messages into tags [5][14].
- **Global send** on load/play/stop/end; **16 custom MIDI action slots**; **MIDI macros** (`$CODE`, expanded at send time, Learn mode) [21][22].
- **Sync out:** MIDI Timecode (25/30 fps, per-song offset) *or* MIDI Clock from song BPM, position on load/seek [5][21].
- **MIDI in / remote:** per-song trigger (PC or song select loads the song), most actions mappable, press/release, CC pickup mode, MIDI Learn everywhere [5][12].
- **Ports:** virtual in/out, exclusion list, USB, Bluetooth LE MIDI, RTP network MIDI, background reception option [8][21][23][24].
- **Pedals:** MIDI foot controllers; HID keyboards/page turners (foreground only); AirTurn AirDirect native (background); "insert timecode" by foot while timing lyrics [12].

## 3. Lyrics, chords, PDF, external display

- **Lyrics editor:** light markdown (3 heading levels, bold/italic/underline), `{color}`/`{font}` tags, `;` comments, search/replace, syntax highlighting [14][35].
- **Time-coded lyrics:** LRC-style `[MM:SS.xxx]`, global offset tag, several stamps per line for word highlighting, highlight position/style, tap-along recording in the editor [15].
- **Autoscroll without timestamps:** constant speed from song duration (with per-song factor), or continuous at fixed tempo with drag-to-resync [15].
- **Chords:** inline `[C]` or chord-only lines, Nashville Number System, slash chords, N.C., bar lines/repeats, transpose ±11, capo 0–7, colour themes [16][35].
- **Import:** text, ChordPro (partial), PDF, RTF, DOC/DOCX (iOS), lyrics from ID3 incl. time-coded lyrics. **Export:** text, mail, PDF [4][14][25].
- **PDF:** one per song, annotations (pen, highlighter, text, eraser, crop), fit width/height/two-up, tap zones, colour inversion, time-coded jumps to page/position, constant autoscroll; annotations shared to clients [17].
- **External display:** HDMI/AirPlay, dedicated lyrics/PDF output, several windows on macOS/Stage Manager, hide chords on the external screen, font multiplier, 90° rotation [18][19].
- **Accessibility:** VoiceOver messages, Magic Tap play/stop [4].

## 4. Lighting and video

Not offered. Lighting only indirectly via MIDI to a lighting desk [1].

## 5. Library, setlists, band features

- **Library:** unlimited songs, nested folders, keywords, colour labels, notes, key, BPM, year, capo, play count, duration [1][25][30].
- **Search:** accent-insensitive, boolean AND/OR/NOT, `-term`, `title:`/`artist:`, phrases; sort on several fields; rule-based filters [25].
- **Playlists:** regular, **smart playlists** (rules + limit), **sets** inside a playlist with count and running time (queueing a set stops at its end), folders, autoplay per playlist/song [26].
- **Queue:** auto (up to 128) or manual; add next/last/from here/whole set [27].
- **Play history:** "done" after half the duration, history → playlist, text export, 500 entries [4][35].
- **Network sessions (local Wi-Fi, no internet):** one host; clients follow song, queue, lyrics, PDFs, transport, position, regions; latency shown. *Viewer* (free, read-only) and *Controller* (password, can load songs, queue, transport, seek, regions; no editing). "Show local lyrics": a member sees their own chart and fires their own MIDI if title+artist match. Phone-as-remote works this way [13].
- **Assignable playbar:** up to 10 buttons/toggles/knobs/sliders per device [29].
- **No** calendar, gigs, finances, contacts, accounts or stage plots [1].

## 6. Import, export, sync, backup

- Import from Files, AirDrop, Mail, share sheet, Music library; batch audio (≥10 files → playlist), duplicate detection; Stage Traxx archives and v3 backups [25][31][34].
- Export songs/playlists as archives (`.st4s`), playlists as text or title/artist-matching playlist files [4][25][26].
- iCloud sync of audio, PDFs, playlists, lyrics, settings (device-specific settings stay local); merge/replace choice on first sync [32].
- Full or database-only backups, restore with/without device settings, backups to external drives; no scheduled backups [31].
- Offline-first [20].

## 7. Platforms and price

iPhone, iPad (iOS 16.6+), Mac (macOS 13.5/13.6+), visionOS. No Android/Windows [1][2][20]. Free download (6 songs, viewer-only sessions); unlock **$29.99 one-time**, includes features released within 12 months; upgrade from v3 $14.99 [1][2].

## 8. Not verified

Complete audio format list; more than one PDF per song; client limit and discovery protocol of network sessions; bar-quantised region jumps; setlist printing; Ableton Link; full list of remote actions; free-tier status of iCloud sync and restore (sources conflict).

## 9. What Stage Traxx does that BandHelper does not emphasise

Real multitrack mixer with buses, EQ, mute groups and AUv3 inserts; live re-arranging with regions and cue slots plus DAW marker import; MIDI Timecode; MIDI tags timed inside lyrics and macros; pitch shift that skips drums/click; stage-safety features (failover tone, double-tap, seek lock, count-in, drift correction); word-level timed lyrics and time-coded PDF scrolling; fully offline, no account, one-time price.

BandHelper in turn has band/business management, DMX and video, Android and web, and multi-user accounts [38][39].

**Consequence for GigCraft:** the two products cover complementary halves. The "complete alternative" is the union: BandHelper's repertoire, charts and band organisation + Stage Traxx's playback engine, regions, MIDI automation and stage safety.

## Sources

1. https://www.stagetraxx.com/
2. https://apps.apple.com/us/app/stage-traxx-4/id6753278694
3. https://stagetraxx.com/docs/v4/
4. https://stagetraxx.com/docs/v4/release-notes/
5. https://stagetraxx.com/docs/v4/midi/
6. https://stagetraxx.com/docs/v4/song-details/audio/
7. https://stagetraxx.com/docs/v4/song-details/audio/track-details/
8. https://stagetraxx.com/docs/v4/settings/audio/audio-settings/
9. https://stagetraxx.com/docs/v4/settings/audio/playback-settings/
10. https://stagetraxx.com/docs/v4/settings/audio/fade-settings/
11. https://stagetraxx.com/docs/v4/song-details/regions/
12. https://stagetraxx.com/docs/v4/remote-control/
13. https://stagetraxx.com/docs/v4/network-sessions/
14. https://stagetraxx.com/docs/v4/song-details/lyrics/
15. https://stagetraxx.com/docs/v4/song-details/lyrics/autoscroll/
16. https://stagetraxx.com/docs/v4/song-details/lyrics/chords/
17. https://stagetraxx.com/docs/v4/pdf-documents/
18. https://stagetraxx.com/docs/v4/external-screens/
19. https://stagetraxx.com/docs/v4/settings/general/external-display/
20. https://stagetraxx.com/docs/v4/faq/
21. https://stagetraxx.com/docs/v4/settings/midi/midi-settings/
22. https://stagetraxx.com/docs/v4/settings/midi/midi-macros/
23. https://stagetraxx.com/docs/v4/settings/midi/network-midi-sessions/
24. https://stagetraxx.com/docs/v4/settings/midi/bluetooth-midi-devices/
25. https://stagetraxx.com/docs/v4/library/songs/
26. https://stagetraxx.com/docs/v4/library/playlists/
27. https://stagetraxx.com/docs/v4/playback/queue/
28. https://stagetraxx.com/docs/v4/playback/
29. https://stagetraxx.com/docs/v4/settings/general/assignable-controls/
30. https://stagetraxx.com/docs/v4/song-details/
31. https://stagetraxx.com/docs/v4/backup/
32. https://stagetraxx.com/docs/v4/cloud-sync/
33. https://stagetraxx.com/docs/v4/mixer/
34. https://stagetraxx.com/docs/v4/getting-started/
35. https://stagetraxx.com/docs/v4/stage-traxx-4-manual.pdf
36. https://www.stagetraxx.com/imprint.html
38. https://www.bandhelper.com/
39. https://www.bandhelper.com/tutorials/advanced_audio.html
