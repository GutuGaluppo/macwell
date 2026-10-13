# M12 — Developer Care (Projetos)

**Status: implemented.** Foundations (command runner, schema v2, `check_mutation`) are in place.
Each delivery below is marked when it lands.

## Why

For people who program, the risky files are not caches: they are repositories with work that was
never pushed, and the regenerable folders (`node_modules`, `target`…) that make a home folder huge.
This module reads repositories, sizes the regenerable artifacts and can snapshot work in progress
without touching the working tree.

## Design

- Roots are folders the user chooses (`dev_roots`). Discovery never follows symlinks and never
  enters `node_modules`, `.git`, `target`, `.next`, `dist`, `build`, `.turbo`, `DerivedData`, `.venv`,
  `Pods`, `vendor`.
- Git is read through `/usr/bin/git` via `exec::CommandRunner` with a fixed environment
  (`GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, `core.fsmonitor=false`, `core.hooksPath=/dev/null`).
  Ahead/behind uses only what is already local: no fetch.
- Regenerable artifacts come from `rules/dev/artifacts.json` (sibling marker → type). No lockfile
  means `review`, never pre-selected. Removal revalidates on the server and goes to the Trash.
- Secret check runs before any snapshot, only on files that would enter it, text files up to 1 MB.
  It reports path and kind, never content.
- Snapshot WIP uses a temporary index outside the repo: `read-tree`, `add -A`, `write-tree`,
  `commit-tree`, `update-ref` on a new `refs/heads/wip/snapshot-<date>`. HEAD, the current branch,
  the real index and the working tree are not touched. Only new objects and one new ref are written.
- Blockers, all shown in the preview: untracked secrets not yet excluded; a new file above 50 MB or
  new files above 500 MB in total; a clean filter (for example Git LFS) both configured and used;
  nothing to save; no free branch name for today. Secrets already tracked only produce a warning.
  Excluded tracked files keep the version HEAD has. Hooks, signing and fsmonitor never run.
- Push is a separate, confirmed action: one ref, never `--force`. Unknown or public visibility
  shows a warning. Visibility is checked online only when the user asks (GitHub only).

## Removing regenerable folders

Removal is a separate, explicit step. The list the user approves is only a request: on the Rust side
each folder is found again by the same rule that listed it (marker next to it, a real directory, no
symlink, not inside another artifact or repository) and git is asked about it. Folders with
versioned files, or whose status git cannot confirm, are refused. A missing lockfile or a
folder git does not ignore is the user's call, because the review screen showed it. Accepted
folders go through `move_to_trash` with the repository as scan root, so the safety policy, the
ignore list and the operation log all apply. Smart Care adds a "Projetos" stage only when the user
has added project folders, preselects only the folders nothing speaks against, and ranks "work not
pushed" above any space to reclaim.

The backup gate (M13) is not wired into these actions yet: B2 will require the acknowledgement in
`remove_dev_artifacts` and in `run_smart_care`.

## Threat model

| Threat | Mitigation |
| --- | --- |
| Secrets end up in `.git` objects and are pushed | scan first; block unless excluded; warn on push |
| Hooks, fsmonitor or clean filters run code | hooks and fsmonitor disabled; refuse when `filter=` is configured |
| Symlink leads discovery outside the root | `symlink_metadata` everywhere; nothing is followed |
| UI sends a forged path to remove | server revalidates marker, location and `SafetyPolicy::check` |
| Data leaves the Mac | no network except the explicit visibility check and the confirmed push |

## Deliveries

| # | Delivery | State |
| --- | --- | --- |
| 0 | Command runner, schema v2, `check_mutation` | done |
| A1 | Repository discovery | done |
| A2 | Repository state | done |
| A3 | Regenerable artifacts | done |
| A4 | Secret scan | done |
| A5 | Snapshot WIP | done |
| A6 | Push and visibility | done |
| A7 | Projects UI | done |
| A8 | Smart Care integration | done |
