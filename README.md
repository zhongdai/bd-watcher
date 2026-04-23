# bd-watcher

[![Release](https://img.shields.io/github/v/release/zhongdai/bd-watcher)](https://github.com/zhongdai/bd-watcher/releases)
[![CI](https://github.com/zhongdai/bd-watcher/actions/workflows/ci.yml/badge.svg)](https://github.com/zhongdai/bd-watcher/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Homebrew](https://img.shields.io/badge/homebrew-zhongdai%2Ftap-orange)](https://github.com/zhongdai/homebrew-tap)

A lightweight Rust TUI dashboard for the [bd (beads)](https://github.com/steveyegge/beads) issue tracker. Polls `bd graph --json` on an interval and shows per-epic progress plus a live activity feed of status changes.

Built to watch AI agents work through a stack of beads.

[![demo](https://asciinema.org/a/I4zNssvFFeQBAIFQ.svg?v=2)](https://asciinema.org/a/I4zNssvFFeQBAIFQ)

`bd-watcher` is read-only — it never writes to the Dolt database. Arrow keys select an epic, `Enter` drills into it, `/` filters.

## Focused epic view

Pass an epic id to zoom in: `bd-watcher <epic-id>`. The pane groups tasks by dependency layer (Layer 0 is ready to start; each higher layer waits on the previous). Each row shows, in order:

```
[status] [bead-id] [#PR] [type] [title] [← blocked-by]
```

- **#PR** — the GitHub PR number when the bead's `external-ref` is set to `gh-<N>`. Blank otherwise.
- **type** — bead type (`task`, `epic`, `design`, etc.).
- **← blocked-by** — inline list of ids this task waits on (if any).

## Install

### Homebrew (macOS / Linux)

```bash
brew tap zhongdai/tap
brew install bd-watcher
```

### Via Cargo

```bash
cargo install --git https://github.com/zhongdai/bd-watcher.git
```

### From source

```bash
git clone https://github.com/zhongdai/bd-watcher.git
cd bd-watcher
cargo install --path .
```

Requires `bd` on `PATH`. Install from the [beads releases](https://github.com/steveyegge/beads) or `brew install beads`.

## Usage

```bash
cd /path/to/your/bd/repo

bd-watcher                       # all-epics dashboard
bd-watcher sel3-42wn             # focus a single epic
bd-watcher --theme dracula       # pick a theme
bd-watcher --interval 2          # refresh every 2s (default 5)
bd-watcher --repo ~/code/my-repo # run `bd` from another directory
```

### Keys

All-epics view:

| Key | Action |
|---|---|
| `q`            | quit |
| `r`            | force refresh |
| `↑` / `k`      | previous epic |
| `↓` / `j`      | next epic |
| `gg` / `G`     | first / last epic |
| `y`            | copy epic id |
| `/`            | filter by id/title (Esc to close) |

Focused-epic view (`bd-watcher <epic-id>`):

| Key | Action |
|---|---|
| `↑` / `k`      | previous sub-bead |
| `↓` / `j`      | next sub-bead |
| `gg` / `G`     | first / last sub-bead |
| `Enter`        | open detail popup for selected sub-bead |
| `Esc` / `Enter`| close popup |
| `y`            | copy selected sub-bead id |
| `q`            | quit |

### Themes

`default` · `light` · `solarized-dark` · `solarized-light` · `gruvbox` · `dracula` · `high-contrast`

Override via `--theme` or the `BD_WATCHER_THEME` env var.

## Development

```bash
just            # list recipes
just build      # cargo build
just test       # cargo test
just lint       # cargo clippy -- -D warnings
just check      # test + lint + fmt-check
just release 0.2.0    # bump version, tag, push, update tap
```

## License

MIT © Zhong Dai
