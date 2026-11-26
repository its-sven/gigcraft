import SwiftUI

/// How GigCraft works: one page per topic.
struct HelpView: View {
    @Environment(\.dismiss) private var dismiss

    private struct Topic: Identifiable {
        let id: Int
        let title: LocalizedStringKey
        let icon: String
        let points: [LocalizedStringKey]
    }

    private let topics: [Topic] = [
        Topic(id: 0, title: "Getting started", icon: "sparkles", points: [
            "GigCraft keeps your songs and setlists in an ordinary folder – your library. There is no account and nothing is uploaded.",
            "On first start you get a demo library to look around. Library (folder icon) lets you pick your own folder, also one in iCloud Drive or Nextcloud.",
            "Add songs with + or import them (Import menu at the bottom).",
            "Make a setlist under Setlists and tap Perform (play icon) for the stage view.",
        ]),
        Topic(id: 1, title: "Songs and charts", icon: "music.note", points: [
            "Charts are written in ChordPro: chords in square brackets right before the syllable, e.g. [G]Lanterns on the [C]water.",
            "Sections: {start_of_chorus} … {end_of_chorus}; repeat the chorus with {chorus}; a remark with {comment: Solo}.",
            "− and + in the toolbar transpose the chart for viewing.",
            "Attach a PDF to show it instead of a chart; long-press an attachment to remove it.",
            "Parts (intro, verse, solo…) can send presets to your MIDI devices when they start.",
        ]),
        Topic(id: 2, title: "Setlists", icon: "list.number", points: [
            "Tap Edit to add songs, breaks and sets, drag to reorder, swipe to remove.",
            "Tap a song while editing to set its key and a note for this gig, or mark it optional.",
            "Long-press a song to start performing from there. Share sends the list as text.",
        ]),
        Topic(id: 3, title: "On stage", icon: "music.mic", points: [
            "Tap the right third of the screen for next, the left third for previous; or use the buttons at the bottom.",
            "Bluetooth page turners and keyboards work right away: arrow keys, space and page up/down.",
            "Auto-scroll (the arrow button, key S or a footswitch) moves the chart as fast as the song is long; slower and faster are in the … menu. Touch the chart to pause; it goes on from where you leave it.",
            "Next goes part by part, then to the next song; each part sends its presets.",
            "When a part has the name of a section (Chorus, or Verse for Verse: 2), it starts with its presets as the backing track reaches that section.",
            "The screen stays on while you perform. Panic sends all notes off.",
            "Connect an external display (AirPlay or HDMI): it shows the current song large – the chart or lyrics only (Screen 2 in perform mode). With section start times, the chart follows the backing track.",
            "Songs with backing tracks or a click show Play at the top; P or a footswitch also starts and stops them. Set them up in the song editor under Backing tracks and click.",
        ]),
        Topic(id: 4, title: "MIDI devices and pedals", icon: "pianokeys", points: [
            "Under Library you find the rig: the status of your devices and MIDI Learn for footswitches.",
            "The TC-Helicon Play Acoustic is switched with its own buttons, so GigCraft never changes its settings. Turn off the simulated pedal to use the real one.",
            "Learn: tap an action, then press the footswitch.",
            "MIDI clock: always at the song's tempo (delays), or only while backing tracks play, with Start and Stop (drum machines, loopers).",
        ]),
        Topic(id: 5, title: "Playing together", icon: "person.3", points: [
            "The band leader starts Live → Start live session – on a computer, iPad or iPhone – and gets an address and a QR code.",
            "Follow (antenna icon) lists the leaders on your network – or enter the address. You see the leader's song with your own chart, and your own presets go to your devices.",
            "Ready tells the leader you are ready for this song; Cue sends a short message to every screen.",
            "Without the app, band members can open the address in Safari.",
            "On a shared venue Wi-Fi, better use the band's own router or a phone hotspot – no internet needed.",
        ]),
        Topic(id: 6, title: "Import, sync and safety", icon: "arrow.triangle.2.circlepath", points: [
            "Import reads ChordPro, OnSong, OpenSong, text with chords over the lyrics, BandHelper exports, CSV, Cuebook libraries, saved web links and pages; paste takes a song, Ultimate Guitar tab links or a playlist link. You can also drag links from Safari or files onto the song list.",
            "Ultimate Guitar: in the UG app or Safari, share a tab or playlist and copy the link, then Import > Import from clipboard. Each tab becomes a song with title, artist, key, capo and the link; a playlist becomes a setlist in its order. The charts stay on Ultimate Guitar – Open on Ultimate Guitar is in the song view – or paste a chart's text to keep it in GigCraft.",
            "Use the same folder in iCloud Drive or Nextcloud on every device: changes appear within seconds, and GigCraft asks before overwriting a song changed elsewhere.",
            "Deleted songs and setlists go to the library's trash; Undo brings back every change.",
        ]),
        Topic(id: 7, title: "Language and accessibility", icon: "accessibility", points: [
            "GigCraft follows the iPhone or iPad language; you can change it for GigCraft alone in the Settings app.",
            "VoiceOver reads each chart line as a sentence with its chords after it. In perform mode, the Actions rotor offers Next and Previous.",
            "Larger text sizes in the system settings also enlarge the charts.",
        ]),
    ]

    var body: some View {
        NavigationStack {
            List(topics) { topic in
                NavigationLink {
                    List {
                        ForEach(Array(topic.points.enumerated()), id: \.offset) { _, point in
                            Text(point).padding(.vertical, 4)
                        }
                    }
                    .navigationTitle(topic.title)
                } label: {
                    Label(topic.title, systemImage: topic.icon)
                }
            }
            .navigationTitle("Help")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}
