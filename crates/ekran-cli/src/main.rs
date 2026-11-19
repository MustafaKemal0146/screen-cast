//! ekran CLI — komut satırı arayüzü (T-M0-04 .. T-M9-06).

mod discover;
mod doctor;
mod net;
mod selftest;
mod share;

use clap::{CommandFactory, Parser, Subcommand};
use ekran_core::config::{default_config_path, Config};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ekran",
    version,
    about = "Linux masaüstünü TV'ye canlı yansıt (TV'de uygulama gerekmez)",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command_>,
}

#[derive(Subcommand)]
enum Command_ {
    /// Masaüstü pencere arayüzünü aç (GTK3)
    Gui,
    /// Ortam kontrolü: oturum, ekranlar, ağ, araç envanteri, ölçümler
    Doctor {
        /// Ağdaki TV'leri tara
        #[arg(long)]
        discover: bool,
        /// 100 örnek gecikme ölçümü (AC-1)
        #[arg(long)]
        bench: bool,
        /// Boş ekran algılama ve bant testi (AC-2)
        #[arg(long)]
        idle: bool,
        /// Ses çıkışı değişiminde yeniden bağlama testi (AC-3)
        #[arg(long)]
        audio_switch: bool,
        /// Kalite merdiveni argüman doğrulaması (AC-9)
        #[arg(long)]
        quality: bool,
    },
    /// Ağdaki Cast / DLNA / Miracast / AirPlay cihazlarını tara
    Discover {
        /// Sabit JSON çıktısı üret
        #[arg(long)]
        json: bool,
        /// Tarama süresi (saniye)
        #[arg(long, default_value_t = 5)]
        timeout: u64,
    },
    /// Masaüstünü seçilen TV'ye veya tarayıcıya yayınla
    Share {
        /// Hedef cihaz kimliği, adı veya `browser`
        #[arg(long)]
        to: Option<String>,
        /// Yayın kalitesi: 480p | 540p | 720p | 1080p | 1440p | auto
        #[arg(long)]
        quality: Option<String>,
        /// Kare hızı (FPS)
        #[arg(long)]
        fps: Option<u32>,
        /// Sistem sesini dahil et
        #[arg(long, conflicts_with = "no_audio")]
        audio: bool,
        /// Sistem sesini kapat
        #[arg(long)]
        no_audio: bool,
        /// PulseAudio null-sink loopback hattı kullan
        #[arg(long)]
        audio_loopback: bool,
        /// Kodlayıcıyı zorla: h264_nvenc | h264_vaapi | libx264
        #[arg(long)]
        codec: Option<String>,
        /// Yakalama arka ucu: auto | x11 | wayland | test
        #[arg(long)]
        backend: Option<String>,
        /// Monitör seçimi: primary | <indeks> | <ad>
        #[arg(long)]
        monitor: Option<String>,
        /// HTTP/WS dinleme portu
        #[arg(long)]
        port: Option<u16>,
        /// Erişim anahtarı (boşsa rastgele üretilir)
        #[arg(long)]
        token: Option<String>,
        /// Etkileşimli terminal arayüzünü kapat
        #[arg(long)]
        no_tui: bool,
        /// Akışı aynı anda fMP4/MP4 dosyasına kaydet (T-M10-05)
        #[arg(long)]
        record: Option<PathBuf>,
    },
    /// Uçtan uca kendi kendine test (sanal ekran + HTTP/WS loopback)
    Selftest,
    /// Kalıcı yapılandırmayı yönet (~/.config/ekran/config.toml)
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Varsayılan config.toml dosyasını oluştur
    Init {
        /// Mevcut dosyanın üzerine yaz
        #[arg(long)]
        force: bool,
    },
    /// Aktif yapılandırmayı yazdır
    Show,
    /// Yapılandırma dosya yolunu yazdır
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
    let res: anyhow::Result<()> = match cli.command {
        None => {
            if std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var_os("DISPLAY").is_some()
            {
                run_gui()
            } else {
                Cli::command().print_help()?;
                println!();
                Ok(())
            }
        }
        Some(Command_::Gui) => run_gui(),
        Some(Command_::Doctor {
            discover,
            bench,
            idle,
            audio_switch,
            quality,
        }) => {
            doctor::run_doctor(doctor::DoctorFlags {
                discover,
                bench,
                idle,
                audio_switch,
                quality,
            })
            .await
        }
        Some(Command_::Discover { json, timeout }) => discover::run_discover(json, timeout).await,
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
                record,
            })
            .await
        }
        Some(Command_::Selftest) => selftest::run_selftest().await,
        Some(Command_::Config { action }) => handle_config(action),
    };

    if let Err(err) = res {
        if let Some(core_err) = err.downcast_ref::<ekran_core::Error>() {
            eprintln!("Hata [{}]: {}", core_err.code(), core_err);
            eprintln!("Öneri: {}", core_err.hint());
        } else {
            eprintln!("Hata: {err}");
        }
        std::process::exit(1);
    }
    Ok(())
}

fn run_gui() -> anyhow::Result<()> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let script_path = runtime_dir.join("ekran-gui.py");
    std::fs::write(&script_path, ekran_ui::gui_script())?;

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ekran"));
    let status = std::process::Command::new("python3")
        .arg(&script_path)
        .arg(&exe)
        .status()
        .map_err(|e| anyhow::anyhow!("Masaüstü arayüzü (python3 + GTK3) başlatılamadı: {e}"))?;
    if !status.success() {
        anyhow::bail!("Masaüstü arayüzü hata ile kapandı ({status})");
    }
    Ok(())
}

fn handle_config(action: Option<ConfigAction>) -> anyhow::Result<()> {
    let path = default_config_path();
    match action.unwrap_or(ConfigAction::Path) {
        ConfigAction::Path => {
            println!("{}", path.display());
        }
        ConfigAction::Show => {
            let cfg = Config::load_or_default(&path)?;
            println!("# Dosya: {}", path.display());
            println!("{}", cfg.to_toml_string()?);
        }
        ConfigAction::Init { force } => {
            if path.exists() && !force {
                println!(
                    "Yapılandırma zaten mevcut: {} (üzerine yazmak için `--force` kullanın)",
                    path.display()
                );
                return Ok(());
            }
            let cfg = Config::default();
            cfg.save(&path)?;
            println!("Yapılandırma oluşturuldu: {}", path.display());
        }
    }
    Ok(())
}
