# M13 — Backup Guard

**Status: implemented** (B1 reader, B2 gate, B3 exclusions). Depends on M12 (artifacts and unpushed repositories).

## Why

Before any batch cleanup the app should show whether the Mac has a recent backup and which
repositories hold work that exists only here. Without that, the user decides blind.

## Design

- Time Machine is read from sources with no side effects:
  - the preferences file, through `defaults export /Library/Preferences/com.apple.TimeMachine -`:
    the destinations, `AutoBackup`, and `SnapshotDates`, the completed backups macOS remembers.
    It needs no disk connected;
  - `tmutil destinationinfo -X`, `status -X` and `listlocalsnapshotdates`;
  - `tmutil latestbackup` only when a destination is already mounted, because it tries to mount
    the disk. `listbackups` is never used: it needs Full Disk Access and a mounted disk.
- States: no destination, recent, stale, never backed up, unknown. "Stale" means older than 7 days
  (the `backup.staleDays` setting, 1 to 90). Whether the disk is mounted, whether a backup is running
  and whether automatic backups are off are reported next to the state, not folded into it.
- Local snapshots on the startup disk are counted but are not a backup: they disappear with the disk.
- iCloud Drive is inferred from local signals (the Drive folder, the signed-in services, the Finder
  keys for Desktop and Documents) and labelled "inferido". Only service names and on/off flags are
  read; account details are never read, stored or sent.
- A read that fails (no Full Disk Access, disk not mounted) is reported as unreadable with the reason.
  An unreadable state never counts as "all good".
- The gate has three levels, decided on the Rust side from fresh reads:
  - `ok`: a recent completed backup and nothing at risk. No question is asked.
  - `warn`: a backup exists but something deserves a look: repositories with work only on this Mac,
    automatic backups off, repositories git could not read, or a state that cannot be verified while
    nothing is at risk. The user acknowledges.
  - `strong`: no destination, no recent backup, never backed up, or a state that cannot be verified
    while work exists only on this Mac. The user types `CONFIRMO` (case and spaces ignored).
  Repositories with unpushed work only make the gate `strong` when no backup is known to protect them;
  with a recent backup they are a warning. iCloud is shown but never counts as a backup: it mirrors deletions.
- The UI asks through `requestBackupAck()` before a batch cleanup and sends the answer with the
  command. The command computes the gate again and refuses with the `backupGate` error kind if the
  answer is not enough, so skipping the dialog changes nothing. A test reads the command sources and
  fails if a gated command stops taking the acknowledgement or checks it after changing anything.
- Gated: Smart Care run, cleanup rules run, leftovers removal, duplicates removal and regenerable
  folder removal. Not gated, on purpose: moving a selection to the Trash from a file list, uninstalling
  one app, and emptying the Trash. Those are single, reviewed, reversible actions the user picked item
  by item; the gate is for batches.
- Time Machine exclusions are the one place the app changes a system setting, so they are narrow:
  - only folders the regenerable-folder rules still recognise right now, checked on the server with the
    same check as before a removal, and never folders with versioned files;
  - only the sticky form, `tmutil addexclusion <path>`: it marks the folder itself, works without root
    and without Full Disk Access (checked on this Mac). The fixed-path form (`-p`) needs root and is
    never used. The mark covers everything inside the folder;
  - each path goes through the safety policy (`check_mutation`), the result is confirmed with
    `tmutil isexcluded` (its exit code is always 0, the answer is in the text), and every change is
    recorded in `tm_exclusions` and in the operation log;
  - the list shown first has only folders nothing speaks against ticked; "review" ones are the
    user's call, because excluding a folder removes its protection;
  - only exclusions the app added can be undone (`removeexclusion`), and undoing is idempotent.
  The mark lives on the folder: if it is deleted and recreated (a clean `npm ci`) the exclusion is gone.
  The app's list shows each one as still in effect, back in the backup, or gone.

## Threat model

| Threat | Mitigation |
| --- | --- |
| False "backup ok" | unreadable is warn at best; labelled as inferred |
| Changing backup settings without consent | exclusions only from an approved previewed list; reversible |
| UI bypasses the gate | acknowledgement checked in the command |

## Deliveries

| # | Delivery | State |
| --- | --- | --- |
| B1 | Time Machine and iCloud reader | done |
| B2 | Backup gate (Rust and dialog) | done |
| B3 | Time Machine exclusions | done |
