//! `gigcraft-cli`: run GigCraft commands against a library folder.
//!
//! ```sh
//! gigcraft-cli init ~/Music/Gigs --demo
//! gigcraft-cli run -l ~/Music/Gigs --cmd song.list --cmd 'song.create={"title":"New"}'
//! gigcraft-cli commands --json
//! gigcraft-cli mcp -l ~/Music/Gigs        # stdio MCP server for AI agents
//! ```
//!
//! Exit codes: 0 ok, 1 a command failed, 2 bad usage or the library could not be opened.

use clap::{Parser, Subcommand};
use gigcraft_automation::{McpServer, parse_cmd, run, specs_json};
use gigcraft_commands::Engine;
use gigcraft_store::Library;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "gigcraft-cli",
    version,
    about = "Run GigCraft commands against a library folder"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Create a new library folder.
    Init {
        dir: PathBuf,
        /// Library name (default: the folder name).
        #[arg(long)]
        name: Option<String>,
        /// Fill it with the demo songs and setlist.
        #[arg(long)]
        demo: bool,
    },
    /// Run commands in order; prints one JSON result per line and stops at the first error.
    Run {
        #[arg(short, long)]
        library: PathBuf,
        /// `id` or `id={json params}`; repeatable.
        #[arg(long = "cmd", required = true)]
        cmds: Vec<String>,
        /// Pretty-print results.
        #[arg(long)]
        pretty: bool,
    },
    /// List all commands.
    Commands {
        /// Full specs as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Serve MCP over stdin/stdout for one library.
    Mcp {
        #[arg(short, long)]
        library: PathBuf,
    },
}

fn open(dir: &PathBuf) -> Result<Engine, ExitCode> {
    Library::open(dir).map(Engine::new).map_err(|e| {
        eprintln!("gigcraft-cli: {e}");
        ExitCode::from(2)
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Init { dir, name, demo } => {
            let name = name.unwrap_or_else(|| {
                dir.file_name()
                    .map_or_else(|| "GigCraft".into(), |n| n.to_string_lossy().into_owned())
            });
            let lib = match Library::create(&dir, &name) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("gigcraft-cli: {e}");
                    return ExitCode::from(2);
                }
            };
            let mut engine = Engine::new(lib);
            if demo && let Err(e) = gigcraft_commands::demo::populate(&mut engine) {
                eprintln!("gigcraft-cli: demo: {e}");
                return ExitCode::from(1);
            }
            println!("{}", dir.display());
            ExitCode::SUCCESS
        }
        Cmd::Run {
            library,
            cmds,
            pretty,
        } => {
            let parsed: Result<Vec<_>, String> = cmds.iter().map(|c| parse_cmd(c)).collect();
            let parsed = match parsed {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("gigcraft-cli: {e}");
                    return ExitCode::from(2);
                }
            };
            let mut engine = match open(&library) {
                Ok(e) => e,
                Err(code) => return code,
            };
            for (id, params) in parsed {
                match run(&mut engine, &id, &params) {
                    Ok(out) => {
                        let text = if pretty {
                            serde_json::to_string_pretty(&out)
                        } else {
                            serde_json::to_string(&out)
                        };
                        println!("{}", text.unwrap_or_default());
                    }
                    Err(e) => {
                        let err = serde_json::json!({"command": id, "error": e.code(), "message": e.to_string()});
                        eprintln!("{err}");
                        return ExitCode::from(1);
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Cmd::Commands { json } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&specs_json()).unwrap_or_default()
                );
            } else {
                for spec in gigcraft_commands::specs() {
                    println!("{:<22} {}", spec.id, spec.summary);
                }
            }
            ExitCode::SUCCESS
        }
        Cmd::Mcp { library } => {
            let mut engine = match open(&library) {
                Ok(e) => e,
                Err(code) => return code,
            };
            let server = McpServer::new(&mut engine);
            let stdin = std::io::stdin().lock();
            let stdout = std::io::stdout().lock();
            match server.serve(stdin, stdout) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("gigcraft-cli: {e}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
