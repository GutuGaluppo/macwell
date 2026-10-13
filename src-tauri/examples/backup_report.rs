//! Read-only backup state: `cargo run --release --example backup_report`.
//! Runs only `defaults` reads and `tmutil` queries; nothing is mounted or changed.
use organizemymac_lib::backup::{read_state, timemachine::DEFAULT_STALE_DAYS};
use organizemymac_lib::exec::RealRunner;
use organizemymac_lib::filesystem::probe_full_disk_access;

fn main() {
    let home = dirs::home_dir().unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let s = read_state(&RealRunner, &home, now, DEFAULT_STALE_DAYS, probe_full_disk_access(&home));
    let tm = &s.time_machine;
    let when = |ms: Option<i64>| ms.map_or("—".to_string(), |m| chrono::DateTime::from_timestamp_millis(m).unwrap().with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string());
    println!("Time Machine: {:?}  (limite {} dias)", tm.status, s.stale_days);
    println!("  último backup concluído: {} ({:?}), há {:?} dia(s)", when(tm.last_backup_ms), tm.last_backup_source, tm.age_days);
    println!("  última tentativa: {}   automático: {:?}   em andamento: {}", when(tm.last_attempt_ms), tm.auto_backup, tm.running);
    for d in &tm.destinations {
        println!("  destino {:?} ({:?}) montado={:?} cota={:?} GB criptografado={:?}", d.name, d.kind, d.mounted, d.quota_gb, d.encrypted);
    }
    println!("  instantâneos locais (no próprio disco, não são backup): {}", tm.local_snapshot_count);
    for u in &tm.unreadable {
        println!("  não foi possível ler {:?}: {:?} {:?}", u.source, u.reason, u.detail);
    }
    let c = &s.icloud;
    println!("iCloud (inferido): pasta={} conta={:?} drive={:?} mesa={:?} documentos={:?}", c.drive_folder_present, c.signed_in, c.drive_enabled, c.desktop_synced, c.documents_synced);
    for u in &c.unreadable {
        println!("  não foi possível ler {:?}: {:?} {:?}", u.source, u.reason, u.detail);
    }
    println!("Acesso Total ao Disco: {:?}", s.full_disk_access);
}
