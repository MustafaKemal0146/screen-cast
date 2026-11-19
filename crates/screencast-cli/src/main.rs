//! screencast CLI — command-line interface and GTK3 desktop GUI launcher.

mod discover;
mod doctor;
mod net;
mod selftest;
mod share;

use clap::{CommandFactory, Parser, Subcommand};
use screencast_core::config::{default_config_path, explicit_language_in_toml, Config};
use screencast_core::i18n::{
    current_language, resolve_language, set_current_language, tf, Language, TextKey,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "screencast",
    version,
    about = "Cast your Linux desktop to a TV live (no TV app required)",
    long_about = None
)]
struct Cli {
    /// Display language: en | tr (overrides SCREENCAST_LANG, config.toml, LANG)
    #[arg(long, global = true, value_name = "en|tr")]
    lang: Option<String>,
    #[command(subcommand)]
    command: Option<Command_>,
}

#[derive(Subcommand)]
enum Command_ {
    /// Open desktop graphical interface (GTK3)
    Gui,
    /// Environment diagnostics: session, displays, network, tools, benchmarks
    Doctor {
        /// Scan TVs on the network
        #[arg(long)]
        discover: bool,
        /// Run 100-sample latency benchmark (AC-1)
        #[arg(long)]
        bench: bool,
        /// Run idle detection and bitrate test (AC-2)
        #[arg(long)]
        idle: bool,
        /// Run audio sink switch watcher test (AC-3)
        #[arg(long)]
        audio_switch: bool,
        /// Verify quality ladder encoding arguments (AC-9)
        #[arg(long)]
        quality: bool,
        /// Print detailed WDI/MCC, DFS P2P-Client, and MS-MICE wireless diagnostics
        #[arg(long)]
        wireless: bool,
    },
    /// Scan Cast / DLNA / Miracast / AirPlay devices on the network
    Discover {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
        /// Discovery duration in seconds
        #[arg(long, default_value_t = 5)]
        timeout: u64,
        /// Include active Wi-Fi Direct P2P radio discovery (`wpa_cli` / NetworkManager WifiP2P)
        #[arg(long)]
        p2p: bool,
    },
    /// Stream desktop to selected TV or web browser
    Share {
        /// Target device ID, name, or `browser`
        #[arg(long)]
        to: Option<String>,
        /// Video quality: 480p | 540p | 720p | 1080p | 1440p | auto
        #[arg(long)]
        quality: Option<String>,
        /// Frame rate (FPS)
        #[arg(long)]
        fps: Option<u32>,
        /// Include system audio
        #[arg(long, conflicts_with = "no_audio")]
        audio: bool,
        /// Disable system audio
        #[arg(long)]
        no_audio: bool,
        /// Use a PulseAudio null-sink loopback pipeline
        #[arg(long)]
        audio_loopback: bool,
        /// Force video encoder: h264_nvenc | h264_vaapi | libx264
        #[arg(long)]
        codec: Option<String>,
        /// Capture backend: auto | x11 | wayland | test
        #[arg(long)]
        backend: Option<String>,
        /// Monitor selection: primary | <index> | <name>
        #[arg(long)]
        monitor: Option<String>,
        /// HTTP/WS listen port
        #[arg(long)]
        port: Option<u16>,
        /// Access token (random if omitted)
        #[arg(long)]
        token: Option<String>,
        /// Disable interactive terminal UI
        #[arg(long)]
        no_tui: bool,
        /// Include active Wi-Fi Direct P2P radio discovery
        #[arg(long)]
        p2p: bool,
        /// Record stream simultaneously to an fMP4/MP4 file
        #[arg(long)]
        record: Option<PathBuf>,
    },
    /// End-to-end self-test (virtual display + HTTP/WS loopback)
    Selftest,
    /// Manage persistent configuration (~/.config/screencast/config.toml)
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Write default config.toml file
    Init {
        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },
    /// Print active configuration
    Show,
    /// Print configuration file path
    Path,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();
    let res = run_parsed_cli(cli).await;

    if let Err(err) = res {
        let lang = current_language();
        if let Some(core_err) = err.downcast_ref::<screencast_core::Error>() {
            let err_s = core_err.message(lang);
            let hint_s = core_err.hint_for(lang);
            eprintln!(
                "{}",
                tf(
                    TextKey::CliErrPrefix,
                    &[("code", core_err.code()), ("err", &err_s)]
                )
            );
            eprintln!("{}", tf(TextKey::CliHintPrefix, &[("hint", &hint_s)]));
        } else {
            let err_s = err.to_string();
            eprintln!("{}", tf(TextKey::CliErrPlain, &[("err", &err_s)]));
        }
        std::process::exit(1);
    }
    Ok(())
}

async fn run_parsed_cli(cli: Cli) -> anyhow::Result<()> {
    let cfg_lang = explicit_language_in_toml(&default_config_path());
    set_current_language(resolve_language(None, cfg_lang));
    let explicit_cli_lang = match cli.lang.as_deref() {
        Some(raw) => Some(Language::parse(raw)?),
        None => None,
    };
    let resolved_lang = resolve_language(explicit_cli_lang, cfg_lang);
    set_current_language(resolved_lang);

    match cli.command {
        None => {
            if std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var_os("DISPLAY").is_some()
            {
                run_gui(resolved_lang)
            } else {
                Cli::command().print_help()?;
                println!();
                Ok(())
            }
        }
        Some(Command_::Gui) => run_gui(resolved_lang),
        Some(Command_::Doctor {
            discover,
            bench,
            idle,
            audio_switch,
            quality,
            wireless,
        }) => {
            doctor::run_doctor(doctor::DoctorFlags {
                discover,
                bench,
                idle,
                audio_switch,
                quality,
                wireless,
            })
            .await
        }
        Some(Command_::Discover { json, timeout, p2p }) => {
            discover::run_discover(json, timeout, p2p).await
        }
        Some(Command_::Share {
            to,
            quality,
            fps,
            audio,
            no_audio,
            audio_loopback,
            codec,
            backend,
            monitor,
            port,
            token,
            no_tui,
            p2p,
            record,
        }) => {
            let audio_flag = if audio {
                Some(true)
            } else if no_audio {
                Some(false)
            } else {
                None
            };
            share::run_share(share::ShareArgs {
                to,
                quality,
                fps,
                audio: audio_flag,
                audio_loopback,
                codec,
                backend,
                monitor,
                port,
                token,
                no_tui,
                p2p,
                record,
            })
            .await
        }
        Some(Command_::Selftest) => selftest::run_selftest().await,
        Some(Command_::Config { action }) => handle_config(action, explicit_cli_lang),
    }
}

fn run_gui(lang: Language) -> anyhow::Result<()> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let gui_dir = runtime_dir.join("screencast-gui");
    std::fs::create_dir_all(&gui_dir)?;
    for (filename, content) in screencast_ui::gui_assets() {
        std::fs::write(gui_dir.join(filename), content)?;
    }

    let script_path = gui_dir.join("screencast_gui.py");
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("screencast"));
    let status = std::process::Command::new("python3")
        .arg(&script_path)
        .arg("--lang")
        .arg(lang.code())
        .arg(&exe)
        .env("SCREENCAST_LANG", lang.code())
        .status()
        .map_err(|e| {
            let err_s = e.to_string();
            anyhow::anyhow!(tf(TextKey::CliGuiStartFailed, &[("err", &err_s)]))
        })?;
    if !status.success() {
        let status_s = status.to_string();
        anyhow::bail!(
            "{}",
            tf(TextKey::CliGuiExitFailed, &[("status", &status_s)])
        );
    }
    Ok(())
}

fn handle_config(
    action: Option<ConfigAction>,
    explicit_cli_lang: Option<Language>,
) -> anyhow::Result<()> {
    let path = default_config_path();
    let path_s = path.display().to_string();
    match action.unwrap_or(ConfigAction::Path) {
        ConfigAction::Path => {
            println!("{path_s}");
        }
        ConfigAction::Show => {
            let cfg = Config::load_or_default(&path)?;
            println!(
                "{}",
                tf(TextKey::CliConfigFileComment, &[("path", &path_s)])
            );
            println!("{}", cfg.to_toml_string()?);
        }
        ConfigAction::Init { force } => {
            if path.exists() && !force {
                println!("{}", tf(TextKey::CliConfigExists, &[("path", &path_s)]));
                return Ok(());
            }
            let cfg = Config {
                language: explicit_cli_lang.unwrap_or(Language::En),
                ..Default::default()
            };
            cfg.save(&path)?;
            println!("{}", tf(TextKey::CliConfigCreated, &[("path", &path_s)]));
        }
    }
    Ok(())
}
