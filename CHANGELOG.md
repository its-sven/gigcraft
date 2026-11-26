# Changelog

Versions follow the roadmap phases (`docs/plan/roadmap.md`). Downloads: the GitHub release of each version – desktop app for macOS, Windows and Linux, and an unsigned iOS IPA for SideStore/AltStore.

## 0.2.1

- **Backups:** once a day each library is saved as a ZIP outside the library (desktop: the app's data folder, iOS: Files → GigCraft → Backups), the last 14 are kept; "Back up now" anywhere. Restore = unzip and open the folder.
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
