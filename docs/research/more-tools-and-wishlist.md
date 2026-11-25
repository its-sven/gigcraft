# Research: further tools, open-source projects and the musicians' wishlist

*Compiled 2026-11-25 from public sources (vendor sites, App Store, vendor forums, GitHub API). Reddit, TalkBass and some forums refused automated access; those items rely on search snippets and are marked. A manual Reddit pass is still to do. Builds on [bandhelper-and-landscape.md](bandhelper-and-landscape.md) and [stagetraxx.md](stagetraxx.md).*

## A. Products

### A1. Charts, setlists, prompters

| Product | Platforms | Key points | Price | OSS |
|---|---|---|---|---|
| **MobileSheets** | Android, iOS, Windows, macOS (AS) | PDF/ChordPro/ABC library, setlists, annotations; MIDI in for song load/actions; audio player, metronome; Wi-Fi/cloud library sync keeping each player's annotations; "Connect Tablets" leader/followers (up to 7 via BT) [1][2] | one-time, one key per device | no |
| **Setlist Helper** → **Band Central** | iOS, Android, web | setlists with breaks and running time, ChordPro, transpose, autoscroll; band sync moved to Band Central (several versions per song, personal lyric versions per member, real-time collaboration) [3][4] | Band Central $14.99–99.99/yr by band size | no |
| **LivePrompter** | Windows, Android, iOS, macOS | ChordPro prompter, no editor, Dropbox required on iOS; MIDI in/out; master device drives others over Wi-Fi [6][7][8] | free/donations | no licence stated |
| **SongBook** (LinkeSOFT) | iOS, Android, Windows, Mac | ChordPro, chord diagrams for many instruments, offline; sync via own cloud/Dropbox/OneDrive; pedals, MIDI [9][10] | one-time | no |
| **SongbookPro** | Android, iOS, Windows | ChordPro/OnSong/PDF/Word import; "Groups" shared band library [11][12] | ~$7 + Groups subscription | no |
| **Ultimate Guitar Pro** | iOS, Android | playlists as setlists; shared playlists lose personal edits [41][43] | subscription | no |
| **WorshipTools Charts**, **Planning Center Music Stand** | iOS, Android, web | church-team charts and setlists, team sync; Music Stand caches only the last 10 plans offline; "Sessions" page-turn sync had an outage in Jan 2026 [23][24][26][54] | tiers | no |

### A2. Playback and live rigs

| Product | Platforms | Key points | Price |
|---|---|---|---|
| **Camelot Pro** | iPad, Mac, Windows | setlist → songs → scenes; AUv3/VST host; MIDI layers, backing tracks, MIDI clock, Ableton Link [13][14] | ~$20 iPad, ~$139 desktop, free tier with item cap |
| **AbleSet** | macOS/Windows companion to Ableton Live; browser remote | setlists from Live locators, synced lyrics/chords, spoken cues, MIDI/OSC/Stream Deck, LTC; **AbleNet** redundancy over several computers with A/B failover [16][17] | $129–269 one-time |
| **Playback** (MultiTracks) | iOS, Mac | multitracks, pads, scrolling charts; Premium: MIDI out, 32 outputs, SMPTE/LTC, clock; public crash-rate page [18][20] | free / €7.49+ per month |
| **Prime** (Loop Community) | Mac, iOS | multitracks, click, cues, pads; MIDI out and routing paywalled; no CC-only controllers [21] | free / subscriptions |
| **Gig Performer** | Win, Mac | plugin racks, setlists, song parts (max 24 PC per song), ChordPro display [28][30][31] | $169–199 |
| **MainStage** | Mac | concerts/sets/patches, Playback plug-in following markers [32][33] | $29.99 |
| **VST Live Pro 2** | Win, Mac | MIDI, audio, VSTi, video, DMX, synced lyrics [53] | €99.99 |
| **Moises** | all | AI stems, chord finder, lyric transcription; collaborative setlist for up to 7 people with personal mix/pitch [34][35] | free / subscriptions |

### A3. Band and gig management

Gigwell (booking platform, contracts, payments, $33–399/mo) [44][45]; Muzeek (booking automation) [46]; Band Mule (calendar, setlists, files, chat) [48]; BandMGT, GigSync, BandRoadie (availability/calendar, unverified) [49]; stage plots: StagePlotPro (Mac, $39.99, dated) [50], Arlomedia Stage Plot Maker [51], StageBuilder Pro (online, auto I/O list) [52].

### A4. ChordPro reference

chordpro.org reference implementation (Perl, Artistic-2.0, active) [39][40]. The format itself can be implemented clean-room.

## B. Open-source projects to build on or learn from (GitHub, 2026-11-25)

**Candidates to reuse (permissive):**

| Repo | Lang | Licence | Use for GigCraft |
|---|---|---|---|
| **koedame/chordsketch** | Rust | crates **MIT** (desktop app/playground AGPL, fonts OFL) | ChordPro R6 parser to AST, renderers (text/HTML/PDF), iReal Pro conversion, Swift/Kotlin bindings, MCP server. **Evaluate as our ChordPro engine** instead of writing our own. Young (16★), check API stability |
| **blindpassasjer/gigboy** | TypeScript | **Apache-2.0** | self-hosted songbook + gig prep (ChordPro, setlists, press kits, rehearsal). Closest idea for the server/web side; read for features and data model |
| vojtechkral/bard | Rust | Apache-2.0 | songbook compiler (PDF), inactive since 2024 |
| devmount/SongDrive | Vue | MIT | songs/setlists presenter |
| mcallegari/qlcplus | C++ | Apache-2.0 | DMX lighting (as an external partner via OSC/MIDI, not embedded) |
| fiverecords/SuperTimecodeConverter | – | MIT | LTC/MTC reference |
| bitwig/dawproject | – | MIT | DAW exchange format (marker/arrangement import) |
| adefossez/demucs, deezer/spleeter | Python | MIT | stem separation (optional AI module) |
| spotify/basic-pitch | Python | Apache-2.0 | audio → MIDI / chord hints |
| openai/whisper | Python | MIT | lyric transcription and timing |

**Learn from only (copyleft or unclear):** pandulapeter/campfire (Kotlin MP, MPL-2.0, songbook+metronome+tuner), ChordSheetJS (GPL-2.0), Chord-Provider (Swift, GPL-3.0), OpenSongApp (GPL-3.0), linux-show-player (GPL-3.0), FreeShow/Quelea (GPL-3.0), Open Setlist (no licence file), essentia (AGPL), madmom (unclear).

**Avoid:** repos that look like SEO/malware lures (e.g. `drillryte/easy-worship`, `*-Install` repos), and `StagePlotPirate` (reuses StagePlotPro graphics).

**Conclusion:** no open project combines native mobile + MIDI/presets + backing tracks + band sync. GigCraft would be the first; ChordSketch can save us the ChordPro layer.

## C. Wishlist: what musicians ask for (themes)

1. **Reliability on stage:** no crashes, fast with 600+ songs, crash recovery, backup device / failover [20][58][60][86][87][88].
2. **Band sync and collaboration:** mixed iPad/iPhone/Android bands; per-member "ready" status with an all-ready signal; parts per role (bass tab to bassist); personal edits on shared setlists; offline annotations visible to others [26][27][43][59][63].
3. **Fair pricing:** subscription fatigue (OnSong rises 2024/2025), features moved behind tiers, per-device keys [61][72][73][74].
4. **Offline completely:** not "last 10 plans", not Dropbox-dependent [3][6][23][42].
5. **All platforms:** one app on Android and iPad; Windows/Linux [18][21][32][80][81][82].
6. **MIDI:** send MIDI *before* the song starts; MC6 and other controllers; tempo ± via MIDI; 0- vs 1-based program numbers; CC-only controllers; volume pedals [21][64][65].
7. **Audio routing:** click and tracks on separate buses/outputs, mix presets per show format, fade groups, no cut transients between songs; iPad routing that just works [62][65][83][84].
8. **Prompter:** hands-free scrolling that is easy to set up (drummer-singer), current-line colour, chord-only view, visual metronome, section labels on the progress bar; voice-following scroll [58][65][95].
9. **Setlist structure:** same song twice, several sets per gig, optional songs, theatre-style GO, gig date on setlists, key/BPM per setlist (per band), printable/exportable setlists, QR code for the remote [65][68][71].
10. **No lock-in:** lossless open export, per-song files instead of one database, settings export, migration guides [4][66][76][77][78][79].
11. **AI, local:** OCR/metadata on import, chord detection exported as editable ChordPro, lyric timing against the track, stems for practice mixes [38][62][67][93].
12. **Accessibility and simplicity:** simple mode for less technical bandmates, VoiceOver, large type/high contrast, full pedal-only operation [58][59][60][91][92].

## D. Gaps nobody covers well (→ GigCraft differentiators)

1. Cross-platform, offline-first sync over the venue network without cloud.
2. An open, lossless, version-control-friendly library format.
3. One "show file" per song (chart, stems, click, MIDI, lighting cues, timecode) with per-band/per-gig variants, not tied to a DAW or a store.
4. Role-aware band views: parts per member, private/shared annotations, ready status, cue messages.
5. Affordable redundancy: hot-standby second device following via timecode/clock with A/B failover.
6. Stems → practice mixes → live mix in one place (local stem separation).
7. Local AI that fits the workflow (OCR import, chord detection to ChordPro, lyric timing, voice-following scroll).
8. Community MIDI device library (profiles for Helix, Kemper, MC6, Play Acoustic…), with confirmation that messages arrived.
9. Gig logistics in the same data model: availability/subs → lineup → stage plot, input list, monitor plan.
10. Accessibility as a first-class feature.
11. Open extension points: scripting, OSC/WebSocket remote, MCP, themes.
12. Fair model: free core, self-hosted sync, no per-device keys.

## Sources

1 https://www.zubersoft.com/mobilesheets/ · 2 https://zubersoft.com/mobilesheets/features/collaboration · 3 https://www.setlisthelper.com/ · 4 https://www.bandcentral.com/ · 6 https://www.liveprompter.com/ · 7 https://apps.apple.com/us/app/liveprompter-2/id6451310318 · 8 https://community.cantabilesoftware.com/t/liveprompter-mobile-update/8428 · 9 https://linkesoft.com/songbook/ · 10 https://apps.apple.com/us/app/songbook-chordpro/id392888837 · 11 https://songbook-pro.com/pricing · 12 https://apps.apple.com/app/id1475370014 · 13 https://apps.apple.com/us/app/camelot-pro/id1326331127 · 14 https://support.audiomodeling.com/en/camelot/where-can-i-find-a-free-vsthost · 16 https://ableset.com/ · 17 https://isotonikstudios.com/how-ableset-3-powers-olivia-deans-live-playback-rig/ · 18 https://www.multitracks.com/products/playback/ · 20 https://multitracks.com/blog/update-to-playback-version-3-2-1-available-now · 21 https://loopcommunity.com/en-us/prime · 23 https://apps.apple.com/us/app/-/id368561278 · 24 https://help.planningcenter.com/en/140421-introduction-to-music-stand.html · 26 https://status.planningcenter.com/incidents/zqb6tsvg6fpl · 27 https://help.planningcenter.com/en/138517-add-annotations-to-files.html · 28 https://gigperformer.com/ · 30 https://gigperformer.freshdesk.com/support/solutions/articles/47000221327 · 31 https://gigperformer.freshdesk.com/support/solutions/articles/47001237746 · 32 https://www.apple.com/mainstage/ · 33 https://support.apple.com/en-au/guide/mainstage/mstgb23bc9fc/mac · 34 https://moises.ai/ · 35 https://moises.ai/features/band-collaborative-setlist · 38 https://www.digitaldjtips.com/reviews/moises-stems-creator-review/ · 39 https://www.chordpro.org/ · 40 https://github.com/ChordPro/chordpro · 41 https://help.ultimate-guitar.com/en/articles/6741372 · 42 https://help.ultimate-guitar.com/en/articles/7936845 · 43 https://apps.apple.com/us/app/id357828853 · 44 https://www.gigwell.com/ · 45 https://www.capterra.co.uk/software/162663/gigwell · 46 https://www.capterra.com/p/210127/Muzeek/ · 48 https://apps.apple.com/gb/app/band-mule/id761447162 · 49 https://www.getapp.com/all-software/a/bandmgt/ · 50 https://stageplotpro.macupdate.com/ · 51 http://forum.arlomedia.com/index.php?topic=3892.0 · 52 https://alternativeto.net/software/stagebuilder-pro/about · 53 https://soundonsound.com/news/steinberg-release-vst-live-pro-2 · 54 https://www.worshiptools.com/en-us/charts/faq · 58 https://apps.apple.com/us/app/bandhelper/id552012927?see-all=reviews · 59 http://forum.arlomedia.com/index.php?topic=2112.0 · 60 http://forum.arlomedia.com/index.php?topic=1118.0 · 61 http://forum.arlomedia.com/index.php?topic=2690.0 · 62 http://forum.arlomedia.com/index.php?topic=2467.0 · 63 http://forum.arlomedia.com/index.php?topic=4099.0 · 64 http://forum.arlomedia.com/index.php?board=7.0 · 65 https://forum.ableset.app/c/feature-requests/ · 66 https://www.zubersoft.com/mobilesheets/forum/forum-3.html · 67 https://www.zubersoft.com/mobilesheets/forum/thread-15819.html · 68 https://www.zubersoft.com/mobilesheets/forum/thread-9966.html · 71 https://community.cantabilesoftware.com/t/fr-a-wishlist-of-improvements/4213 · 72 https://onsongapp.zendesk.com/hc/en-us/articles/22650142112151-What-s-New · 73 https://apps.apple.com/ie/app/onsong-2026/id1330350343 · 74 https://www.trustpilot.com/review/onsongapp.com · 76 https://onsongapp.com/docs/features/output/file-formats · 77 https://www.npmjs.com/package/onsongtxt-2-chordpro · 78 https://onsongapp.com/docs/1.999/sync · 79 https://www.dropboxforum.com/discussions/101001017/missing-files-after-syncing/673826 · 80 https://www.talkbass.com/threads/lyric-and-setlist-tablet-app-for-both-android-and-ipad.1151369/ (snippet) · 81 https://www.talkbass.com/threads/on-song-and-other-solutions-to-better-organize-and-perform-charts-setlists.1294714/ (snippet) · 82 https://strat-talk.com/threads/setlist-helper-and-android-tablets-help-needed-please.543334/ (snippet) · 83 https://forum.loopypro.com/discussion/43502 · 84 https://gearspace.com/threads/ipad-and-multi-channel-interfaces.1356415/ · 86 https://forum.ableton.com/viewtopic.php?p=1813530 · 87 https://forum.ableton.com/viewtopic.php?p=731519 · 88 https://www.soundonsound.com/techniques/using-laptops-live-music · 91 https://afb.org/aw/23/5/17937 · 92 https://apps.apple.com/us/app/all-set-setlist-maker/id1453232689 · 93 https://supermegaultragroovy.com/products/capo/features/get-chords/ · 95 https://apps.apple.com/app/promptsmart/id1526733375 · GitHub repos: https://github.com/koedame/chordsketch, https://github.com/blindpassasjer/gigboy, https://github.com/pandulapeter/campfire, https://github.com/vojtechkral/bard and the others named in section B.
