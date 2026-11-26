//! Build tasks.
//!
//! - `cargo xtask ci`            format check, clippy, tests, layering check (the gate before review)
//! - `cargo xtask layering`      only the layering check
//! - `cargo xtask apple [--release]`
//!   build the core for iOS device, iOS simulator and macOS, generate Swift
//!   bindings and pack `apps/apple/Frameworks/GigCraftCore.xcframework`.
//! - `cargo xtask package [--build N]`
//!   the desktop app as a download for this OS in `dist/`: macOS a universal
//!   `GigCraft.app` (zip), Linux a tar.gz with a desktop entry, Windows a zip.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

type Result<T = ()> = std::result::Result<T, String>;
type Step = (&'static str, Box<dyn Fn() -> Result>);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("layering") => layering(),
        Some("apple") => apple(args.iter().any(|a| a == "--release")),
        Some("package") => package(
            args.iter()
                .position(|a| a == "--build")
                .and_then(|i| args.get(i + 1))
                .map_or("0", String::as_str),
        ),
        _ => Err(
            "usage: cargo xtask <ci | layering | apple [--release] | package [--build N]>".into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn run(cmd: &mut Command) -> Result {
    eprintln!("» {}", format!("{cmd:?}").replace('"', ""));
    let status = cmd
        .current_dir(root())
        .status()
        .map_err(|e| format!("{cmd:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed: {cmd:?}"))
    }
}

fn cargo() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn ci() -> Result {
    let steps: [Step; 4] = [
        (
            "format",
            Box::new(|| run(cargo().args(["fmt", "--all", "--check"]))),
        ),
        (
            "clippy",
            Box::new(|| {
                run(cargo().args([
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ]))
            }),
        ),
        (
            "tests",
            Box::new(|| run(cargo().args(["test", "--workspace", "--locked"]))),
        ),
        ("layering", Box::new(layering)),
    ];
    let mut failed = Vec::new();
    for (name, step) in &steps {
        match step() {
            Ok(()) => eprintln!("ok    {name}"),
            Err(e) => {
                eprintln!("FAIL  {name}: {e}");
                failed.push(*name);
                break;
            }
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("ci failed at: {}", failed.join(", ")))
    }
}

/// Crates below the UIs must not know any UI toolkit, binding layer or
/// platform host (audio device access lives in host crates such as
/// `gigcraft-audio-cpal`).
fn layering() -> Result {
    const CORE: &[&str] = &[
        "gigcraft-model",
        "gigcraft-chordpro",
        "gigcraft-store",
        "gigcraft-commands",
        "gigcraft-perform",
        "gigcraft-automation",
        "gigcraft-midi",
        "gigcraft-rig",
        "gigcraft-formats",
        "gigcraft-live",
        "gigcraft-audio",
    ];
    const FORBIDDEN: &[&str] = &[
        "gigcraft-live-net",
        "tungstenite",
        "egui",
        "eframe",
        "rfd",
        "uniffi",
        "gigcraft-ui-egui",
        "gigcraft-ffi",
        "winit",
        "wgpu",
        "cpal",
        "gigcraft-audio-cpal",
    ];
    let out = cargo()
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root())
        .output()
        .map_err(|e| e.to_string())?;
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut problems = Vec::new();
    for pkg in meta["packages"].as_array().into_iter().flatten() {
        let name = pkg["name"].as_str().unwrap_or_default();
        if !CORE.contains(&name) {
            continue;
        }
        for dep in pkg["dependencies"].as_array().into_iter().flatten() {
            let dep_name = dep["name"].as_str().unwrap_or_default();
            if FORBIDDEN.contains(&dep_name) {
                problems.push(format!("{name} depends on {dep_name}"));
            }
        }
    }
    if problems.is_empty() {
        eprintln!("layering ok ({} core crates)", CORE.len());
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

fn apple(release: bool) -> Result {
    let profile = if release { "release" } else { "debug" };
    let targets = [
        "aarch64-apple-ios",
        "aarch64-apple-ios-sim",
        "aarch64-apple-darwin",
    ];
    for t in targets {
        let mut c = cargo();
        c.args(["build", "-p", "gigcraft-ffi", "--lib", "--target", t]);
        if release {
            c.arg("--release");
        }
        // Match the Xcode project's deployment target.
        c.env("IPHONEOS_DEPLOYMENT_TARGET", "17.0");
        run(&mut c)?;
    }
    let target_dir = root().join("target");
    let lib = |t: &str, ext: &str| {
        target_dir
            .join(t)
            .join(profile)
            .join(format!("libgigcraft_ffi.{ext}"))
    };

    let gen_dir = target_dir.join("uniffi-swift");
    let _ = std::fs::remove_dir_all(&gen_dir);
    run(cargo()
        .args([
            "run",
            "--quiet",
            "-p",
            "gigcraft-bindgen",
            "--",
            "generate",
            "--library",
        ])
        .arg(lib("aarch64-apple-darwin", "dylib"))
        .args(["--language", "swift", "--out-dir"])
        .arg(&gen_dir))?;

    // Headers + a module map named module.modulemap for the XCFramework.
    let headers = gen_dir.join("headers");
    std::fs::create_dir_all(&headers).map_err(|e| e.to_string())?;
    let mut swift_file = None;
    for entry in std::fs::read_dir(&gen_dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let p = entry.path();
        match p.extension().and_then(|e| e.to_str()) {
            Some("h") => {
                std::fs::copy(&p, headers.join(entry.file_name())).map_err(|e| e.to_string())?;
            }
            Some("modulemap") => {
                std::fs::copy(&p, headers.join("module.modulemap")).map_err(|e| e.to_string())?;
            }
            Some("swift") => swift_file = Some(p),
            _ => {}
        }
    }
    let swift_file = swift_file.ok_or("bindgen produced no Swift file")?;
    // The real-time audio functions (ADR-0014) are plain C, next to UniFFI's header.
    std::fs::copy(
        root().join("crates/gigcraft-ffi/include/gigcraft_audio.h"),
        headers.join("gigcraft_audio.h"),
    )
    .map_err(|e| e.to_string())?;
    let map_path = headers.join("module.modulemap");
    let map = std::fs::read_to_string(&map_path).map_err(|e| e.to_string())?;
    if !map.contains("gigcraft_audio.h") {
        let map = map.replacen(
            "    export *",
            "    header \"gigcraft_audio.h\"\n    export *",
            1,
        );
        std::fs::write(&map_path, map).map_err(|e| e.to_string())?;
    }

    let apple_dir = root().join("apps/apple");
    let xcf = apple_dir.join("Frameworks/GigCraftCore.xcframework");
    let _ = std::fs::remove_dir_all(&xcf);
    std::fs::create_dir_all(apple_dir.join("Frameworks")).map_err(|e| e.to_string())?;
    let mut c = Command::new("xcodebuild");
    c.arg("-create-xcframework");
    for t in ["aarch64-apple-ios", "aarch64-apple-ios-sim"] {
        c.arg("-library")
            .arg(lib(t, "a"))
            .arg("-headers")
            .arg(&headers);
    }
    c.arg("-output").arg(&xcf);
    run(&mut c)?;

    let generated = apple_dir.join("Generated");
    std::fs::create_dir_all(&generated).map_err(|e| e.to_string())?;
    std::fs::copy(&swift_file, generated.join("GigCraftCore.swift")).map_err(|e| e.to_string())?;
    eprintln!(
        "apple ok: {} and Generated/GigCraftCore.swift",
        xcf.display()
    );
    Ok(())
}

/// The workspace version from Cargo.toml (`0.2.0`).
fn version() -> Result<String> {
    let toml = std::fs::read_to_string(root().join("Cargo.toml")).map_err(|e| e.to_string())?;
    toml.lines()
        .find_map(|l| {
            l.strip_prefix("version = \"")
                .and_then(|v| v.strip_suffix('"'))
        })
        .map(str::to_string)
        .ok_or_else(|| "no workspace version in Cargo.toml".into())
}

fn package(build: &str) -> Result {
    let dist = root().join("dist");
    std::fs::create_dir_all(&dist).map_err(|e| e.to_string())?;
    let version = version()?;
    if cfg!(target_os = "macos") {
        package_macos(&dist, &version, build)
    } else if cfg!(target_os = "linux") {
        package_linux(&dist, &version)
    } else {
        package_windows(&dist, &version)
    }
}

fn release_build(target: Option<&str>) -> Result<PathBuf> {
    let mut c = cargo();
    c.args(["build", "--release", "--locked", "-p", "gigcraft-desktop"]);
    if let Some(t) = target {
        c.args(["--target", t]);
    }
    run(&mut c)?;
    let dir = root().join("target");
    let dir = match target {
        Some(t) => dir.join(t).join("release"),
        None => dir.join("release"),
    };
    Ok(dir.join(if cfg!(windows) {
        "gigcraft.exe"
    } else {
        "gigcraft"
    }))
}

fn package_macos(dist: &Path, version: &str, build: &str) -> Result {
    // One app for Apple silicon and Intel.
    let arm = release_build(Some("aarch64-apple-darwin"))?;
    let intel = release_build(Some("x86_64-apple-darwin"))?;
    let work = root().join("target/package");
    let _ = std::fs::remove_dir_all(&work);
    let app = work.join("GigCraft.app/Contents");
    for d in ["MacOS", "Resources"] {
        std::fs::create_dir_all(app.join(d)).map_err(|e| e.to_string())?;
    }
    run(Command::new("lipo")
        .args(["-create", "-output"])
        .arg(app.join("MacOS/gigcraft"))
        .arg(&arm)
        .arg(&intel))?;
    // Icon: the 1024 px PNG as an .icns.
    let iconset = work.join("GigCraft.iconset");
    std::fs::create_dir_all(&iconset).map_err(|e| e.to_string())?;
    let icon = root().join("docs/img/icon.png");
    for size in [16u32, 32, 128, 256, 512] {
        for (scale, suffix) in [(1, ""), (2, "@2x")] {
            let px = (size * scale).to_string();
            run(Command::new("sips")
                .args(["-z", &px, &px])
                .arg(&icon)
                .arg("--out")
                .arg(iconset.join(format!("icon_{size}x{size}{suffix}.png")))
                .stdout(std::process::Stdio::null()))?;
        }
    }
    run(Command::new("iconutil")
        .args(["-c", "icns", "-o"])
        .arg(app.join("Resources/GigCraft.icns"))
        .arg(&iconset))?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>GigCraft</string>
  <key>CFBundleDisplayName</key><string>GigCraft</string>
  <key>CFBundleIdentifier</key><string>io.github.its-sven.gigcraft.desktop</string>
  <key>CFBundleExecutable</key><string>gigcraft</string>
  <key>CFBundleIconFile</key><string>GigCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleVersion</key><string>{build}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key><string>GigCraft serves your setlist to the band on the local network.</string>
</dict>
</plist>
"#
    );
    std::fs::write(app.join("Info.plist"), plist).map_err(|e| e.to_string())?;
    // Ad-hoc signature: required to run on Apple silicon; not notarized.
    run(Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(work.join("GigCraft.app")))?;
    let zip = dist.join(format!("GigCraft-macOS-{version}.zip"));
    let _ = std::fs::remove_file(&zip);
    run(Command::new("ditto")
        .args(["-c", "-k", "--keepParent"])
        .arg(work.join("GigCraft.app"))
        .arg(&zip))?;
    eprintln!("packaged {}", zip.display());
    Ok(())
}

fn package_linux(dist: &Path, version: &str) -> Result {
    let bin = release_build(None)?;
    let name = format!("GigCraft-linux-x86_64-{version}");
    let work = root().join("target/package").join(&name);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    std::fs::copy(&bin, work.join("gigcraft")).map_err(|e| e.to_string())?;
    std::fs::copy(root().join("docs/img/icon.png"), work.join("gigcraft.png"))
        .map_err(|e| e.to_string())?;
    std::fs::write(
        work.join("gigcraft.desktop"),
        "[Desktop Entry]\nType=Application\nName=GigCraft\nComment=Setlists, chord charts and presets for the stage\nExec=gigcraft\nIcon=gigcraft\nCategories=Audio;Music;\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        work.join("README.txt"),
        "GigCraft for Linux\n\nRun ./gigcraft. To add it to your menu: copy gigcraft to ~/.local/bin,\ngigcraft.png to ~/.local/share/icons and gigcraft.desktop to ~/.local/share/applications.\n",
    )
    .map_err(|e| e.to_string())?;
    let tar = dist.join(format!("{name}.tar.gz"));
    run(Command::new("tar")
        .arg("-czf")
        .arg(&tar)
        .arg("-C")
        .arg(root().join("target/package"))
        .arg(&name))?;
    eprintln!("packaged {}", tar.display());
    Ok(())
}

fn package_windows(dist: &Path, version: &str) -> Result {
    let bin = release_build(None)?;
    let zip = dist.join(format!("GigCraft-windows-x86_64-{version}.zip"));
    let _ = std::fs::remove_file(&zip);
    // Windows 10+ ships bsdtar, which writes zip files.
    run(Command::new("tar")
        .args(["-a", "-c", "-f"])
        .arg(&zip)
        .arg("-C")
        .arg(bin.parent().unwrap_or(Path::new(".")))
        .arg("gigcraft.exe"))?;
    eprintln!("packaged {}", zip.display());
    Ok(())
}
