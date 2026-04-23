use chrono::{DateTime, Local, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::app::App;
use crate::model::{ActivityEvent, Component, DepType, Snapshot, Status, StatusCounts};
use crate::theme::Theme;

pub fn status_color(theme: &Theme, status: Status) -> ratatui::style::Color {
    match status {
        Status::Open => theme.status_open,
        Status::InProgress => theme.status_in_progress,
        Status::Blocked => theme.status_blocked,
        Status::Closed => theme.status_closed,
        Status::Deferred => theme.status_deferred,
    }
}

fn fmt_local(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Local).format("%H:%M:%S").to_string()
}

pub fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let repo = app.repo.display().to_string();
    let interval = app.interval_secs;
    let refreshed = match &app.snapshot {
        Some(s) => fmt_local(s.fetched_at),
        None => "—".to_string(),
    };

    let counts = app
        .snapshot
        .as_ref()
        .map(Snapshot::total_counts)
        .unwrap_or_default();
    let pct = (counts.done_fraction() * 100.0).round() as u32;

    let title = format!(
        " bd-watcher · repo: {} · every {}s · {} ",
        repo, interval, refreshed
    );

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            " TOTAL  ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        status_count_span(theme, Status::Open, counts.open),
        Span::raw("  "),
        status_count_span(theme, Status::InProgress, counts.in_progress),
        Span::raw("  "),
        status_count_span(theme, Status::Blocked, counts.blocked),
        Span::raw("  "),
        status_count_span(theme, Status::Closed, counts.closed),
        Span::raw("  "),
        Span::styled(
            format!("DONE {}%", pct),
            Style::default()
                .fg(theme.progress_filled)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if let Some((at, msg)) = &app.last_error {
        lines.push(Line::from(vec![Span::styled(
            format!(
                " ⚠ last refresh failed at {} — retrying in {}s · {}",
                fmt_local(*at),
                app.interval_secs,
                msg
            ),
            Style::default().fg(theme.error),
        )]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(theme.muted));
    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, area);
}

fn status_count_span<'a>(theme: &Theme, status: Status, count: usize) -> Span<'a> {
    Span::styled(
        format!("{} {} {}", status.icon(), status.label(), count),
        Style::default().fg(status_color(theme, status)),
    )
}

/// Counts the component's sub-beads — excludes the root epic itself,
/// which is a container, not real work. A 5/10-done epic means five
/// of its ten sub-beads are closed.
pub fn counts_for(component: &Component) -> StatusCounts {
    let mut c = StatusCounts::default();
    let mut seen = std::collections::HashSet::new();
    for i in &component.issues {
        if i.id == component.root.id {
            continue;
        }
        if seen.insert(&i.id) {
            c.add(i.status);
        }
    }
    c
}

pub fn progress_bar(filled_width: usize, total_width: usize) -> String {
    let filled = "▓".repeat(filled_width);
    let empty = "░".repeat(total_width.saturating_sub(filled_width));
    format!("{}{}", filled, empty)
}

pub fn render_epics(app: &App, frame: &mut Frame, area: Rect, highlight_selected: bool) {
    let theme = &app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Epics ")
        .border_style(Style::default().fg(theme.muted));
    let inner = block.inner(area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(" Epics ")
            .border_style(Style::default().fg(theme.muted))
            .style(Style::default().bg(theme.bg)),
        area,
    );

    let Some(snap) = &app.snapshot else {
        let p = Paragraph::new(Line::from(Span::styled(
            " loading… ",
            Style::default().fg(theme.muted),
        )))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
        frame.render_widget(p, inner);
        return;
    };

    let indices = app.filtered_epic_indices(snap);
    if indices.is_empty() {
        let msg = if snap.components.is_empty() {
            " no open issues "
        } else {
            " no epics match filter "
        };
        let p = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default().fg(theme.muted),
        )))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
        frame.render_widget(p, inner);
        return;
    }

    let bar_width = 20usize;
    let lines_per_epic = 3usize;
    let mut lines: Vec<Line> = Vec::new();
    let selected_pos = indices
        .iter()
        .position(|&i| i == app.selected_epic)
        .unwrap_or(0);

    for &idx in &indices {
        let comp = &snap.components[idx];
        let counts = counts_for(comp);
        let total = counts.total();
        let filled = if total == 0 {
            0
        } else {
            (bar_width as f64 * (counts.closed as f64 / total as f64)).round() as usize
        };
        let bar = progress_bar(filled, bar_width);

        let is_selected = highlight_selected && idx == app.selected_epic;
        let row_style = if is_selected {
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.selection_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };

        let title = &comp.root.title;
        let title_trimmed = truncate(title, 48);
        let id_col = 18usize;
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<width$}", comp.root.id, width = id_col),
                row_style.add_modifier(Modifier::BOLD).fg(theme.accent),
            ),
            Span::raw("  "),
            Span::styled(format!("{:<48} ", title_trimmed), row_style),
            Span::styled(bar, Style::default().fg(theme.progress_filled)),
            Span::styled(format!("  {}/{}", counts.closed, total), row_style),
        ]));

        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::raw(" ".repeat(id_col + 2)),
            status_count_span(theme, Status::InProgress, counts.in_progress),
            Span::raw("  "),
            status_count_span(theme, Status::Blocked, counts.blocked),
            Span::raw("  "),
            status_count_span(theme, Status::Open, counts.open),
            Span::raw("  "),
            status_count_span(theme, Status::Closed, counts.closed),
        ]));

        lines.push(Line::raw(""));
    }

    let inner_lines = inner.height as usize;
    let total_epics = indices.len();
    let offset = if highlight_selected {
        scroll_offset(selected_pos, lines_per_epic, inner_lines, total_epics)
    } else {
        0
    };

    let p = Paragraph::new(lines)
        .scroll((offset as u16, 0))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, inner);
}

/// Compute line-scroll offset so the selected epic stays visible.
/// Keeps the selected epic in view, preferring to show one epic of context
/// above it when possible, and clamps at the bottom.
fn scroll_offset(
    selected_pos: usize,
    lines_per_epic: usize,
    inner_lines: usize,
    total_epics: usize,
) -> usize {
    if inner_lines == 0 || total_epics == 0 {
        return 0;
    }
    let visible_epics = (inner_lines / lines_per_epic).max(1);
    if total_epics <= visible_epics {
        return 0;
    }
    let max_top = total_epics - visible_epics;
    let desired_top = selected_pos.saturating_sub(visible_epics.saturating_sub(1) / 2);
    let top = desired_top.min(max_top);
    top * lines_per_epic
}

pub fn render_activity(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    // In focused-epic view, highlight the border when the Activity
    // pane has keyboard focus. Outside focused-epic view the pane is
    // non-interactive, so stay muted.
    let focused = app.focus.is_some() && app.focused_pane == crate::app::FocusedPane::Activity;
    let border_color = if focused { theme.accent } else { theme.muted };
    let title = if focused {
        format!(" Activity (last {}) · focus ", app.activity.len())
    } else {
        format!(" Activity (last {}) ", app.activity.len())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.activity.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            " (waiting for status changes…) ",
            Style::default().fg(theme.muted),
        )))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
        frame.render_widget(p, inner);
        return;
    }

    let rows = inner.height as usize;
    // app.activity is oldest-first; we display newest first. Scroll
    // offset counts "how many newest to skip", so offset=0 shows the
    // newest, offset=N shows older history further back.
    let offset = app
        .activity_scroll
        .min(app.activity.len().saturating_sub(1));
    let events: Vec<&ActivityEvent> = app.activity.iter().rev().skip(offset).take(rows).collect();

    let lines: Vec<Line> = events
        .into_iter()
        .map(|ev| activity_line(theme, ev))
        .collect();

    let p = Paragraph::new(lines).style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, inner);
}

fn activity_line<'a>(theme: &Theme, ev: &'a ActivityEvent) -> Line<'a> {
    match ev {
        ActivityEvent::StatusChange {
            id,
            title,
            from,
            to,
            at,
        } => Line::from(vec![
            Span::styled(
                format!(" {} ", fmt_local(*at)),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!(" {} ", to.icon()),
                Style::default()
                    .fg(status_color(theme, *to))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {:<14}", id), Style::default().fg(theme.accent)),
            Span::styled(
                format!("{} → {}  ", from.label(), to.label()),
                Style::default().fg(status_color(theme, *to)),
            ),
            Span::styled(truncate(title, 60), Style::default().fg(theme.fg)),
        ]),
        ActivityEvent::Added {
            id,
            title,
            status,
            at,
        } => Line::from(vec![
            Span::styled(
                format!(" {} ", fmt_local(*at)),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                " + ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {:<14}", id), Style::default().fg(theme.accent)),
            Span::styled(
                format!("added ({})  ", status.label()),
                Style::default().fg(status_color(theme, *status)),
            ),
            Span::raw(truncate(title, 60).to_string()),
        ]),
        ActivityEvent::Removed { id, at } => Line::from(vec![
            Span::styled(
                format!(" {} ", fmt_local(*at)),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                " − ",
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {:<14}", id), Style::default().fg(theme.muted)),
            Span::styled("removed", Style::default().fg(theme.muted)),
        ]),
    }
}

pub fn render_footer(app: &App, frame: &mut Frame, area: Rect, hint: &str) {
    let theme = &app.theme;
    let p = Paragraph::new(Line::from(Span::styled(
        format!(" {} ", hint),
        Style::default().fg(theme.muted),
    )))
    .style(Style::default().bg(theme.bg));
    frame.render_widget(p, area);
}

/// Renders the top-right detail pane: metadata + description for the
/// currently-selected epic.
pub fn render_detail_header(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let Some(snap) = &app.snapshot else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Detail ")
            .border_style(Style::default().fg(theme.muted))
            .style(Style::default().bg(theme.bg));
        frame.render_widget(block, area);
        return;
    };
    let Some(comp) = snap.components.get(app.selected_epic) else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Detail ")
            .border_style(Style::default().fg(theme.muted))
            .style(Style::default().bg(theme.bg));
        frame.render_widget(block, area);
        return;
    };

    let title_text = format!(
        " {} · {} ",
        comp.root.id,
        truncate(&comp.root.title, area.width.saturating_sub(12) as usize)
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_text)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let updated = fmt_local(comp.root.updated_at);
    let created = fmt_local(comp.root.created_at);

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("status: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{} {}", comp.root.status.icon(), comp.root.status.label()),
                Style::default()
                    .fg(status_color(theme, comp.root.status))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("priority: ", Style::default().fg(theme.muted)),
            Span::styled(
                comp.root.priority.to_string(),
                Style::default().fg(theme.fg),
            ),
            Span::raw("   "),
            Span::styled("type: ", Style::default().fg(theme.muted)),
            Span::styled(&comp.root.issue_type, Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled("owner: ", Style::default().fg(theme.muted)),
            Span::styled(
                comp.root.owner.as_deref().unwrap_or("—"),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled("updated: ", Style::default().fg(theme.muted)),
            Span::styled(updated, Style::default().fg(theme.fg)),
            Span::raw("   "),
            Span::styled("created: ", Style::default().fg(theme.muted)),
            Span::styled(created, Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled("external: ", Style::default().fg(theme.muted)),
            Span::styled(
                comp.root.external_ref.as_deref().unwrap_or("—"),
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::raw(""),
    ];
    if !comp.root.description.is_empty() {
        lines.push(Line::from(Span::styled(
            "description",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::UNDERLINED),
        )));
        for line in comp.root.description.lines() {
            lines.push(Line::raw(line.to_string()));
        }
    }

    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, inner);
}

/// Renders the bottom-right children pane: task list for the
/// currently-selected epic.
pub fn render_detail_children(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Children ")
        .border_style(Style::default().fg(theme.muted))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = &app.snapshot else { return };
    let Some(comp) = snap.components.get(app.selected_epic) else {
        return;
    };

    let title_width = (area.width as usize).saturating_sub(20);
    let mut lines: Vec<Line> = Vec::new();
    for issue in comp.issues.iter().filter(|i| i.id != comp.root.id) {
        let blocked_by: Vec<&str> = comp
            .dependencies
            .iter()
            .filter(|d| {
                d.issue_id == issue.id && matches!(d.dep_type, crate::model::DepType::Blocks)
            })
            .map(|d| d.depends_on_id.as_str())
            .collect();
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", issue.status.icon()),
                Style::default().fg(status_color(theme, issue.status)),
            ),
            Span::styled(
                format!("{:<14}", issue.id),
                Style::default().fg(theme.accent),
            ),
            Span::raw(" "),
            Span::styled(
                truncate(&issue.title, title_width.max(10)),
                Style::default().fg(theme.fg),
            ),
        ]));
        if !blocked_by.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(
                    format!("blocked-by: {}", blocked_by.join(", ")),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " (no children) ",
            Style::default().fg(theme.muted),
        )));
    }
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, inner);
}

pub fn render_filter(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let p = Paragraph::new(Line::from(vec![
        Span::styled(" filter: ", Style::default().fg(theme.muted)),
        Span::styled(
            &app.filter,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled("█", Style::default().fg(theme.accent)),
    ]))
    .style(Style::default().bg(theme.bg));
    frame.render_widget(p, area);
}

pub fn too_small_placeholder(frame: &mut Frame, theme: &Theme, area: Rect) {
    let p = Paragraph::new(Line::from(Span::styled(
        "resize to at least 80×24",
        Style::default().fg(theme.muted),
    )))
    .alignment(ratatui::layout::Alignment::Center)
    .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, area);
}

/// Extracts a GitHub PR number from a bead's `external_ref` when it matches
/// the `gh-<N>` convention. Returns `None` for other ref shapes (jira-*,
/// linear-*), empty refs, or unparseable input.
fn pr_number(external_ref: Option<&str>) -> Option<u32> {
    external_ref
        .and_then(|s| s.strip_prefix("gh-"))
        .and_then(|n| n.parse::<u32>().ok())
}

/// Width reserved for the PR column: 2 leading spaces + "#" + up to 6
/// digits + 1 trailing gap before the next column. Widens to 10 so a
/// 5- or 6-digit PR (common in active monorepos) still has visible
/// separation from the type column.
const PR_CELL_WIDTH: usize = 10;

/// Renders the fixed-width PR column cell. Always `PR_CELL_WIDTH` chars
/// so the following column stays aligned whether or not a PR ref is
/// present.
fn pr_cell(external_ref: Option<&str>) -> String {
    match pr_number(external_ref) {
        Some(n) => format!("{:<width$}", format!("  #{n}"), width = PR_CELL_WIDTH),
        None => " ".repeat(PR_CELL_WIDTH),
    }
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

/// Topologically groups a component's non-root issues into layers by
/// `Blocks`/`ParentChild` dependency depth. Layer 0 = no in-component deps;
/// layer N = depends on issues in layer < N. Returns a Vec of Vecs of
/// indices into `component.issues`. Layer ordering within is stable by id.
pub fn compute_layers(component: &Component) -> Vec<Vec<usize>> {
    use std::collections::HashMap;

    let root_id = component.root.id.as_str();
    let n = component.issues.len();

    let id_to_idx: HashMap<&str, usize> = component
        .issues
        .iter()
        .enumerate()
        .map(|(i, iss)| (iss.id.as_str(), i))
        .collect();

    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
    for d in &component.dependencies {
        // Only ordering edges contribute to layering.
        if !matches!(d.dep_type, DepType::Blocks | DepType::ParentChild) {
            continue;
        }
        if d.issue_id == root_id || d.depends_on_id == root_id {
            continue;
        }
        if let (Some(&a), Some(&b)) = (
            id_to_idx.get(d.issue_id.as_str()),
            id_to_idx.get(d.depends_on_id.as_str()),
        ) {
            deps[a].push(b);
        }
    }

    let mut layer: Vec<Option<usize>> = vec![None; n];
    loop {
        let mut progress = false;
        for i in 0..n {
            if component.issues[i].id == root_id {
                continue;
            }
            if layer[i].is_some() {
                continue;
            }
            if deps[i].iter().all(|&d| layer[d].is_some()) {
                let l = deps[i]
                    .iter()
                    .filter_map(|&d| layer[d])
                    .max()
                    .map(|m| m + 1)
                    .unwrap_or(0);
                layer[i] = Some(l);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }

    let max_l = layer.iter().filter_map(|l| *l).max().unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_l + 1];
    for (i, l) in layer.iter().enumerate() {
        if let Some(l) = l {
            layers[*l].push(i);
        }
    }
    for bucket in &mut layers {
        bucket.sort_by(|&a, &b| component.issues[a].id.cmp(&component.issues[b].id));
    }
    layers
}

/// Returns indices into `component.issues` in the same top-to-bottom
/// order the focused-epic view renders them: layer 0 first, then
/// layer 1, etc.; within each layer sorted by id. This is the
/// authoritative order for sub-bead selection so arrow-key navigation
/// matches what's on screen.
pub fn visual_sub_order(component: &Component) -> Vec<usize> {
    compute_layers(component).into_iter().flatten().collect()
}

/// Shortens a child id by stripping the root epic prefix. E.g.
/// ("sel3-42wn.10", "sel3-42wn") -> ".10". Full id returned unchanged if the
/// prefix doesn't match.
fn short_id(id: &str, root_id: &str) -> String {
    let prefix = format!("{root_id}.");
    if let Some(rest) = id.strip_prefix(&prefix) {
        format!(".{rest}")
    } else {
        id.to_string()
    }
}

/// Renders the focused epic as a single-pane layered DAG: header with
/// overall progress, then one section per layer listing the tasks at that
/// depth (status icon + id + title + deps inline). The pane is the primary
/// view used when bd-watcher is run against a single epic id.
pub fn render_single_epic_dag(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let Some(snap) = &app.snapshot else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Epic ")
            .border_style(Style::default().fg(theme.muted))
            .style(Style::default().bg(theme.bg));
        frame.render_widget(block, area);
        return;
    };
    let Some(comp) = snap.components.first() else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Epic ")
            .border_style(Style::default().fg(theme.muted))
            .style(Style::default().bg(theme.bg));
        frame.render_widget(block, area);
        return;
    };

    let counts = counts_for(comp);
    let total_tasks = counts.total();
    let title_line = format!(
        " {} · {} · {}/{} done ",
        comp.root.id,
        truncate(&comp.root.title, area.width.saturating_sub(30) as usize),
        counts.closed,
        total_tasks,
    );

    let focused = app.focused_pane == crate::app::FocusedPane::Tasks;
    let border_color = if focused { theme.accent } else { theme.muted };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_line)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layers = compute_layers(comp);
    let inner_width = inner.width as usize;
    let id_col = 16usize;
    // Reserve room for " [icon] id  " prefix + PR + type (8) columns + "  ← deps" suffix.
    let title_width = inner_width
        .saturating_sub(id_col + 6 + PR_CELL_WIDTH + 8 + 20)
        .max(10);

    let mut lines: Vec<Line> = Vec::new();

    // Compact summary row under the title
    lines.push(Line::from(vec![
        Span::styled(" status: ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} {}", comp.root.status.icon(), comp.root.status.label()),
            Style::default()
                .fg(status_color(theme, comp.root.status))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        status_count_span(theme, Status::InProgress, counts.in_progress),
        Span::raw("  "),
        status_count_span(theme, Status::Blocked, counts.blocked),
        Span::raw("  "),
        status_count_span(theme, Status::Open, counts.open),
        Span::raw("  "),
        status_count_span(theme, Status::Closed, counts.closed),
        Span::raw("   "),
        Span::styled(
            format!("tasks: {}", total_tasks),
            Style::default().fg(theme.muted),
        ),
    ]));
    lines.push(Line::raw(""));

    if layers.is_empty() || layers.iter().all(|l| l.is_empty()) {
        lines.push(Line::from(Span::styled(
            " (no sub-tasks) ",
            Style::default().fg(theme.muted),
        )));
    }

    let root_id = comp.root.id.as_str();
    // Build a per-issue lookup of the issues it depends on (filtered to
    // non-root, Blocks/ParentChild) so we can annotate each row inline.
    let mut dep_labels: std::collections::HashMap<&str, Vec<String>> =
        std::collections::HashMap::new();
    for d in &comp.dependencies {
        if !matches!(d.dep_type, DepType::Blocks | DepType::ParentChild) {
            continue;
        }
        if d.issue_id == root_id || d.depends_on_id == root_id {
            continue;
        }
        dep_labels
            .entry(d.issue_id.as_str())
            .or_default()
            .push(short_id(&d.depends_on_id, root_id));
    }

    // Index of the sub-bead currently selected (for row highlighting).
    // None when the user hasn't focused this epic or when there are no
    // children yet.
    let selected_issue_idx = visual_sub_order(comp).get(app.selected_sub).copied();

    for (layer_idx, issues) in layers.iter().enumerate() {
        if issues.is_empty() {
            continue;
        }
        // Layer divider
        lines.push(Line::from(vec![
            Span::styled(
                format!(" Layer {layer_idx} "),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "─".repeat(inner_width.saturating_sub(10)),
                Style::default().fg(theme.muted),
            ),
        ]));
        for &i in issues {
            let issue = &comp.issues[i];
            let deps = dep_labels
                .get(issue.id.as_str())
                .map(|v| v.join(", "))
                .unwrap_or_default();
            let mut spans = vec![
                Span::raw("   "),
                Span::styled(
                    format!("{} ", issue.status.icon()),
                    Style::default()
                        .fg(status_color(theme, issue.status))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<width$}", issue.id, width = id_col),
                    Style::default().fg(theme.accent),
                ),
                Span::raw(" "),
                Span::styled(
                    pr_cell(issue.external_ref.as_deref()),
                    Style::default().fg(theme.accent),
                ),
                Span::styled(
                    format!("{:<8}", truncate(&issue.issue_type, 7)),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(
                    format!(
                        "{:<width$}",
                        truncate(&issue.title, title_width),
                        width = title_width
                    ),
                    Style::default().fg(theme.fg),
                ),
            ];
            if !deps.is_empty() {
                spans.push(Span::styled(
                    format!("  ← {deps}"),
                    Style::default().fg(theme.muted),
                ));
            }
            let mut line = Line::from(spans);
            if Some(i) == selected_issue_idx {
                line = line.style(
                    Style::default()
                        .bg(theme.selection_bg)
                        .fg(theme.selection_fg),
                );
            }
            lines.push(line);
        }
    }

    let p = Paragraph::new(lines).style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, inner);
}

/// Renders a centered modal with the full details of `app.selected_sub_bead()`.
/// Does nothing when there's no selected sub-bead or when the terminal is
/// smaller than the modal's minimum size.
///
/// Layout: a bordered block with metadata rows fixed at the top and the
/// description in a scrollable region below. A `Scrollbar` widget on the
/// right edge of the description gives visible scroll feedback.
pub fn render_bead_detail_popup(app: &App, frame: &mut Frame) {
    let Some(issue) = app.selected_sub_bead() else {
        return;
    };
    let theme = &app.theme;
    let area = frame.area();

    let popup = centered_rect(area, 70, 80);
    if popup.width < 40 || popup.height < 10 {
        return;
    }

    // Wipe whatever was underneath.
    frame.render_widget(ratatui::widgets::Clear, popup);

    let title = format!(" {} · {} ", issue.id, truncate(&issue.title, 60));
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(Line::from(Span::styled(
            " ↑↓ scroll · enter/esc close ",
            Style::default().fg(theme.muted),
        )))
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // --- Build the fixed metadata lines ---
    let updated = fmt_local(issue.updated_at);
    let created = fmt_local(issue.created_at);

    let mut meta_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("status: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{} {}", issue.status.icon(), issue.status.label()),
                Style::default()
                    .fg(status_color(theme, issue.status))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("priority: ", Style::default().fg(theme.muted)),
            Span::styled(issue.priority.to_string(), Style::default().fg(theme.fg)),
            Span::raw("   "),
            Span::styled("type: ", Style::default().fg(theme.muted)),
            Span::styled(&issue.issue_type, Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled("owner: ", Style::default().fg(theme.muted)),
            Span::styled(
                issue.owner.as_deref().unwrap_or("—"),
                Style::default().fg(theme.fg),
            ),
            Span::raw("   "),
            Span::styled("external: ", Style::default().fg(theme.muted)),
            Span::styled(
                issue.external_ref.as_deref().unwrap_or("—"),
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::from(vec![
            Span::styled("updated: ", Style::default().fg(theme.muted)),
            Span::styled(updated, Style::default().fg(theme.fg)),
            Span::raw("   "),
            Span::styled("created: ", Style::default().fg(theme.muted)),
            Span::styled(created, Style::default().fg(theme.fg)),
        ]),
    ];
    if let Some(comp) = app.focused_component() {
        let root_id = comp.root.id.as_str();
        let blocked_by: Vec<String> = comp
            .dependencies
            .iter()
            .filter(|d| {
                d.issue_id == issue.id
                    && matches!(d.dep_type, DepType::Blocks | DepType::ParentChild)
            })
            .filter(|d| d.depends_on_id != root_id)
            .map(|d| short_id(&d.depends_on_id, root_id))
            .collect();
        if !blocked_by.is_empty() {
            meta_lines.push(Line::from(vec![
                Span::styled("blocked-by: ", Style::default().fg(theme.muted)),
                Span::styled(blocked_by.join(", "), Style::default().fg(theme.fg)),
            ]));
        }
    }

    // --- Split inner into (metadata | divider | scrollable body) ---
    let meta_height = meta_lines.len() as u16;
    let divider_h = if !issue.description.is_empty() { 1 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(meta_height),
            Constraint::Length(divider_h),
            Constraint::Min(1),
        ])
        .split(inner);

    let meta_p = Paragraph::new(meta_lines).style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(meta_p, chunks[0]);

    if issue.description.is_empty() {
        return;
    }

    // Horizontal rule between metadata and description.
    let rule = Paragraph::new(Line::from(Span::styled(
        "─".repeat(chunks[1].width as usize),
        Style::default().fg(theme.muted),
    )))
    .style(Style::default().bg(theme.bg));
    frame.render_widget(rule, chunks[1]);

    // --- Scrollable description body ---
    let body_area = chunks[2];
    // Reserve one column on the right for the scrollbar; render the
    // description into the narrower rect to keep text and bar
    // non-overlapping.
    let text_area = Rect {
        x: body_area.x,
        y: body_area.y,
        width: body_area.width.saturating_sub(1),
        height: body_area.height,
    };
    let bar_area = Rect {
        x: body_area.x + body_area.width.saturating_sub(1),
        y: body_area.y,
        width: 1,
        height: body_area.height,
    };

    let desc_lines: Vec<Line> = issue
        .description
        .lines()
        .map(|l| Line::raw(l.to_string()))
        .collect();

    // Naive line count (wrapped long lines count as one). Good enough
    // for clamping; ratatui 0.29 gates accurate post-wrap line_count
    // behind an unstable feature.
    let total = desc_lines.len() as u16;
    let max_scroll = total.saturating_sub(text_area.height.max(1));
    let scroll = app.popup_scroll.min(max_scroll);

    let desc_p = Paragraph::new(desc_lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(desc_p, text_area);

    // Scrollbar on the right. content_length = number of rows we can
    // scroll (total - viewport); position = current scroll offset.
    let mut sb_state = ScrollbarState::new(max_scroll as usize).position(scroll as usize);
    let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("█")
        .style(Style::default().fg(theme.muted));
    frame.render_stateful_widget(sb, bar_area, &mut sb_state);
}

/// Returns a Rect centered inside `area`, `width_pct` and `height_pct`
/// as percentages [0..=100].
fn centered_rect(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let w = area.width.saturating_mul(width_pct) / 100;
    let h = area.height.saturating_mul(height_pct) / 100;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Component, Dependency, Issue};
    use chrono::Utc;

    fn issue(id: &str, status: Status, issue_type: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: id.to_string(),
            description: String::new(),
            status,
            priority: 0,
            issue_type: issue_type.to_string(),
            owner: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            external_ref: None,
        }
    }

    #[test]
    fn counts_for_excludes_root_epic() {
        let root = issue("ep-1", Status::Open, "epic");
        let children = vec![
            issue("ep-1.a", Status::Closed, "task"),
            issue("ep-1.b", Status::InProgress, "task"),
            issue("ep-1.c", Status::Open, "task"),
        ];
        let mut issues = vec![root.clone()];
        issues.extend(children);
        let comp = Component {
            root,
            issues,
            dependencies: Vec::<Dependency>::new(),
        };
        let counts = counts_for(&comp);
        // Root epic is not counted — total equals the three children.
        assert_eq!(counts.total(), 3);
        assert_eq!(counts.closed, 1);
        assert_eq!(counts.in_progress, 1);
        assert_eq!(counts.open, 1);
    }

    #[test]
    fn pr_number_parses_gh_ref() {
        assert_eq!(pr_number(Some("gh-196")), Some(196));
        assert_eq!(pr_number(Some("gh-1")), Some(1));
        assert_eq!(pr_number(Some("gh-99999")), Some(99999));
    }

    #[test]
    fn pr_number_rejects_non_gh_refs() {
        assert_eq!(pr_number(Some("jira-SEL-1")), None);
        assert_eq!(pr_number(Some("linear-ABC-42")), None);
        assert_eq!(pr_number(Some("#196")), None);
    }

    #[test]
    fn pr_number_rejects_malformed() {
        assert_eq!(pr_number(Some("gh-")), None);
        assert_eq!(pr_number(Some("gh-abc")), None);
        assert_eq!(pr_number(Some("gh-1.2")), None);
        assert_eq!(pr_number(Some("gh--1")), None);
        assert_eq!(pr_number(Some("")), None);
    }

    #[test]
    fn pr_number_handles_none() {
        assert_eq!(pr_number(None), None);
    }

    #[test]
    fn pr_cell_pads_to_fixed_width() {
        assert_eq!(pr_cell(Some("gh-196")).len(), PR_CELL_WIDTH);
        assert_eq!(pr_cell(Some("gh-1")).len(), PR_CELL_WIDTH);
        assert_eq!(pr_cell(Some("gh-99999")).len(), PR_CELL_WIDTH);
        assert_eq!(pr_cell(None).len(), PR_CELL_WIDTH);
        assert_eq!(pr_cell(Some("jira-1")).len(), PR_CELL_WIDTH);
    }

    #[test]
    fn pr_cell_renders_visible_number_when_present() {
        assert!(pr_cell(Some("gh-196")).contains("#196"));
    }

    #[test]
    fn pr_cell_is_blank_when_absent_or_non_gh() {
        assert_eq!(pr_cell(None), " ".repeat(PR_CELL_WIDTH));
        assert_eq!(pr_cell(Some("jira-SEL-1")), " ".repeat(PR_CELL_WIDTH));
    }

    #[test]
    fn pr_cell_leaves_gap_for_five_digit_prs() {
        // A 5-digit PR like gh-10006 should still leave at least one
        // trailing space so the next column doesn't butt up against it.
        let cell = pr_cell(Some("gh-10006"));
        assert!(cell.contains("#10006"));
        assert!(cell.ends_with(' '), "expected trailing space, got {cell:?}");
    }
}
