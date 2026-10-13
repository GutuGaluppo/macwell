//! The report: what the app did, what is still open, and the one decision to take next. Built from
//! the app's own local records and a fresh read of the backup state, written only where the user
//! chooses, and never sent anywhere.
//!
//! Names of files come from the disk and cannot be trusted: in Markdown they are escaped so they
//! cannot become links or HTML, and in CSV a cell that a spreadsheet could take for a formula is
//! neutralised.

pub mod export;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::backup::gate::BackupGate;
use crate::backup::timemachine::Status;
use crate::db::{OperationRecord, SnapshotRecord};

/// Longest list of individual actions in a report; the totals above it always count everything.
pub const MAX_LINES: usize = 200;
const MAX_ITEMS: usize = 10;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportFormat {
    Markdown,
    Csv,
}

impl ReportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ReportFormat::Markdown => "md",
            ReportFormat::Csv => "csv",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneGroup {
    pub action: String,
    pub label: String,
    pub total: u32,
    pub ok: u32,
    /// Size of what was actually done (failed attempts do not count).
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneLine {
    pub when_ms: i64,
    pub action: String,
    pub label: String,
    pub path: String,
    pub size: u64,
    pub ok: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotLine {
    pub when_ms: i64,
    pub repo: String,
    pub ref_name: String,
    pub pushed: bool,
}

/// One example under a pending item: a path or name, and a note about it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingLine {
    pub text: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingItem {
    pub code: &'static str,
    pub title: String,
    pub detail: String,
    pub count: u32,
    pub bytes: u64,
    /// A few examples, at most ten.
    pub items: Vec<PendingLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextDecision {
    pub code: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub status: Status,
    pub last_backup_ms: Option<i64>,
    pub auto_backup: Option<bool>,
    pub stale_days: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub generated_at: i64,
    pub period_days: u32,
    pub groups: Vec<DoneGroup>,
    pub recent: Vec<DoneLine>,
    /// How many more actions the period has than `recent` lists.
    pub recent_omitted: u32,
    pub snapshots: Vec<SnapshotLine>,
    pub backup: BackupSummary,
    pub pending: Vec<PendingItem>,
    pub next: NextDecision,
}

pub struct ReportInput {
    pub now_ms: i64,
    pub period_days: u32,
    /// Newest first, as the operation log returns them.
    pub operations: Vec<OperationRecord>,
    pub snapshots: Vec<SnapshotRecord>,
    /// Exclusions the app added whose folder is back in the backup.
    pub lost_exclusions: Vec<String>,
    /// What the quarantine still holds: (origin, size, still in place).
    pub quarantine: Vec<(String, u64, bool)>,
    pub gate: BackupGate,
}

pub fn action_label(action: &str) -> String {
    match action {
        "trash" => "Movido para a Lixeira",
        "uninstall" => "App desinstalado (Lixeira)",
        "emptyTrash" => "Lixeira esvaziada",
        "quarantine" => "Movido para a Quarentena",
        "quarantineRestore" => "Restaurado da Quarentena",
        "quarantineEmpty" => "Enviado da Quarentena para a Lixeira",
        "tmExclude" => "Excluído do backup do Time Machine",
        "tmUnexclude" => "Devolvido ao backup do Time Machine",
        "snapshot" => "Snapshot do trabalho criado",
        "push" => "Snapshot enviado a um remote",
        "report" => "Relatório exportado",
        other => return other.to_string(),
    }
    .to_string()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn examples(items: impl Iterator<Item = PendingLine>) -> Vec<PendingLine> {
    items.take(MAX_ITEMS).collect()
}

fn line(text: impl Into<String>, note: Option<String>) -> PendingLine {
    PendingLine { text: text.into(), note }
}

fn repo_reason(code: &str) -> &str {
    match code {
        "uncommitted" => "alterações sem commit",
        "unpushedCommits" => "commits não enviados",
        "noUpstream" => "branch sem upstream",
        "noRemote" => "sem remote",
        other => other,
    }
}

fn source_label(s: crate::backup::Source) -> &'static str {
    use crate::backup::Source::*;
    match s {
        Preferences => "preferências do Time Machine",
        DestinationInfo => "destinos do Time Machine",
        Status => "estado do Time Machine",
        LatestBackup => "último backup no disco",
        LocalSnapshots => "instantâneos locais",
        MobileMeAccounts => "contas do iCloud",
        FinderPreferences => "Mesa e Documentos no iCloud",
    }
}

fn reason_label(r: crate::backup::Reason) -> &'static str {
    use crate::backup::Reason::*;
    match r {
        PermissionDenied => "o macOS recusou a leitura (em geral falta o Acesso Total ao Disco)",
        NotMounted => "o disco de backup não está conectado",
        Unavailable => "a leitura falhou ou veio num formato desconhecido",
    }
}

pub fn build(input: ReportInput) -> Report {
    let since = input.now_ms - input.period_days as i64 * DAY_MS;
    let ops: Vec<&OperationRecord> = input.operations.iter().filter(|o| o.ts >= since).collect();

    // Totals over everything in the period, in the order actions first appear (newest first).
    let mut groups: Vec<DoneGroup> = Vec::new();
    for o in &ops {
        let g = match groups.iter_mut().find(|g| g.action == o.action) {
            Some(g) => g,
            None => {
                groups.push(DoneGroup { action: o.action.clone(), label: action_label(&o.action), total: 0, ok: 0, bytes: 0 });
                groups.last_mut().unwrap()
            }
        };
        g.total += 1;
        if o.ok {
            g.ok += 1;
            g.bytes += o.size;
        }
    }
    let recent: Vec<DoneLine> = ops
        .iter()
        .take(MAX_LINES)
        .map(|o| DoneLine { when_ms: o.ts, action: o.action.clone(), label: action_label(&o.action), path: o.path.clone(), size: o.size, ok: o.ok, detail: o.detail.clone() })
        .collect();
    let recent_omitted = ops.len().saturating_sub(recent.len()) as u32;

    let snapshots: Vec<SnapshotLine> = input
        .snapshots
        .iter()
        .filter(|s| s.created_at >= since)
        .map(|s| SnapshotLine { when_ms: s.created_at, repo: s.repo.clone(), ref_name: s.ref_name.clone(), pushed: s.pushed_at.is_some() })
        .collect();

    let tm = &input.gate.backup.time_machine;
    let backup = BackupSummary { status: tm.status, last_backup_ms: tm.last_backup_ms, auto_backup: tm.auto_backup, stale_days: input.gate.backup.stale_days };

    // ---- what is still open
    let mut pending: Vec<PendingItem> = Vec::new();
    let item = |code, title: String, detail: String, count: usize, bytes: u64, items: Vec<PendingLine>| PendingItem { code, title, detail, count: count as u32, bytes, items };
    match tm.status {
        Status::NoDestination => pending.push(item("noBackup", "O Time Machine não tem destino".into(), "Nada está sendo copiado por ele neste Mac.".into(), 0, 0, vec![])),
        Status::Stale => pending.push(item(
            "backupStale",
            "O último backup é antigo".into(),
            format!("O último backup concluído foi há {} dias; o limite é de {}.", tm.age_days.unwrap_or(0), input.gate.backup.stale_days),
            0,
            0,
            vec![],
        )),
        Status::NeverBackedUp => pending.push(item("noBackup", "Nenhum backup concluído".into(), "Há um destino do Time Machine, mas nenhum backup concluído é conhecido.".into(), 0, 0, vec![])),
        Status::Unknown => pending.push(item("backupUnknown", "Estado do backup desconhecido".into(), "O app não conseguiu ler o estado do Time Machine.".into(), 0, 0, vec![])),
        Status::Recent => {}
    }
    if tm.auto_backup == Some(false) {
        pending.push(item("autoBackupOff", "Backups automáticos desligados".into(), "Um novo backup só acontece se alguém o disparar.".into(), 0, 0, vec![]));
    }
    let unreadable: Vec<PendingLine> =
        tm.unreadable.iter().chain(input.gate.backup.icloud.unreadable.iter()).map(|u| line(source_label(u.source), Some(reason_label(u.reason).to_string()))).collect();
    if !unreadable.is_empty() {
        pending.push(item("unreadable", "Há leituras que não foram possíveis".into(), "O relatório pode estar incompleto nestes pontos.".into(), unreadable.len(), 0, examples(unreadable.into_iter())));
    }
    if !input.gate.unpushed.is_empty() {
        let lines = input.gate.unpushed.iter().map(|r| line(r.path.display().to_string(), Some(r.reasons.iter().map(|c| repo_reason(c)).collect::<Vec<_>>().join(", "))));
        pending.push(item(
            "unpushedRepos",
            format!("{} com trabalho só neste Mac", plural(input.gate.unpushed.len(), "repositório", "repositórios")),
            "Alterações sem commit, commits não enviados ou branches sem destino.".into(),
            input.gate.unpushed.len(),
            0,
            examples(lines),
        ));
    }
    let local_snaps: Vec<&SnapshotRecord> = input.snapshots.iter().filter(|s| s.pushed_at.is_none()).collect();
    if !local_snaps.is_empty() {
        pending.push(item(
            "localSnapshots",
            format!("{} que só {} neste Mac", plural(local_snaps.len(), "snapshot", "snapshots"), if local_snaps.len() == 1 { "existe" } else { "existem" }),
            "Foram criados mas nunca enviados a um remote.".into(),
            local_snaps.len(),
            0,
            examples(local_snaps.iter().map(|s| line(s.ref_name.clone(), Some(format!("em {}", s.repo))))),
        ));
    }
    let held: Vec<&(String, u64, bool)> = input.quarantine.iter().collect();
    if !held.is_empty() {
        pending.push(item(
            "quarantine",
            format!("{} na Quarentena", plural(held.len(), "item", "itens")),
            "Continuam ocupando espaço: restaure ou envie à Lixeira.".into(),
            held.len(),
            held.iter().filter(|h| h.2).map(|h| h.1).sum(),
            examples(held.iter().map(|h| line(h.0.clone(), (!h.2).then(|| "não está mais na pasta".to_string())))),
        ));
    }
    if !input.lost_exclusions.is_empty() {
        pending.push(item(
            "lostExclusions",
            format!("{} voltou ao backup", plural(input.lost_exclusions.len(), "exclusão do Time Machine", "exclusões do Time Machine")),
            "A pasta foi apagada e recriada, e a marca de exclusão se perdeu.".into(),
            input.lost_exclusions.len(),
            0,
            examples(input.lost_exclusions.iter().map(|p| line(p.clone(), None))),
        ));
    }

    let next = decide_next(&pending, tm.status);
    Report { generated_at: input.now_ms, period_days: input.period_days, groups, recent, recent_omitted, snapshots, backup, pending, next }
}

/// One next step, in a fixed order of importance: protection first, then work that exists only here,
/// then housekeeping. Deterministic, so the same state always gives the same advice.
pub fn decide_next(pending: &[PendingItem], status: Status) -> NextDecision {
    let has = |code: &str| pending.iter().find(|p| p.code == code);
    let text = |code, text: String| NextDecision { code, text };
    if matches!(status, Status::NoDestination | Status::NeverBackedUp) {
        return text("backup", "Configure um destino do Time Machine e faça um backup antes de qualquer limpeza em lote.".into());
    }
    if status == Status::Stale {
        return text("backup", "Conecte o disco do Time Machine e faça um backup: o último está antigo.".into());
    }
    if status == Status::Unknown {
        return text("backup", "Descubra o estado do backup: o app não conseguiu ler o Time Machine (conceda o Acesso Total ao Disco ou confira o disco).".into());
    }
    if let Some(p) = has("unpushedRepos") {
        return text("snapshot", format!("Faça um snapshot de {}: o que está lá só existe neste Mac.", plural(p.count as usize, "repositório", "repositórios")));
    }
    if let Some(p) = has("localSnapshots") {
        return text("pushSnapshots", format!("Decida o que fazer com {} só local: enviar a um remote privado ou mantê-lo apenas neste Mac.", plural(p.count as usize, "snapshot", "snapshots")));
    }
    if let Some(p) = has("quarantine") {
        return text("quarantine", format!("Decida {} da Quarentena: restaurar ou enviar à Lixeira.", plural(p.count as usize, "o item", "os itens")));
    }
    if let Some(p) = has("lostExclusions") {
        return text("exclusions", format!("Refaça {} do Time Machine que se perderam.", plural(p.count as usize, "exclusão", "exclusões")));
    }
    if has("autoBackupOff").is_some() {
        return text("autoBackup", "Ligue os backups automáticos do Time Machine, se isso for o que você quer.".into());
    }
    text("none", "Nada pendente. Quando quiser, rode o Cuidado inteligente.".into())
}

// ---------------------------------------------------------------- rendering

fn when(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "—".into())
}

fn size(bytes: u64) -> String {
    let mut v = bytes as f64;
    for unit in ["bytes", "KB", "MB", "GB", "TB"] {
        if v < 1000.0 || unit == "TB" {
            return if unit == "bytes" { format!("{bytes} bytes") } else { format!("{v:.1} {unit}").replace('.', ",") };
        }
        v /= 1000.0;
    }
    unreachable!()
}

/// Text for a table cell: one line, no pipes, and nothing a Markdown reader could turn into HTML or a link.
fn md_text(s: &str) -> String {
    s.replace(['\n', '\r'], " ").replace('|', "\\|").replace('<', "&lt;").replace('>', "&gt;").replace('[', "\\[").replace(']', "\\]")
}

/// A path or name as inline code, safe inside a table.
fn md_code(s: &str) -> String {
    format!("`{}`", s.replace(['\n', '\r'], " ").replace('`', "'").replace('|', "\\|"))
}

pub fn to_markdown(r: &Report) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let _ = writeln!(o, "# Relatório do OrganizeMyMac\n");
    let _ = writeln!(o, "Gerado em {} · período: últimos {} dias · feito só com dados deste Mac, nada foi enviado a lugar nenhum.\n", when(r.generated_at), r.period_days);

    let _ = writeln!(o, "## Próxima decisão\n\n**{}**\n", md_text(&r.next.text));

    let _ = writeln!(o, "## Backup\n");
    let status = match r.backup.status {
        Status::Recent => "recente",
        Status::Stale => "antigo",
        Status::NoDestination => "sem destino",
        Status::NeverBackedUp => "nunca concluído",
        Status::Unknown => "desconhecido",
    };
    let _ = writeln!(o, "- Time Machine: {status}; último backup concluído: {}", r.backup.last_backup_ms.map_or("—".to_string(), when));
    let _ = writeln!(o, "- Backups automáticos: {}", match r.backup.auto_backup { Some(true) => "ligados", Some(false) => "desligados", None => "não foi possível saber" });
    let _ = writeln!(o, "- Limite para considerar antigo: {} dias\n", r.backup.stale_days);

    let _ = writeln!(o, "## O que foi feito\n");
    if r.groups.is_empty() {
        let _ = writeln!(o, "Nenhuma ação no período.\n");
    } else {
        let _ = writeln!(o, "| Ação | Tentativas | Concluídas | Tamanho |\n| --- | ---: | ---: | ---: |");
        for g in &r.groups {
            let _ = writeln!(o, "| {} | {} | {} | {} |", md_text(&g.label), g.total, g.ok, size(g.bytes));
        }
        let _ = writeln!(o, "\n### Ações, da mais recente para a mais antiga\n");
        let _ = writeln!(o, "| Quando | Ação | Caminho | Tamanho | Resultado |\n| --- | --- | --- | ---: | --- |");
        for l in &r.recent {
            let result = if l.ok { "ok".to_string() } else { format!("falhou: {}", md_text(l.detail.as_deref().unwrap_or("sem detalhe"))) };
            let _ = writeln!(o, "| {} | {} | {} | {} | {} |", when(l.when_ms), md_text(&l.label), md_code(&l.path), size(l.size), result);
        }
        if r.recent_omitted > 0 {
            let _ = writeln!(o, "\n… e mais {} ações no período, contadas nos totais acima.", r.recent_omitted);
        }
        let _ = writeln!(o);
    }
    if !r.snapshots.is_empty() {
        let _ = writeln!(o, "### Snapshots\n\n| Quando | Repositório | Branch | Enviado |\n| --- | --- | --- | --- |");
        for s in &r.snapshots {
            let _ = writeln!(o, "| {} | {} | {} | {} |", when(s.when_ms), md_code(&s.repo), md_code(&s.ref_name), if s.pushed { "sim" } else { "só neste Mac" });
        }
        let _ = writeln!(o);
    }

    let _ = writeln!(o, "## O que falta\n");
    if r.pending.is_empty() {
        let _ = writeln!(o, "Nada pendente.");
    }
    for p in &r.pending {
        let bytes = if p.bytes > 0 { format!(" ({})", size(p.bytes)) } else { String::new() };
        let _ = writeln!(o, "- **{}**{} — {}", md_text(&p.title), bytes, md_text(&p.detail));
        for it in &p.items {
            match &it.note {
                Some(n) => {
                    let _ = writeln!(o, "  - {} — {}", md_code(&it.text), md_text(n));
                }
                None => {
                    let _ = writeln!(o, "  - {}", md_code(&it.text));
                }
            }
        }
        if p.count as usize > p.items.len() && !p.items.is_empty() {
            let _ = writeln!(o, "  - … e mais {}", p.count as usize - p.items.len());
        }
    }
    o
}

/// A cell a spreadsheet could read as a formula gets a leading apostrophe.
fn csv_cell(s: &str) -> String {
    let guarded = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    if guarded.contains([',', '"', '\n', '\r']) { format!("\"{}\"", guarded.replace('"', "\"\"")) } else { guarded }
}

pub fn to_csv(r: &Report) -> String {
    let mut o = String::from("secao,tipo,quando,caminho,tamanho,resultado,detalhe\n");
    let mut row = |cells: [String; 7]| {
        o.push_str(&cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        o.push('\n');
    };
    row(["proxima".into(), r.next.code.into(), when(r.generated_at), String::new(), String::new(), String::new(), r.next.text.clone()]);
    for l in &r.recent {
        row(["feito".into(), l.action.clone(), when(l.when_ms), l.path.clone(), l.size.to_string(), if l.ok { "ok".into() } else { "falhou".into() }, l.detail.clone().unwrap_or_default()]);
    }
    for s in &r.snapshots {
        row(["snapshot".into(), s.ref_name.clone(), when(s.when_ms), s.repo.clone(), String::new(), if s.pushed { "enviado".into() } else { "so-local".into() }, String::new()]);
    }
    for p in &r.pending {
        if p.items.is_empty() {
            row(["pendente".into(), p.code.into(), String::new(), String::new(), p.bytes.to_string(), String::new(), format!("{} — {}", p.title, p.detail)]);
        }
        for it in &p.items {
            let detail = match &it.note {
                Some(n) => format!("{} — {}", p.title, n),
                None => p.title.clone(),
            };
            row(["pendente".into(), p.code.into(), String::new(), it.text.clone(), String::new(), String::new(), detail]);
        }
    }
    o
}

pub fn render(r: &Report, format: ReportFormat) -> String {
    match format {
        ReportFormat::Markdown => to_markdown(r),
        ReportFormat::Csv => to_csv(r),
    }
}
