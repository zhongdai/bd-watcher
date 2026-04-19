# bd-watcher — Design

**Date:** 2026-04-18
**Status:** Approved for implementation

## Summary

A lightweight Rust TUI that polls `bd graph --all --json` (or a single epic) on an interval and renders a dashboard of beads progress plus a live activity feed of status changes. Purpose: a TV dashboard for watching AI agents make progress through a stack of beads. Read-only — never writes to the Dolt database.

## Goals

- Glanceable from across a room (large, high-contrast rendering in TV mode).
- Interactive drill-in on a laptop (computer mode) — read-only detail panel.
- Lightweight dependencies: `ratatui`, `crossterm`, `tokio`, `serde`, `clap`, `anyhow`.
- Never mutate the bd DB — no `bd update`, `bd close`, `bd claim`, etc.

## Non-goals

- Creating, editing, or closing beads.
- Rendering a full DAG layout (unreadable on TV at scale).
- Jira/external sync.
- Filesystem watching of `.beads/` (we poll `bd` instead).

## CLI

```
bd-watcher [EPIC_ID] [OPTIONS]

Arguments:
  [EPIC_ID]              Optional epic id (e.g. sel3-42wn) to focus on.
                         Omitted → show all open components.

Options:
  --tv                   TV mode: read-only, no selection or detail panel.
                         Default is computer (interactive) mode.
  --interval <SECS>      Poll interval in seconds (default: 5).
  --repo <PATH>          Directory to run `bd` from (default: current dir).
  --theme <NAME>         default | light | solarized-dark | solarized-light |
                         gruvbox | dracula | high-contrast
                         Default: `default`. TV mode auto-picks `high-contrast`
                         unless `--theme` overrides.
  -h, --help
  -V, --version
```

Env vars:
- `BD_WATCHER_THEME` overrides default theme.

## Architecture

```
CLI (clap) ──▶ Poller task (tokio) ──▶ App state ──▶ TUI (ratatui)
                     │                     │
                     ▼                     ▼
              bd graph --json        bounded activity
              subprocess             ring buffer
```

- **Poller**: a tokio task loops every N seconds. It spawns `bd graph [--all|<id>] --json` in the configured repo dir, reads stdout, parses JSON into a typed `Snapshot`, diffs against the previous snapshot to emit `ActivityEvent`s, and sends both (new snapshot, new events) over an mpsc channel to the UI task.
- **App state**: holds the latest `Snapshot`, a bounded ring buffer of last ~100 `ActivityEvent`s, selection state (computer mode), last-refresh timestamp, and last error (if any).
- **TUI**: render loop on the main thread reacts to (a) input events from `crossterm`, (b) new snapshots from the poller channel. Redraws on either.

## Data model

Mirrors what `bd graph --all --json` emits (array of components):

```rust
struct Component {
    root: Issue,
    issues: Vec<Issue>,
    dependencies: Vec<Dependency>,
    // IssueMap, Phase, VarDefs, Pour ignored (not needed for dashboard)
}

struct Issue {
    id: String,
    title: String,
    description: String,
    status: Status,          // Open | InProgress | Blocked | Closed | Deferred
    priority: i32,
    issue_type: String,      // "epic" | "task" | ...
    owner: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    external_ref: Option<String>,
}

struct Dependency {
    issue_id: String,
    depends_on_id: String,
    dep_type: DepType,       // ParentChild | Blocks | Related
}

struct Snapshot {
    components: Vec<Component>,
    fetched_at: DateTime<Utc>,
}

enum ActivityEvent {
    StatusChange { id: String, title: String, from: Status, to: Status, at: DateTime<Utc> },
    Added        { id: String, title: String, status: Status, at: DateTime<Utc> },
    Removed      { id: String, at: DateTime<Utc> },
}
```

Only status changes are surfaced in the activity feed (per user decision). `Added` / `Removed` are computed for completeness but not rendered — trivial to expose later if wanted.

## Diff algorithm

Given `prev: Option<Snapshot>` and `next: Snapshot`:

1. If `prev` is None → no events (first snapshot).
2. Build `prev_by_id: HashMap<String, &Issue>` and same for `next`.
3. For each id in `next` but not `prev` → `Added`.
4. For each id in `prev` but not `next` → `Removed`.
5. For each id in both where `status` differs → `StatusChange`.

Timestamps for events use `next.fetched_at`. Pushed onto the bounded ring buffer; oldest drop off.

## Layout

### TV mode

```
┌─ bd-watcher ─ repo: ~/code/my-repo ─ every 5s ─ 14:32:07 ────────────┐
│  TOTAL   open 42   in-progress 3   blocked 5   closed 28   DONE 40% │
├─ Epics ─────────────────────────────────────────────────────────────┤
│  sel3-42wn  GetFeatures migration              ▓▓▓▓▓▓░░░░  12/19   │
│             ◐ 2  ● 1  ○ 4  ✓ 12                                    │
│  sel3-88aa  Auction v2 cutover                 ▓▓▓░░░░░░░   3/9    │
│             ◐ 1  ● 0  ○ 5  ✓ 3                                     │
├─ Activity (last 20) ────────────────────────────────────────────────┤
│  14:31:52  ✓  sel3-42wn.10  open → closed       "Wire FeatureS..."  │
│  14:30:14  ◐  sel3-42wn.11  open → in_progress  "Add OTel metr..."  │
│  14:27:03  ●  sel3-42wn.16  open → blocked      "[Stage] Enabl..."  │
└─────────────────────────────────────────────────────────────────────┘
```

- No selection highlight, no footer hints, no detail panel.
- Status icons: `○` open, `◐` in_progress, `●` blocked, `✓` closed, `❄` deferred.
- Progress bar = closed/total for that epic.
- Activity rows auto-scroll, oldest drops off. Capped at (terminal height - header - epics area).

### Computer mode

Same three regions, plus:
- Selected epic row is highlighted (reversed colors).
- Footer hint line: `q quit · r refresh · ↑↓ select · ↵ detail · / filter · esc close`.
- `Enter` on a selected epic opens a detail panel as a full-screen overlay, showing:
  - Root issue: id, title, status, owner, external_ref, full description.
  - Children table: id, status icon, title, deps summary (`blocks: a, b · blocked-by: c`).
- `/` opens a filter input; typing filters the epic list by substring match on id or title.
- `Esc` closes whichever panel/input is open.

## Themes

A `Theme` struct holds ratatui styles for each semantic role:

```rust
struct Theme {
    bg: Color,
    fg: Color,
    muted: Color,
    accent: Color,
    status_open: Color,
    status_in_progress: Color,
    status_blocked: Color,
    status_closed: Color,
    status_deferred: Color,
    progress_filled: Color,
    progress_empty: Color,
    selection_bg: Color,
    selection_fg: Color,
    error: Color,
}
```

Built-in themes: `default` (dark), `light`, `solarized-dark`, `solarized-light`, `gruvbox`, `dracula`, `high-contrast`.

Resolution order: `--theme` CLI flag → `BD_WATCHER_THEME` env var → mode default (`high-contrast` for `--tv`, `default` otherwise).

## Error handling

| Situation | Behavior |
|---|---|
| `bd` not on PATH at startup | Print friendly message, exit 1. |
| `bd graph` nonzero exit or invalid JSON mid-run | Keep last good snapshot. Show a banner in the header: `⚠ last refresh failed at HH:MM — retrying in Ns`. Do not crash. |
| Terminal too small (<80×24) | Render a centered "resize to at least 80×24" message; resume normal rendering once resized. |
| Ctrl-C / `q` | Restore terminal state cleanly (drop `AlternateScreen` guard, disable raw mode). |
| Poller channel closed (UI exited) | Poller task exits. |

## Project layout

```
bd-watcher/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs         # CLI parsing + entrypoint
│   ├── bd.rs           # spawn `bd graph --json`, parse
│   ├── model.rs        # Issue, Dependency, Component, Snapshot, Status
│   ├── diff.rs         # compare snapshots → ActivityEvent[]
│   ├── app.rs          # App state, event loop, Mode enum
│   ├── theme.rs        # Theme struct + built-in themes
│   └── ui/
│       ├── mod.rs
│       ├── tv.rs       # TV-mode render
│       ├── computer.rs # interactive-mode render + input
│       └── widgets.rs  # shared: progress row, activity row, detail panel
└── tests/
    └── diff_test.rs    # snapshot diff unit tests (no bd needed)
```

## Dependencies (Cargo.toml)

```toml
[dependencies]
ratatui     = "0.29"
crossterm   = "0.28"
tokio       = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "process", "signal"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
clap        = { version = "4", features = ["derive", "env"] }
anyhow      = "1"
chrono      = { version = "0.4", features = ["serde"] }

[dev-dependencies]
pretty_assertions = "1"
```

## Testing

- Unit test `diff.rs` with synthetic `Snapshot` fixtures — covers added, removed, status-change, and no-change cases.
- Unit test `bd.rs` JSON parser against a checked-in fixture (sample `bd graph --all --json` output trimmed to 2–3 components).
- No end-to-end test invoking real `bd` (out of scope; would need a bd repo fixture).

## Success criteria

- `cargo run -- --tv` in a directory with a bd repo shows the dashboard with correct counts, progress bars, and a growing activity feed when another terminal runs `bd update <id> --status in_progress`.
- `cargo run -- sel3-42wn` in computer mode lets me navigate epics with arrows, drill in with Enter, see full descriptions, and exit cleanly.
- `cargo run -- --theme solarized-dark` applies the theme across all regions.
- Binary is under ~5 MB release-built.
