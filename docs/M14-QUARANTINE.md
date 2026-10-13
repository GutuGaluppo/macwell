# M14 — Quarantine and report

**Status: implemented** (C1 quarantine, C2 report, C3b Homebrew). C3a was tested and dropped: see below.

## Why

The Trash is the default and stays so. Quarantine is an optional, dated holding folder with a CSV
log, for people who want to remove things from sight while keeping a clear way back.

## Design

- A choice in Settings ("Ao remover itens"): the Trash (default) or the quarantine. It applies to every
  batch removal and review (Smart Care, cleanup rules, leftovers, duplicates, regenerable folders, the
  file lists) through one dispatcher, `commands::remove_items`, so there is no second path around the
  safety policy. Uninstalling an app stays on the Trash.
- Moving is `rename(2)` on one volume. A test checks that the inode is the same afterwards: nothing is
  copied. An item on another volume is refused ("use a Lixeira"); there is no copy followed by delete.
- Destination: the app data folder (`.../Quarantine/YYYY-MM-DD/<id>-<name>`) for the home volume, and
  `<volume root>/.OrganizeMyMac-Quarantine/YYYY-MM-DD/` for another volume, each with its own
  `quarantine.csv` (id, origin, destination, size, date). Names are shortened and ids are unique, so
  items never collide.
- Each item is validated by `SafetyPolicy::check` first, exactly as for the Trash. Anything inside a
  folder named `.OrganizeMyMac-Quarantine`, or inside the app data folder, is the app's own: no other
  flow can remove or change it. Nothing is ever moved without a record: if the database or the CSV
  cannot be written, the move is undone.
- Quarantined items still take space. They are logged as actions, not as "space freed".
- Restoring puts an item back at its origin, or in a folder the user picks. It uses an exclusive
  rename (`renamex_np` with `RENAME_EXCL`), so it never overwrites, even if something appears between
  the check and the move. If the original folder is gone it is not recreated. Both restoring and
  emptying need an explicit confirmation, enforced in the command.
- "Emptying" sends the items to the Trash. The app never deletes anything permanently on its own. A
  record that points outside the quarantine is refused for restoring and for emptying.
- The Quarantine page lists items by day with their origin, size and whether they are still in
  place, and offers restore (with "another folder" when the place is taken) and "send to the Trash".

## Report

- Markdown or CSV, with the next decision first, then the backup state, what was done in a chosen
  period (totals that always count everything, and up to 200 individual actions) and what is still
  open. It is built on the Rust side from the app's own records and a fresh read every time: the UI
  never sends content to write, only where and in which format.
- "What is open" and "next decision" are deterministic. The order is fixed: protection first (no
  destination, an old or unknown backup), then work that exists only on this Mac (repositories, local
  snapshots), then housekeeping (quarantine, lost Time Machine exclusions, automatic backups off).
- File names come from the disk, so they are never trusted. In Markdown a name is shown as inline code,
  with pipes, newlines and brackets neutralised so it cannot break a table or become a link. In CSV,
  a cell a spreadsheet could read as a formula (`=`, `+`, `-`, `@`, tab, return) gets a leading
  apostrophe.
- Writing is narrow: an absolute path with the right extension, in an existing folder, allowed by the
  safety policy (not system places, not the app's own data, not ignored folders); never through a
  symbolic link; written beside the target and moved into place, so a failure leaves no partial file;
  with permissions 0600. It never replaces an existing file unless told to, and the `exists` error
  lets the screen ask first. Nothing is sent anywhere.

## Extras

### Homebrew analysis (C3b, read only)

- The only command the module can run is `brew outdated --json=v2`, with auto-update, analytics and
  hints switched off, so nothing is updated, downloaded or reported. A test reads the module source
  and fails if any mutating verb appears or a second command is added; it also checks the
  environment. The app never runs `brew upgrade`: it shows the command for the user to copy.
- Packages come out riskiest first. The risk is how far the version jumps (major, minor, patch;
  compared as numbers, so `1.10` is newer than `1.9`), raised for runtimes and databases that
  projects depend on (`node`, `python@3.x`, `postgresql@14`…), lowered for apps (casks), which no
  project links against, and "pinned" for packages `brew upgrade` skips. A jump that cannot be
  compared is never "low".
- Which projects use a package: a `Brewfile` is exact; everything else is an estimate from files that
  identify the toolchain (`package.json` → `node`, `Cargo.toml` → `rust`, `requirements.txt` →
  `python`, `.tool-versions`…), looked at a few folders deep, never inside `node_modules` or similar,
  never through symlinks and never inside nested repositories. The screen labels the estimates.

### `.metadata_never_index` in `node_modules` (C3a): dropped

The plan was to drop an empty `.metadata_never_index` file into `node_modules` so Spotlight skips it.
It was tested on macOS 26.6 with three folders holding uniquely named files: one with the marker, one
plain and one named `*.noindex`. After indexing, the marker folder was indexed exactly like the plain
one, while the `.noindex` folder was not (the control that proves the test can see an exclusion).
The marker is only honoured at the root of a volume, so an app button for it would have done nothing
while looking as if it had. It was not built. The folder suffix `.noindex` works but means renaming
`node_modules`, which breaks tooling, and the Search Privacy list in System Settings is the one
reliable way to exclude a folder: that is a manual step.

## Deliveries

| # | Delivery | State |
| --- | --- | --- |
| C1 | Quarantine | done |
| C2 | Report export | done |
| C3a | `.metadata_never_index` | dropped (does not work in subfolders) |
| C3b | Homebrew outdated (read only) | done |
