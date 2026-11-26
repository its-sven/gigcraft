//! GigCraft desktop app.
//!
//! `gigcraft [LIBRARY] [--song TITLE|ID [--edit]] [--setlist NAME|ID [--perform]] [--rig] [--lang en|de] [--live | --join ADDRESS] [--show-help]`

mod audio_io;
mod live_io;
mod midi_io;

use gigcraft_ui_egui::{GigCraftApp, Hooks, Start};
use std::path::PathBuf;

fn parse_args() -> Result<Start, String> {
    let mut start = Start::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy().into_owned();
        let mut value = |name: &str| {
            args.next()
                .map(|v| v.to_string_lossy().into_owned())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match text.as_str() {
            "--song" => start.song = Some(value("--song")?),
            "--setlist" => start.setlist = Some(value("--setlist")?),
            "--edit" => start.edit = true,
            "--perform" => start.perform = true,
            "--rig" => start.rig = true,
            "--lang" => start.language = Some(value("--lang")?),
            "--live" => start.live = true,
            "--show-help" => start.help = true,
            "--show-live" => start.show_live = true,
            "--screen2" => start.screen2 = Some(value("--screen2")?),
            "--play" => start.play = true,
            "--join" => start.join = Some(value("--join")?),
            "-h" | "--help" => {
                return Err(
                    "usage: gigcraft [LIBRARY] [--song TITLE|ID [--edit]] [--setlist NAME|ID [--perform]] [--rig] [--lang en|de] [--live | --join ADDRESS] [--show-help]".into(),
                );
            }
            _ if text.starts_with('-') => return Err(format!("unknown option {text}")),
            _ => start.library = Some(PathBuf::from(arg)),
        }
    }
    Ok(start)
}

fn main() -> eframe::Result {
    let start = match parse_args() {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("GigCraft")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([640.0, 420.0])
            .with_icon(eframe::egui::IconData {
                rgba: include_bytes!("../assets/icon-256.rgba").to_vec(),
                width: 256,
                height: 256,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "GigCraft",
        options,
        Box::new(move |cc| {
            let hooks = Hooks {
                pick_folder: Some(Box::new(|| rfd::FileDialog::new().pick_folder())),
                pick_file: Some(Box::new(|| rfd::FileDialog::new().pick_file())),
                midi: Some(Box::new(midi_io::Midir::new())),
                live: Some(Box::new(live_io::Net::default())),
                audio: Some(Box::new(audio_io::Cpal::default())),
            };
            Ok(Box::new(GigCraftApp::new(cc, start, hooks)))
        }),
    )
}
