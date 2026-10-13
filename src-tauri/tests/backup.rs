use chrono::{TimeZone, Utc};
use organizemymac_lib::backup::icloud;
use organizemymac_lib::backup::timemachine::{self, classify, Facts, Status};
use organizemymac_lib::backup::{read_state, Reason, Source};
use organizemymac_lib::exec::FakeRunner;
use organizemymac_lib::filesystem::AccessStatus;

const DAY: i64 = 86_400_000;

fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> i64 {
    Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap().timestamp_millis()
}

/// "Now" for every test: 2026-10-13 12:00 UTC.
fn now() -> i64 {
    at(2026, 10, 13, 12, 0)
}

// ---- fixtures: shapes taken from real `defaults` and `tmutil` output, identifiers replaced.

fn prefs(snapshot_dates: &[&str], auto_backup: i32, destinations: bool) -> String {
    let dates: String = snapshot_dates.iter().map(|d| format!("<date>{d}</date>")).collect();
    let dest = if destinations {
        format!(
            r#"<key>Destinations</key><array><dict>
              <key>AttemptDates</key><array><date>2026-10-13T10:05:50Z</date><date>2026-10-13T10:39:18Z</date></array>
              <key>BytesAvailable</key><integer>418880163840</integer>
              <key>BytesUsed</key><integer>181119836160</integer>
              <key>DestinationID</key><string>DEST-1</string>
              <key>LastKnownEncryptionState</key><string>Encrypted</string>
              <key>LastKnownVolumeName</key><string>TM-Disk</string>
              <key>QuotaGB</key><integer>600</integer>
              <key>SnapshotDates</key><array>{dates}</array>
            </dict></array>"#
        )
    } else {
        String::new()
    };
    format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>AutoBackup</key><integer>{auto_backup}</integer>{dest}</dict></plist>"#)
}

fn info(mounted: bool) -> String {
    let mount = if mounted { "<key>MountPoint</key><string>/Volumes/TM-Disk</string>" } else { "" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>Destinations</key><array><dict>
          <key>Kind</key><string>Local</string><key>ID</key><string>DEST-1</string><key>Name</key><string>TM-Disk</string>{mount}
        </dict></array></dict></plist>"#
    )
}

const STATUS_IDLE: &str = r#"<?xml version="1.0"?><plist version="1.0"><dict><key>ClientID</key><string>com.apple.backupd</string><key>Percent</key><real>-1</real><key>Running</key><false/></dict></plist>"#;
const STATUS_RUNNING: &str = r#"<?xml version="1.0"?><plist version="1.0"><dict><key>Percent</key><real>0.42</real><key>Running</key><true/></dict></plist>"#;
const LOCAL_SNAPSHOTS: &str = "Snapshot dates for all disks:\n2026-10-13-120555\n2026-10-13-123924\n";

/// Everything answers, as on a Mac with a healthy setup.
fn healthy(snapshot: &str, mounted: bool) -> FakeRunner {
    FakeRunner::new()
        .route("export /Library/Preferences/com.apple.TimeMachine", &prefs(&[snapshot], 1, true))
        .route("destinationinfo", &info(mounted))
        .route("status -X", STATUS_IDLE)
        .route("listlocalsnapshotdates", LOCAL_SNAPSHOTS)
}

fn tm(fake: &FakeRunner) -> timemachine::TimeMachineState {
    timemachine::read(fake, now(), 7)
}

#[test]
fn a_recent_completed_backup_from_the_preferences_needs_no_disk() {
    let fake = healthy("2026-10-13T11:24:02Z", false);
    let s = tm(&fake);
    assert_eq!(s.status, Status::Recent);
    assert_eq!(s.last_backup_source, Some(Source::Preferences));
    assert_eq!(s.age_days, Some(0));
    assert_eq!(s.auto_backup, Some(true));
    assert_eq!(s.destinations.len(), 1);
    let d = &s.destinations[0];
    assert_eq!((d.name.as_deref(), d.mounted, d.quota_gb, d.encrypted), (Some("TM-Disk"), Some(false), Some(600), Some(true)));
    assert_eq!(d.last_attempt_ms, Some(at(2026, 10, 13, 10, 39) + 18_000));
    assert!(s.unreadable.is_empty());
    // Reading must never try to mount the disk.
    assert!(!fake.calls().iter().any(|c| c.contains("latestbackup")), "{:?}", fake.calls());
}

#[test]
fn old_backups_are_stale_and_the_limit_is_inclusive() {
    let thirty = healthy("2026-09-13T11:00:00Z", false);
    let s = tm(&thirty);
    assert_eq!((s.status, s.age_days), (Status::Stale, Some(30)));
    // Exactly seven days old is still recent; one minute more is not.
    let edge = healthy("2026-10-06T12:00:00Z", false);
    assert_eq!(tm(&edge).status, Status::Recent);
    let over = healthy("2026-10-06T11:59:00Z", false);
    assert_eq!(tm(&over).status, Status::Stale);
    // The limit is a setting.
    assert_eq!(timemachine::read(&over, now(), 10).status, Status::Recent);
}

#[test]
fn automatic_backups_switched_off_is_reported() {
    let fake = FakeRunner::new()
        .route("export /Library/Preferences/com.apple.TimeMachine", &prefs(&["2026-10-13T11:00:00Z"], 0, true))
        .route("destinationinfo", &info(false))
        .route("status -X", STATUS_IDLE)
        .route("listlocalsnapshotdates", "");
    assert_eq!(tm(&fake).auto_backup, Some(false));
}

#[test]
fn no_destination_means_no_backup_whatever_local_snapshots_exist() {
    let fake = FakeRunner::new()
        .route("export /Library/Preferences/com.apple.TimeMachine", &prefs(&[], 1, false))
        .route("destinationinfo", "No destinations configured.\n")
        .route("status -X", STATUS_IDLE)
        .route("listlocalsnapshotdates", LOCAL_SNAPSHOTS);
    let s = tm(&fake);
    assert_eq!(s.status, Status::NoDestination);
    assert!(s.destinations.is_empty() && s.last_backup_ms.is_none());
    // Snapshots on the startup disk are counted but are not a backup.
    assert_eq!(s.local_snapshot_count, 2);
    assert!(s.newest_local_snapshot_ms.is_some());
}

#[test]
fn a_destination_with_no_completed_backup_is_never_backed_up() {
    let fake = healthy("2026-10-13T11:24:02Z", false);
    let empty = FakeRunner::new()
        .route("export /Library/Preferences/com.apple.TimeMachine", &prefs(&[], 1, true))
        .route("destinationinfo", &info(false))
        .route("status -X", STATUS_IDLE)
        .route("listlocalsnapshotdates", "");
    assert_eq!(tm(&empty).status, Status::NeverBackedUp);
    assert_eq!(tm(&fake).status, Status::Recent);
}

#[test]
fn unreadable_sources_never_become_all_good() {
    // Nothing can be read at all.
    let blind = FakeRunner::new().route_failure("defaults", 1, "defaults: cannot read").route_failure("tmutil", 1, "tmutil: failed");
    let s = tm(&blind);
    assert_eq!(s.status, Status::Unknown);
    assert!(s.last_backup_ms.is_none() && s.age_days.is_none());
    assert_eq!(s.unreadable.len(), 4);

    // The destination is known but the preferences (where the dates live) cannot be read: the age is unknown.
    let no_dates = FakeRunner::new()
        .route_failure("export /Library/Preferences/com.apple.TimeMachine", 1, "Operation not permitted")
        .route("destinationinfo", &info(false))
        .route("status -X", STATUS_IDLE)
        .route("listlocalsnapshotdates", "");
    let s = tm(&no_dates);
    assert_eq!(s.status, Status::Unknown);
    assert_eq!(s.destinations.len(), 1);
    let p = s.unreadable.iter().find(|u| u.source == Source::Preferences).unwrap();
    assert_eq!(p.reason, Reason::PermissionDenied);

    // The preferences answer but destinationinfo does not: dates are still trusted, the mount state is not.
    let no_info = FakeRunner::new()
        .route("export /Library/Preferences/com.apple.TimeMachine", &prefs(&["2026-10-13T11:00:00Z"], 1, true))
        .route_failure("destinationinfo", 1, "tmutil: boom")
        .route("status -X", STATUS_IDLE)
        .route("listlocalsnapshotdates", "");
    let s = tm(&no_info);
    assert_eq!(s.status, Status::Recent);
    assert_eq!(s.destinations[0].mounted, None);
    assert!(s.unreadable.iter().any(|u| u.source == Source::DestinationInfo));
}

#[test]
fn garbage_output_is_unreadable_not_empty() {
    let fake = FakeRunner::new()
        .route("export /Library/Preferences/com.apple.TimeMachine", "this is not a plist")
        .route("destinationinfo", "<html>nope</html>")
        .route("status -X", "???")
        .route("listlocalsnapshotdates", "");
    let s = tm(&fake);
    assert_eq!(s.status, Status::Unknown);
    assert!(s.unreadable.iter().all(|u| u.reason == Reason::Unavailable));
    assert_eq!(s.unreadable.len(), 3);
}

#[test]
fn latestbackup_runs_only_for_a_mounted_destination_and_wins_when_newer() {
    let fake = healthy("2026-10-12T08:00:00Z", true).route("latestbackup", "/Volumes/TM-Disk/2026-10-13-110000.backup\n");
    let s = tm(&fake);
    assert!(fake.calls().iter().any(|c| c.ends_with("tmutil latestbackup")));
    assert_eq!(s.last_backup_source, Some(Source::LatestBackup));
    assert_eq!(s.status, Status::Recent);
    assert!(s.last_backup_ms.unwrap() > at(2026, 10, 12, 8, 0));
}

#[test]
fn a_refused_latestbackup_falls_back_to_the_preferences_and_says_why() {
    let fake = healthy("2026-10-13T11:24:02Z", true).route_failure("latestbackup", 1, "Operation not permitted");
    let s = tm(&fake);
    assert_eq!(s.status, Status::Recent);
    assert_eq!(s.last_backup_source, Some(Source::Preferences));
    let u = s.unreadable.iter().find(|u| u.source == Source::LatestBackup).unwrap();
    assert_eq!(u.reason, Reason::PermissionDenied);
    // A mount failure is told apart from a permission failure.
    let fake = healthy("2026-10-13T11:24:02Z", true).route_failure("latestbackup", 1, "Failed to mount destination.");
    assert_eq!(tm(&fake).unreadable[0].reason, Reason::NotMounted);
}

#[test]
fn a_running_backup_is_flagged_without_hiding_the_last_one() {
    let fake = FakeRunner::new()
        .route("export /Library/Preferences/com.apple.TimeMachine", &prefs(&["2026-09-01T00:00:00Z"], 1, true))
        .route("destinationinfo", &info(false))
        .route("status -X", STATUS_RUNNING)
        .route("listlocalsnapshotdates", "");
    let s = tm(&fake);
    assert!(s.running);
    assert_eq!(s.percent, Some(0.42));
    assert_eq!(s.status, Status::Stale, "running does not make an old backup recent");
}

#[test]
fn classification_table() {
    let f = |destinations, prefs_readable, last: Option<i64>| Facts { destinations, prefs_readable, last_backup_ms: last };
    let n = now();
    assert_eq!(classify(&f(Some(0), true, None), n, 7), Status::NoDestination);
    assert_eq!(classify(&f(Some(0), false, None), n, 7), Status::NoDestination);
    assert_eq!(classify(&f(Some(1), true, Some(n - DAY)), n, 7), Status::Recent);
    assert_eq!(classify(&f(Some(1), true, Some(n - 8 * DAY)), n, 7), Status::Stale);
    assert_eq!(classify(&f(Some(2), true, None), n, 7), Status::NeverBackedUp);
    assert_eq!(classify(&f(Some(1), false, None), n, 7), Status::Unknown);
    assert_eq!(classify(&f(None, false, None), n, 7), Status::Unknown);
    // A clock set in the past never produces a negative age or a stale backup.
    assert_eq!(classify(&f(Some(1), true, Some(n + DAY)), n, 7), Status::Recent);
}

#[test]
fn parsers_on_their_own() {
    assert_eq!(timemachine::parse_latest_backup("/Volumes/X/Backups.backupdb/Host/2026-10-13-112402\n").is_some(), true);
    assert_eq!(timemachine::parse_latest_backup("Failed to mount backup destination, error: ..."), None);
    assert_eq!(timemachine::parse_latest_backup(""), None);
    assert_eq!(timemachine::parse_local_snapshots(LOCAL_SNAPSHOTS).len(), 2);
    assert!(timemachine::parse_local_snapshots("Snapshot dates for all disks:\n").is_empty());
    assert_eq!(timemachine::parse_status(STATUS_IDLE).unwrap(), timemachine::Running { running: false, percent: None });
    assert!(timemachine::parse_destination_info("No destinations configured.").unwrap().is_empty());
}

// ---- iCloud

const ACCOUNTS: &str = r#"<?xml version="1.0"?><plist version="1.0"><dict><key>Accounts</key><array><dict>
  <key>AccountDescription</key><string>someone@example.invalid</string>
  <key>AccountID</key><string>someone@example.invalid</string>
  <key>DisplayName</key><string>Some One</string>
  <key>AccountDSID</key><string>1234567890</string>
  <key>Services</key><array>
    <dict><key>Name</key><string>MOBILE_DOCUMENTS</string><key>Enabled</key><true/></dict>
    <dict><key>Name</key><string>PHOTOS</string><key>Enabled</key><false/></dict>
  </array></dict></array></dict></plist>"#;

fn cloud_runner(desktop: &str, documents: &str) -> FakeRunner {
    FakeRunner::new()
        .route("export MobileMeAccounts", ACCOUNTS)
        .route("FXICloudDriveDesktop", desktop)
        .route("FXICloudDriveDocuments", documents)
}

#[test]
fn icloud_signals_are_inferred_and_never_expose_the_account() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("Library/Mobile Documents/com~apple~CloudDocs")).unwrap();
    let s = icloud::read(&cloud_runner("1\n", "0\n"), home.path());
    assert!(s.drive_folder_present && s.inferred);
    assert_eq!((s.signed_in, s.drive_enabled), (Some(true), Some(true)));
    assert_eq!((s.desktop_synced, s.documents_synced), (Some(true), Some(false)));
    assert!(s.unreadable.is_empty());
    let json = serde_json::to_string(&s).unwrap();
    for private in ["someone@example", "Some One", "1234567890"] {
        assert!(!json.contains(private), "the state leaked account data: {private}");
    }
}

#[test]
fn icloud_when_signed_out_unset_or_unreadable() {
    let home = tempfile::tempdir().unwrap();
    // Signed out: an empty preferences file.
    let out = FakeRunner::new()
        .route("export MobileMeAccounts", r#"<?xml version="1.0"?><plist version="1.0"><dict/></plist>"#)
        .route_failure("FXICloudDriveDesktop", 1, "The domain/default pair of (com.apple.finder, FXICloudDriveDesktop) does not exist")
        .route_failure("FXICloudDriveDocuments", 1, "The domain/default pair of (com.apple.finder, FXICloudDriveDocuments) does not exist");
    let s = icloud::read(&out, home.path());
    assert!(!s.drive_folder_present);
    assert_eq!((s.signed_in, s.drive_enabled), (Some(false), Some(false)));
    assert_eq!((s.desktop_synced, s.documents_synced), (Some(false), Some(false)), "a key never set means off");
    assert!(s.unreadable.is_empty());
    // A real failure is reported, and the value stays unknown.
    let blind = FakeRunner::new().route_failure("defaults", 1, "Operation not permitted");
    let s = icloud::read(&blind, home.path());
    assert_eq!((s.signed_in, s.drive_enabled, s.desktop_synced), (None, None, None));
    assert_eq!(s.unreadable.len(), 3);
    assert!(s.unreadable.iter().all(|u| u.reason == Reason::PermissionDenied));
}

#[test]
fn the_whole_state_carries_the_full_disk_access_status() {
    let home = tempfile::tempdir().unwrap();
    let fake = healthy("2026-10-13T11:24:02Z", false)
        .route("export MobileMeAccounts", ACCOUNTS)
        .route("FXICloudDrive", "1\n");
    let state = read_state(&fake, home.path(), now(), 7, AccessStatus::Denied);
    assert_eq!(state.full_disk_access, AccessStatus::Denied);
    assert_eq!((state.stale_days, state.read_at), (7, now()));
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["timeMachine"]["status"], "recent");
    assert_eq!(json["fullDiskAccess"], "denied");
}
