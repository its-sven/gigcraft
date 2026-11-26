# Changelog

Versions follow the roadmap phases (`docs/plan/roadmap.md`). Downloads: the GitHub release of each version – desktop app for macOS, Windows and Linux, and an unsigned iOS IPA for SideStore/AltStore.

## 0.2.4

- **Scroll together:** in a live session every chart follows the leader's scrolling – on desktop, iPad/iPhone and in the browser viewer – measured by section, so it matches on any screen size and text size. Each band member can switch it off.
- **Band mark for rehearsals:** "Mark here" draws a line with your name at that place on every screen ("from here"); everyone jumps there once, "Clear mark" removes it. Late joiners get it too.
- **Links in the editor:** add, change and remove a song's links (videos, tabs, recordings) on desktop and iOS.
- **Auto-scroll in the desktop song view** for practising, with the same speed setting as the stage view.
- **Songs without a chart** that came from Ultimate Guitar show "Open on Ultimate Guitar" right where the chart would be, on stage too.
- iPhone: wrapping preset rows in the editor, two-row footer when following a leader.

## 0.2.3

- **One import for everything:** paste or drop a song, Ultimate Guitar tab links, a share message or a playlist link – or pick any file, including web links saved as `.webloc`/`.url` and pages saved from the browser. GigCraft works out what it is; "Import from clipboard" on iPad/iPhone takes a copied link in one tap. Files it cannot read get a clear message.
- **Title, artist, key, capo and tuning from Ultimate Guitar:** after a link or playlist import the app loads each new song's tab page once and fills these in (never the chart). Titles you typed stay.
- **The link stays with the song:** "Open on Ultimate Guitar" in the song view (copy and share in its menu); the song list marks songs with their link (UG on the desktop, a link icon on iOS).
- **Auto-scroll:** the chart scrolls by itself, as fast as the song is long; slower/faster in steps; key S or a learnable footswitch; touch or scroll by hand and it goes on from there. In the stage view on all devices and in the song view on iPad/iPhone.
- **iPhone layout:** the song view keeps transpose, edit, auto-scroll speed, link and trash behind one "…" menu with icons; the stage view uses two rows on narrow screens.
- **Messages:** errors read as sentences (in German, too), close with a tap and go away after a few seconds; import results show as a short note instead of a dialog.

## 0.2.2

- **Ultimate Guitar playlists as setlists:** paste the share link of a playlist (desktop and iPad/iPhone) – GigCraft loads the page once and creates a setlist in that order; songs get title, artist and the tab link (chords, tabs…), songs already in the library are reused. A playlist page saved in the browser can be imported as a file. Charts are not copied.

## 0.2.1

- **Backups:** once a day each library is saved as a ZIP outside the library (desktop: the app's data folder, iOS: Files → GigCraft → Backups), the last 14 are kept; "Back up now" anywhere. Restore = unzip and open the folder.
- **Settings to another computer:** the rig window exports rig devices, footswitches, keys, language and text size as one file and imports them.
- **iPad/iPhone with an audio interface:** every output is used (up to 8), so the click can go to the in-ears only; output pairs per track and for the click in the iOS editor.

## 0.2.0 – band on stage (Phase 2, part 1)

- **Live sessions:** a computer, iPad or iPhone leads the setlist; the band follows on the same Wi-Fi in any browser (QR code, no app) or in GigCraft with its own chart and its own pedal presets. Leaders are found by name on the network (Bonjour). Ready status and cues ("last chorus") for everyone.
- **Backing tracks and click** on desktop and iPad/iPhone: audio files per song with volume, pan, mute and output pair, click with count-in, Play/Stop (key P or a footswitch). The next song loads in the background.
- **The chart follows the track:** section start times scroll the stage view and mark the current section; parts named like a section switch – with their presets – at the right moment.
- **MIDI clock** per device: always at the song's tempo, or only while tracks play with Start and Stop.
- **Screen 2:** a large stage view for a monitor or projector (desktop window, iPad external display), chart or lyrics only.
- **Design:** own look with an amber LED accent, display readouts, status lights and an app icon; help window (F1) and tooltips on the desktop, help sheet on iOS.
- **Downloads:** CI builds the desktop app for macOS (universal), Windows and Linux and an IPA for sideloading.

## 0.1.0 – solo on stage (Phase 1)

Library, song and setlist editors, perform mode with page turners and MIDI footswitches, pedal presets (TC-Helicon Play Acoustic, any MIDI device), PDF charts, import/export (ChordPro, OnSong, OpenSong, BandHelper, CSV, Cuebook, Ultimate Guitar), folder sync with conflict detection, English and German, screen readers.
