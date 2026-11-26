//! The help window (F1 or "Help" in the top bar): one page per topic, the
//! current keyboard and page-turner bindings, and how to get started.

use crate::GigCraftApp;
use gigcraft_perform::Gesture;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Topic {
    #[default]
    Start,
    Songs,
    Setlists,
    Perform,
    Rig,
    Live,
    Import,
    Keys,
    Access,
}

impl Topic {
    const ALL: [Topic; 9] = [
        Topic::Start,
        Topic::Songs,
        Topic::Setlists,
        Topic::Perform,
        Topic::Rig,
        Topic::Live,
        Topic::Import,
        Topic::Keys,
        Topic::Access,
    ];

    fn title(self) -> &'static str {
        match self {
            Topic::Start => tr!("Getting started"),
            Topic::Songs => tr!("Songs and charts"),
            Topic::Setlists => tr!("Setlists"),
            Topic::Perform => tr!("On stage"),
            Topic::Rig => tr!("MIDI devices and pedals"),
            Topic::Live => tr!("Playing together"),
            Topic::Import => tr!("Import, export, sync"),
            Topic::Keys => tr!("Keyboard shortcuts"),
            Topic::Access => tr!("Language and accessibility"),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct HelpUi {
    pub(crate) open: bool,
    topic: Topic,
}

impl HelpUi {
    /// Open the help at a topic.
    pub(crate) fn show(&mut self, topic: Topic) {
        self.open = true;
        self.topic = topic;
    }
}

fn para(ui: &mut egui::Ui, text: &str) {
    ui.label(text);
    ui.add_space(6.0);
}

fn steps(ui: &mut egui::Ui, items: &[&str]) {
    for (i, item) in items.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{}.", i + 1)).strong());
            ui.label(*item);
        });
    }
    ui.add_space(6.0);
}

fn bullets(ui: &mut egui::Ui, items: &[&str]) {
    for item in items {
        ui.horizontal_wrapped(|ui| {
            ui.label("•");
            ui.label(*item);
        });
    }
    ui.add_space(6.0);
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).strong().size(16.0))
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Heading, true, text));
}

fn code(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).monospace());
}

fn keys_table(app: &GigCraftApp, ui: &mut egui::Ui) {
    egui::Grid::new("help-keys")
        .num_columns(2)
        .spacing([24.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            for (keys, what) in [
                ("Cmd/Ctrl+Z", tr!("Undo")),
                ("Shift+Cmd/Ctrl+Z", tr!("Redo")),
                ("Cmd/Ctrl+S", tr!("Save the song in the editor")),
                ("F1", tr!("This help")),
            ] {
                ui.label(egui::RichText::new(keys).monospace());
                ui.label(what);
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    heading(ui, tr!("In perform mode"));
    egui::Grid::new("help-perform-keys")
        .num_columns(2)
        .spacing([24.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            for b in &app.keymap.bindings {
                let key = match b.gesture {
                    Gesture::Press => b.key.clone(),
                    Gesture::DoublePress => tr!("{key} twice", key = b.key),
                    Gesture::Hold => tr!("hold {key}", key = b.key),
                };
                ui.label(egui::RichText::new(key).monospace());
                ui.label(crate::i18n::t(b.action.label()));
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    para(
        ui,
        tr!(
            "Bluetooth page turners act as keyboards and work out of the box. MIDI footswitches are set up under Rig… > Learn."
        ),
    );
}

fn page(app: &mut GigCraftApp, ui: &mut egui::Ui, topic: Topic) {
    ui.heading(topic.title());
    ui.add_space(6.0);
    match topic {
        Topic::Start => {
            para(
                ui,
                tr!(
                    "GigCraft keeps your songs and setlists in an ordinary folder on your computer – your library. There is no account and nothing is uploaded."
                ),
            );
            steps(
                ui,
                &[
                    tr!(
                        "Look around: Library > New demo library… creates a library with example songs and a setlist."
                    ),
                    tr!(
                        "Or open your own folder with Library > Open library…. An empty folder becomes a new library."
                    ),
                    tr!("Add songs with New song, Import file… or Paste…."),
                    tr!("Make a setlist under Setlists and press Perform for the stage view."),
                ],
            );
            para(
                ui,
                tr!("Tip: rest the mouse on any button to see what it does."),
            );
            if app.engine.is_none() {
                ui.add_space(6.0);
                crate::welcome_buttons(app, ui);
            }
        }
        Topic::Songs => {
            para(
                ui,
                tr!(
                    "A song has its details (key, tempo, time signature, duration, capo, tags, notes), a chart, attachments and parts."
                ),
            );
            heading(ui, tr!("Charts in ChordPro"));
            para(
                ui,
                tr!("Write chords in square brackets right before the syllable they belong to:"),
            );
            code(
                ui,
                "[G]Lanterns on the [C]water, [G]ropes along the [D]pier",
            );
            ui.add_space(6.0);
            bullets(
                ui,
                &[
                    tr!(
                        "Sections: {start_of_chorus} … {end_of_chorus}, {start_of_verse: 2} … {end_of_verse}."
                    ),
                    tr!(
                        "Repeat the chorus with {chorus}; a remark with {comment: Solo over the verse}."
                    ),
                    tr!("The editor shows the result next to the text while you type."),
                ],
            );
            heading(ui, tr!("Key and transposing"));
            para(
                ui,
                tr!(
                    "− and + above the chart transpose it for viewing. A setlist can give a song another key for one gig; the chart follows on stage."
                ),
            );
            heading(ui, tr!("Attachments"));
            para(
                ui,
                tr!(
                    "Attach file… copies a PDF, image or recording into the library. Right-click an attachment to remove it."
                ),
            );
            heading(ui, tr!("Parts and presets"));
            para(
                ui,
                tr!(
                    "Parts (intro, verse, solo…) can each send presets to your MIDI devices when they start. Add them in the song editor under Parts and presets."
                ),
            );
        }
        Topic::Setlists => {
            bullets(
                ui,
                &[
                    tr!(
                        "A setlist has one or more sets with songs and breaks. Press Edit to change it, Done when finished."
                    ),
                    tr!(
                        "For each gig you can set a different key, a note, and mark a song as optional. The song itself stays unchanged."
                    ),
                    tr!("The same song can appear twice, e.g. again as an encore."),
                    tr!("Copy as text gives a list for mail, chat or printing."),
                    tr!("Right-click a song to start performing from there."),
                ],
            );
        }
        Topic::Perform => {
            bullets(
                ui,
                &[
                    tr!(
                        "Perform opens the stage view: big type, the gig note, the chart in the gig key and what comes next."
                    ),
                    tr!(
                        "Next and Previous go part by part, then to the next song. Each part sends its presets."
                    ),
                    tr!("Breaks show a countdown."),
                    tr!(
                        "With section start times (song editor), the chart follows the backing track and marks the current section."
                    ),
                    tr!(
                        "When a part has the name of a section (Chorus, or Verse for Verse: 2), it starts with its presets as the backing track reaches that section."
                    ),
                    tr!(
                        "Screen 2 opens a second window for a monitor or projector – the chart for the band or lyrics only for singers and the audience."
                    ),
                    tr!(
                        "Songs with backing tracks or a click show Play next to the setlist name; the files of the next song load in the background."
                    ),
                    tr!("PANIC sends all notes off to every device."),
                    tr!("The Text size slider in the top bar also sets the stage text size."),
                ],
            );
            keys_table(app, ui);
        }
        Topic::Rig => {
            bullets(
                ui,
                &[
                    tr!(
                        "Songs name a device in their presets, e.g. “Play Acoustic”. Under Rig… you tell GigCraft which MIDI port that device is on – once per computer."
                    ),
                    tr!(
                        "The TC-Helicon Play Acoustic is switched by stepping with its own buttons, so GigCraft never changes its settings. Turn off Simulated to use the real pedal."
                    ),
                    tr!(
                        "MIDI clock per device: always at the song's tempo (delays, tremolos), or only while backing tracks play, with Start and Stop (drum machines, loopers)."
                    ),
                    tr!("Use Test: program 0 and program 1 to check the connection."),
                    tr!("Footswitches: click Learn next to an action, then press the switch."),
                    tr!(
                        "Audio output: pick the device for backing tracks and the click, and test it with Test click. With an audio interface, send the click to its own outputs for in-ears."
                    ),
                    tr!(
                        "Backing tracks: attach audio files to a song, then add them as tracks in the song editor under Backing tracks and click."
                    ),
                ],
            );
        }
        Topic::Live => {
            para(
                ui,
                tr!("One device leads the show; everyone else follows on the same Wi-Fi."),
            );
            steps(
                ui,
                &[
                    tr!("Live… > Start live session shows an address and a QR code."),
                    tr!(
                        "Band members open the address in a browser on their phone or tablet – no app needed – and see the current song with chords in the gig key."
                    ),
                    tr!("Start Perform as usual. Every move reaches the band at once."),
                ],
            );
            bullets(
                ui,
                &[
                    tr!(
                        "Another GigCraft can join the address under Live…: it shows its own chart and sends its own presets to its own devices."
                    ),
                    tr!(
                        "Ready tells the leader you are ready for this song. Cues like “Last chorus” appear on every screen."
                    ),
                    tr!(
                        "Nothing leaves your network. Anyone on the network can follow, but only the leader moves the show."
                    ),
                    tr!(
                        "On a shared venue Wi-Fi, better use the band's own router or a phone hotspot – no internet needed."
                    ),
                ],
            );
        }
        Topic::Import => {
            bullets(
                ui,
                &[
                    tr!(
                        "Import file… reads ChordPro, OnSong, OpenSong, text with chords over the lyrics, BandHelper exports (TSV), CSV and Cuebook libraries."
                    ),
                    tr!(
                        "Paste… takes a song or Ultimate Guitar links; for links, GigCraft stores the link, not the chart."
                    ),
                    tr!(
                        "Copy as text in a setlist, and the command line, export songs and setlists."
                    ),
                ],
            );
            heading(ui, tr!("Using several devices"));
            para(
                ui,
                tr!(
                    "Keep the library in iCloud Drive, Nextcloud or Syncthing and open the same folder on every device. Changes appear within a few seconds. If two devices change the same song, GigCraft asks before overwriting."
                ),
            );
            heading(ui, tr!("Nothing gets lost"));
            para(
                ui,
                tr!(
                    "Deleted songs and setlists go to the library's trash. Undo brings back every change."
                ),
            );
        }
        Topic::Keys => keys_table(app, ui),
        Topic::Access => {
            bullets(
                ui,
                &[
                    tr!(
                        "Choose the language at the top right; System language follows your computer."
                    ),
                    tr!(
                        "Screen readers (VoiceOver, Narrator, Orca) can read and operate every control."
                    ),
                    tr!("The Text size slider scales charts and the stage view."),
                    tr!("Everything works with the keyboard: Tab moves between controls."),
                ],
            );
        }
    }
}

/// The help window.
pub(crate) fn window(app: &mut GigCraftApp, ctx: &egui::Context) {
    if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
        app.help.open = !app.help.open;
    }
    if !app.help.open {
        return;
    }
    let mut open = true;
    let mut topic = app.help.topic;
    egui::Window::new(tr!("GigCraft Help"))
        .open(&mut open)
        .default_size([760.0, 520.0])
        .default_pos(egui::pos2(80.0, 72.0))
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    for t in Topic::ALL {
                        if ui.selectable_label(topic == t, t.title()).clicked() {
                            topic = t;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("help-page")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            page(app, ui, topic);
                        });
                });
            });
        });
    app.help.open = open;
    app.help.topic = topic;
}
