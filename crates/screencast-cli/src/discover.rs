//! `screencast discover [--json]` command implementation.

use screencast_core::i18n::{t, TextKey};
use screencast_core::Discovery;
use screencast_sinks::CombinedDiscovery;
use screencast_transport::ApiDiscover;
use std::time::Duration;

pub async fn run_discover(json: bool, timeout_secs: u64, active_p2p: bool) -> anyhow::Result<()> {
    let scanner = CombinedDiscovery::new().with_active_p2p(active_p2p);
    let devices = scanner
        .scan(Duration::from_secs(timeout_secs.max(1)))
        .await?;

    if json {
        let payload = ApiDiscover { devices };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if devices.is_empty() {
        println!("{}", t(TextKey::DiscoverNoneFound));
        println!("{}", t(TextKey::DiscoverHint));
        return Ok(());
    }

    println!(
        "{:<22} {:<16} {:<22} {}",
        t(TextKey::DiscoverColId),
        t(TextKey::DiscoverColProto),
        t(TextKey::DiscoverColAddr),
        t(TextKey::DiscoverColNameNote)
    );
    println!("{}", "-".repeat(78));
    for d in devices {
        let note = d
            .note
            .as_deref()
            .unwrap_or_else(|| t(TextKey::DiscoverDefaultNote));
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
