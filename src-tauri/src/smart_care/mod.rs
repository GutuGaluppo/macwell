//! Smart Care: runs the existing scanners in one pass and turns their results into one reviewable
//! plan with deterministic recommendations (no ML). Nothing runs until the user reviews the plan.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::applications::leftovers::{find_orphans, installed_bundle_ids, OrphanGroup};
use crate::applications::{list_apps, running_executables, AppInfo};
use crate::cleanup::downloads::{self, DownloadGroup, DownloadItem};
use crate::cleanup::trash_bin::{self, TrashSummary};
use crate::dev::git::Git;
use crate::dev::report::{build as build_dev_report, DevReport};
use crate::duplicates::{DuplicateCandidates, DuplicateGroup, DuplicateSearch};
use crate::error::{AppError, AppResult};
use crate::exec::RealRunner;
use crate::filesystem::scanner::{scan, ScanEvent, ScanOptions};
use crate::storage::finder::{DateField, FileFilter, FileFinder};
use crate::storage::volumes;
use crate::storage::Fanout;
use crate::types::FileEntry;

const DAY_MS: i64 = 86_400_000;
pub const UNUSED_APP_DAYS: i64 = 180;
/// Duplicates below this size are left to the Duplicates screen (Smart Care stays quick).
const DUPLICATE_MIN: u64 = 10_000_000;
const LARGE_OLD_MIN: u64 = 500_000_000;
const LARGE_OLD_DAYS: u32 = 365;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub id: String,
    /// "high" only for low disk space; "suggest" for everything else.
    pub level: String,
    pub title: String,
    pub detail: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCareReport {
    pub disk_total: u64,
    pub disk_free: u64,
    pub trash: TrashSummary,
    /// Downloads items, with old installers preselected.
    pub downloads: Vec<DownloadItem>,
    pub duplicates: Vec<DuplicateGroup>,
    /// Large files not modified in a year: only for review, never preselected.
    pub large_old: Vec<FileEntry>,
    pub unused_apps: Vec<AppInfo>,
    pub leftovers: Vec<OrphanGroup>,
    /// Repositories and regenerable folders under the user's project folders; `None` when no project
    /// folder was ever added.
    pub dev: Option<DevReport>,
    pub recommendations: Vec<Recommendation>,
    pub duration_ms: u64,
}

/// Input of the rules: plain numbers, so they are easy to test.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    pub disk_total: u64,
    pub disk_free: u64,
    pub trash_bytes: u64,
    pub trash_readable: bool,
    pub installers_count: usize,
    pub old_installers_bytes: u64,
    pub duplicate_wasted: u64,
    pub large_old_count: usize,
    pub large_old_bytes: u64,
    pub unused_apps: usize,
    pub unused_apps_bytes: u64,
    pub leftovers_safe_bytes: u64,
    /// Repositories holding work that exists only on this Mac.
    pub dev_unpushed_repos: usize,
    /// Regenerable folders marked safe, so nothing that needs a look is counted.
    pub dev_artifact_bytes: u64,
}

const GB: u64 = 1_000_000_000;

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9).replace('.', ",")
}

/// Deterministic recommendations, most important first.
pub fn recommend(f: &Facts) -> Vec<Recommendation> {
    let mut out = Vec::new();
    let rec = |id: &str, level: &str, title: String, detail: String, bytes: u64| Recommendation { id: id.into(), level: level.into(), title, detail, bytes };
    if f.disk_total > 0 && (f.disk_free as f64) < f.disk_total as f64 * 0.10 {
        out.push(rec("lowDisk", "high", "Pouco espaço livre".into(), format!("Restam {} ({:.0}% do disco). Abaixo de 10% o macOS pode ficar lento e falhar em atualizações.", gb(f.disk_free), f.disk_free as f64 / f.disk_total as f64 * 100.0), 0));
    }
    if f.trash_readable && f.trash_bytes > 2 * GB {
        out.push(rec("emptyTrash", "suggest", "Esvaziar a Lixeira".into(), format!("A Lixeira ocupa {}.", gb(f.trash_bytes)), f.trash_bytes));
    }
    if f.old_installers_bytes > GB || f.installers_count > 10 {
        out.push(rec("installers", "suggest", "Revisar instaladores em Downloads".into(), format!("{}; os antigos somam {}.", plural(f.installers_count, "instalador", "instaladores"), gb(f.old_installers_bytes)), f.old_installers_bytes));
    }
    if f.duplicate_wasted > GB {
        out.push(rec("duplicates", "suggest", "Remover cópias duplicadas".into(), format!("Arquivos idênticos de mais de 10 MB ocupam {} a mais.", gb(f.duplicate_wasted)), f.duplicate_wasted));
    }
    if f.large_old_count > 0 && f.large_old_bytes > 2 * GB {
        out.push(rec("largeOld", "suggest", "Arquivos grandes esquecidos".into(), format!("{} de mais de 500 MB sem modificação há mais de um ano: {}.", plural(f.large_old_count, "arquivo", "arquivos"), gb(f.large_old_bytes)), f.large_old_bytes));
    }
    if f.unused_apps >= 3 {
        out.push(rec("unusedApps", "suggest", "Apps sem uso".into(), format!("{} sem abrir há mais de 6 meses ({}).", plural(f.unused_apps, "app", "apps"), gb(f.unused_apps_bytes)), f.unused_apps_bytes));
    }
    if f.leftovers_safe_bytes > 200_000_000 {
        out.push(rec("leftovers", "suggest", "Restos de apps desinstalados".into(), format!("Caches e preferências de apps que não estão mais no Mac somam {}.", gb(f.leftovers_safe_bytes)), f.leftovers_safe_bytes));
    }
    if f.dev_unpushed_repos > 0 {
        out.push(rec(
            "devUnpushed",
            "suggest",
            format!("Trabalho não enviado em {}", plural(f.dev_unpushed_repos, "repositório", "repositórios")),
            "Parte do que está lá só existe neste Mac. Faça um snapshot em Projetos antes de limpar.".into(),
            0,
        ));
    }
    if f.dev_artifact_bytes > GB {
        out.push(rec(
            "devArtifacts",
            "suggest",
            "Pastas dos projetos que dá para recriar".into(),
            format!("node_modules, target e similares somam {} e voltam com um comando.", gb(f.dev_artifact_bytes)),
            f.dev_artifact_bytes,
        ));
    }
    // Low disk space first; then work that would be lost (it matters whatever its size); then by size.
    out.sort_by_key(|r| (r.level != "high", r.id != "devUnpushed", std::cmp::Reverse(r.bytes)));
    out
}

pub fn run(home: &Path, base: &ScanOptions, cancel: &Arc<AtomicBool>, dev_roots: &[PathBuf], emit: &mut dyn FnMut(ScanEvent)) -> AppResult<SmartCareReport> {
    let started = std::time::Instant::now();
    let total_stages: u64 = if dev_roots.is_empty() { 6 } else { 7 };
    let stage = |emit: &mut dyn FnMut(ScanEvent), name: &str, n: u64| emit(ScanEvent::Stage { stage: name.into(), done: n, total: total_stages });
    let check = || if cancel.load(Ordering::Relaxed) { Err(AppError::Cancelled) } else { Ok(()) };

    stage(emit, "trash", 0);
    let trash = trash_bin::summary(&home.join(".Trash"))?;
    check()?;

    stage(emit, "downloads", 1);
    let dl_options = ScanOptions { root: home.join("Downloads"), ..base.clone() };
    let downloads = if dl_options.root.is_dir() { downloads::analyze(&dl_options, cancel, &mut |_| {})?.0 } else { Vec::new() };
    check()?;

    // One scan of the home folder feeds both the duplicate and the large-old-file finders.
    stage(emit, "home", 2);
    let now = crate::db::now_ms();
    let mut dup = DuplicateCandidates::new(DUPLICATE_MIN);
    let mut finder = FileFinder::new(
        FileFilter { min_size: LARGE_OLD_MIN, older_than_days: Some(LARGE_OLD_DAYS), date_field: DateField::Modified, exclude_library: true, ..Default::default() },
        home,
        home,
        now,
        200,
    );
    let mut exclusions = base.exclusions.clone();
    exclusions.push(home.join("Library"));
    exclusions.push(home.join(".Trash"));
    let home_options = ScanOptions { root: home.to_path_buf(), exclusions, ..base.clone() };
    let stats = scan(&home_options, cancel, &mut Fanout(vec![&mut dup, &mut finder]), &mut *emit)?;
    if stats.cancelled {
        return Err(AppError::Cancelled);
    }
    stage(emit, "duplicates", 3);
    let (duplicates, _) = DuplicateSearch { home, cancel }.run(dup, &mut |_| {})?;
    let large_old = finder.finish().matches;
    check()?;

    stage(emit, "apps", 4);
    let cutoff = now - UNUSED_APP_DAYS * DAY_MS;
    let unused_apps: Vec<AppInfo> = list_apps(home, &running_executables(), cancel, &|_, _| {})
        .into_iter()
        .filter(|a| !a.system_app && !a.running && a.last_used_at.is_some_and(|t| t < cutoff))
        .collect();
    check()?;

    stage(emit, "leftovers", 5);
    let installed: HashSet<String> = installed_bundle_ids(home);
    let full_disk_access = crate::filesystem::probe_full_disk_access(home) == crate::filesystem::AccessStatus::Granted;
    let leftovers = find_orphans(&home.join("Library"), &installed, full_disk_access);
    check()?;

    // Optional: only when the user chose project folders. Read-only, like every other stage.
    let dev = if dev_roots.is_empty() {
        None
    } else {
        stage(emit, "projects", 6);
        let runner = RealRunner;
        let report = build_dev_report(dev_roots, &Git::new(&runner), &HashMap::new(), cancel, &mut |_| {});
        check()?;
        Some(report)
    };
    stage(emit, "done", total_stages);

    let root = volumes::list().into_iter().find(|v| v.is_root);
    let facts = Facts {
        disk_total: root.as_ref().map(|v| v.total).unwrap_or(0),
        disk_free: root.as_ref().map(|v| v.free).unwrap_or(0),
        trash_bytes: trash.bytes,
        trash_readable: trash.readable,
        installers_count: downloads.iter().filter(|d| matches!(d.group, DownloadGroup::OldInstaller | DownloadGroup::Installer)).count(),
        old_installers_bytes: downloads.iter().filter(|d| d.group == DownloadGroup::OldInstaller).map(|d| d.entry.size_logical).sum(),
        duplicate_wasted: duplicates.iter().map(|g| g.wasted).sum(),
        large_old_count: large_old.len(),
        large_old_bytes: large_old.iter().map(|f| f.size_logical).sum(),
        unused_apps: unused_apps.len(),
        unused_apps_bytes: unused_apps.iter().map(|a| a.size).sum(),
        leftovers_safe_bytes: leftovers.iter().flat_map(|g| &g.items).filter(|i| i.selected).map(|i| i.size).sum(),
        dev_unpushed_repos: dev.as_ref().map_or(0, |d| d.repos.iter().filter(|r| r.is_unsynced()).count()),
        dev_artifact_bytes: dev.as_ref().map_or(0, |d| d.artifacts.iter().filter(|a| a.confidence == crate::types::Confidence::Safe).map(|a| a.size).sum()),
    };
    Ok(SmartCareReport {
        disk_total: facts.disk_total,
        disk_free: facts.disk_free,
        recommendations: recommend(&facts),
        trash,
        downloads,
        duplicates,
        large_old,
        unused_apps,
        leftovers,
        dev,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert!(recommend(&Facts::default()).is_empty());
        let f = Facts {
            disk_total: 1000 * GB,
            disk_free: 50 * GB,
            trash_bytes: 3 * GB,
            trash_readable: true,
            installers_count: 12,
            old_installers_bytes: 500_000_000,
            duplicate_wasted: 2 * GB,
            unused_apps: 4,
            unused_apps_bytes: 6 * GB,
            ..Default::default()
        };
        let ids: Vec<_> = recommend(&f).into_iter().map(|r| r.id).collect();
        // Low disk space first, then by size.
        assert_eq!(ids, vec!["lowDisk", "unusedApps", "emptyTrash", "duplicates", "installers"]);
        // Thresholds are strict: exactly 2 GB in the Trash is not enough, and an unreadable Trash
        // is never suggested.
        let f = Facts { trash_bytes: 2 * GB, trash_readable: true, ..Default::default() };
        assert!(recommend(&f).is_empty());
        let f = Facts { trash_bytes: 9 * GB, trash_readable: false, ..Default::default() };
        assert!(recommend(&f).is_empty());
    }

    #[test]
    fn project_recommendations() {
        // Nothing to say without projects, or with small regenerable folders.
        assert!(recommend(&Facts { dev_artifact_bytes: GB, ..Default::default() }).is_empty());
        let f = Facts { dev_unpushed_repos: 2, dev_artifact_bytes: 14 * GB, trash_bytes: 3 * GB, trash_readable: true, ..Default::default() };
        let recs = recommend(&f);
        let ids: Vec<_> = recs.iter().map(|r| r.id.as_str()).collect();
        // Work that would be lost comes before any space to reclaim, whatever its size.
        assert_eq!(ids, ["devUnpushed", "devArtifacts", "emptyTrash"]);
        assert!(recs[0].title.contains("2 repositórios"));
        assert!(recommend(&Facts { dev_unpushed_repos: 1, ..Default::default() })[0].title.contains("1 repositório"));
        // Only low disk space outranks it.
        let f = Facts { disk_total: 1000 * GB, disk_free: 10 * GB, dev_unpushed_repos: 1, ..Default::default() };
        assert_eq!(recommend(&f).iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["lowDisk", "devUnpushed"]);
    }
}
