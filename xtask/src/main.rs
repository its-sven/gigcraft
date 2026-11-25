//! Build tasks.
//!
//! - `cargo xtask ci`            format check, clippy, tests, layering check (the gate before review)
//! - `cargo xtask layering`      only the layering check
//! - `cargo xtask apple [--release]`
//!   build the core for iOS device, iOS simulator and macOS, generate Swift
//!   bindings and pack `apps/apple/Frameworks/GigCraftCore.xcframework`.

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
        _ => Err("usage: cargo xtask <ci | layering | apple [--release]>".into()),
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

/// Crates below the UIs must not know any UI toolkit or binding layer.
fn layering() -> Result {
    const CORE: &[&str] = &[
        "gigcraft-model",
        "gigcraft-chordpro",
        "gigcraft-store",
        "gigcraft-commands",
        "gigcraft-perform",
        "gigcraft-automation",
    ];
    const FORBIDDEN: &[&str] = &[
        "egui",
        "eframe",
        "rfd",
        "uniffi",
        "gigcraft-ui-egui",
        "gigcraft-ffi",
        "winit",
        "wgpu",
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
