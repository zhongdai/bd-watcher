use chrono::{DateTime, Local, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Mode};
use crate::model::{ActivityEvent, Component, Snapshot, Status, StatusCounts};
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
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<14}", comp.root.id),
                row_style.add_modifier(Modifier::BOLD).fg(theme.accent),
            ),
            Span::styled(format!("{:<48} ", title_trimmed), row_style),
            Span::styled(bar, Style::default().fg(theme.progress_filled)),
            Span::styled(format!("  {}/{}", counts.closed, total), row_style),
        ]));

        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::raw(" ".repeat(14)),
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

pub fn render_detail(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let Some(snap) = &app.snapshot else { return };
    let Some(comp) = snap.components.get(app.selected_epic) else {
        return;
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " {} · {} ",
            comp.root.id,
            truncate(&comp.root.title, 60)
        ))
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(3)])
        .split(inner);

    // Header: status, owner, external_ref
    let header_lines = vec![
        Line::from(vec![
            Span::styled("status: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{} {}", comp.root.status.icon(), comp.root.status.label()),
                Style::default()
                    .fg(status_color(theme, comp.root.status))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("type: ", Style::default().fg(theme.muted)),
            Span::styled(&comp.root.issue_type, Style::default().fg(theme.fg)),
            Span::raw("   "),
            Span::styled("priority: ", Style::default().fg(theme.muted)),
            Span::styled(
                comp.root.priority.to_string(),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled("owner: ", Style::default().fg(theme.muted)),
            Span::styled(
                comp.root.owner.as_deref().unwrap_or("—"),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled("external: ", Style::default().fg(theme.muted)),
            Span::styled(
                comp.root.external_ref.as_deref().unwrap_or("—"),
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "description",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::UNDERLINED),
        )),
    ];
    let header_p = Paragraph::new(header_lines).style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(header_p, chunks[0]);

    // Body split: left description, right children
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[1]);

    let desc = Paragraph::new(comp.root.description.clone())
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(desc, body[0]);

    let child_block = Block::default()
        .borders(Borders::LEFT)
        .title(" children ")
        .border_style(Style::default().fg(theme.muted));
    let child_inner = child_block.inner(body[1]);
    frame.render_widget(child_block, body[1]);

    let mut child_lines: Vec<Line> = Vec::new();
    for issue in comp.issues.iter().filter(|i| i.id != comp.root.id) {
        let blocked_by: Vec<&str> = comp
            .dependencies
            .iter()
            .filter(|d| {
                d.issue_id == issue.id && matches!(d.dep_type, crate::model::DepType::Blocks)
            })
            .map(|d| d.depends_on_id.as_str())
            .collect();
        child_lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", issue.status.icon()),
                Style::default().fg(status_color(theme, issue.status)),
            ),
            Span::styled(
                format!("{:<14}", issue.id),
                Style::default().fg(theme.accent),
            ),
            Span::styled(truncate(&issue.title, 50), Style::default().fg(theme.fg)),
        ]));
        if !blocked_by.is_empty() {
            child_lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(
                    format!("blocked-by: {}", blocked_by.join(", ")),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
    }
    if child_lines.is_empty() {
        child_lines.push(Line::from(Span::styled(
            " (no children) ",
            Style::default().fg(theme.muted),
        )));
    }
    let children_p = Paragraph::new(child_lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(children_p, child_inner);
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
