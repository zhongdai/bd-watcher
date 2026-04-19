use chrono::{DateTime, Local, Utc};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Mode};
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

/// Most recent status-change time across the component's issues, as observed
/// by this process (via diff of successive snapshots). Falls back to the
/// max `updated_at` of the issues when no status change has been seen yet —
/// that keeps initial ordering reasonable before any changes have happened.
fn component_latest_status_change(app: &App, component: &Component) -> DateTime<Utc> {
    if let Some(t) = component
        .issues
        .iter()
        .filter_map(|i| app.last_status_change.get(&i.id).copied())
        .max()
    {
        return t;
    }
    component
        .issues
        .iter()
        .map(|i| i.updated_at)
        .max()
        .unwrap_or(component.root.updated_at)
}

pub fn counts_for(component: &Component) -> StatusCounts {
    let mut c = StatusCounts::default();
    let mut seen = std::collections::HashSet::new();
    for i in &component.issues {
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

    let mut indices = app.filtered_epic_indices(snap);
    if app.mode == Mode::Tv {
        indices.sort_by_key(|&i| {
            std::cmp::Reverse(component_latest_status_change(app, &snap.components[i]))
        });
    }
    if indices.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            " no epics match filter ",
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Activity (last {}) ", app.activity.len()))
        .border_style(Style::default().fg(theme.muted))
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
    let events: Vec<&ActivityEvent> = app.activity.iter().rev().take(rows).collect();

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
    let total_tasks = counts.total().saturating_sub(1); // exclude the root epic if counted
    let title_line = format!(
        " {} · {} · {}/{} done ",
        comp.root.id,
        truncate(&comp.root.title, area.width.saturating_sub(30) as usize),
        counts.closed,
        counts.total(),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_line)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layers = compute_layers(comp);
    let inner_width = inner.width as usize;
    let id_col = 16usize;
    // Reserve room for " [icon] id  " prefix + "  ← deps" suffix.
    let title_width = inner_width.saturating_sub(id_col + 6 + 20).max(10);

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
        status_count_span(
            theme,
            Status::Open,
            counts
                .open
                .saturating_sub(if comp.root.status == Status::Open {
                    1
                } else {
                    0
                }),
        ),
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
            lines.push(Line::from(spans));
        }
    }

    let p = Paragraph::new(lines).style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(p, inner);
}
