//! `ekran discover [--json]` command implementation (T-M3-03).

use ekran_core::Discovery;
use ekran_sinks::CombinedDiscovery;
use ekran_transport::ApiDiscover;
use std::time::Duration;

pub async fn run_discover(json: bool, timeout_secs: u64) -> anyhow::Result<()> {
    let scanner = CombinedDiscovery::new();
    let devices = scanner
        .scan(Duration::from_secs(timeout_secs.max(1)))
        .await?;

    if json {
        let payload = ApiDiscover { devices };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if devices.is_empty() {
        println!("Ağda Cast veya DLNA cihazı bulunamadı.");
        println!("İpucu: TV'nin aynı Wi-Fi/LAN ağında olduğundan emin olun veya `ekran share --to browser` kullanın.");
        return Ok(());
    }

    println!(
        "{:<22} {:<16} {:<22} AD / NOT",
        "KİMLİK", "PROTOKOL", "ADRES"
    );
    println!("{}", "-".repeat(78));
    for d in devices {
        let note = d.note.as_deref().unwrap_or("uygun");
        println!(
            "{:<22} {:<16} {:<22} {} ({})",
            truncate(&d.id, 21),
            d.kind.label(),
            format!("{}:{}", d.address, d.port),
            d.name,
            note
        );
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
