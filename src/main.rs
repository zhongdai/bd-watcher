use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio::time;

use bd_watcher::app::{App, FocusedPane, View};
use bd_watcher::bd::BdRunner;
use bd_watcher::clipboard;
use bd_watcher::diff::diff;
use bd_watcher::model::{ActivityEvent, Snapshot};
use bd_watcher::theme::{self, ThemeName};
use bd_watcher::ui;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "bd-watcher",
    version,
    about = "Watch bd (beads) graph progress in a TUI"
)]
struct Args {
    /// Optional epic id to focus on. Omit to show all open components.
    epic_id: Option<String>,

    /// Poll interval in seconds.
    #[arg(long, default_value_t = 5)]
    interval: u64,

    /// Directory to run `bd` from (defaults to the current directory).
    #[arg(long)]
    repo: Option<PathBuf>,

    /// Color theme.
    #[arg(long, value_enum)]
    theme: Option<ThemeName>,
}

enum PollerMsg {
    Snapshot {
        snapshot: Snapshot,
        events: Vec<ActivityEvent>,
    },
    Error(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Err(err) = BdRunner::check_available().await {
        eprintln!("bd-watcher: {err}");
        eprintln!("Install `bd` (beads) and ensure it's on PATH.");
        std::process::exit(1);
    }

    let repo = match args.repo.clone() {
        Some(p) => p,
        None => std::env::current_dir().context("failed to read current working directory")?,
    };
    let interval_secs = args.interval.max(1);
    let env_theme = std::env::var("BD_WATCHER_THEME").ok();
    let theme = theme::resolve(args.theme, env_theme.as_deref());

    // Pre-flight fetch: catch non-bd repos and unknown epic ids BEFORE we
    // swap to the alternate screen, so we can print a useful message and
    // exit 1 instead of flashing a vague error banner in the TUI.
    let preflight_runner = BdRunner::new(repo.clone(), args.epic_id.clone());
    match preflight_runner.fetch().await {
        Ok(snap) => {
            // When focusing a bead, reject anything that isn't an epic —
            // the single-epic layered view is designed around epic+children.
            if let Some(id) = &args.epic_id {
                if let Some(root) = snap.components.first().map(|c| &c.root) {
                    if root.issue_type != "epic" {
                        eprintln!("bd-watcher: '{id}' is a {}, not an epic", root.issue_type);
                        eprintln!("The focused view requires an epic. Run `bd list --type epic` to see available epics.");
                        std::process::exit(1);
                    }
                }
            }
        }
        Err(err) => {
            let msg = format!("{err:#}").to_lowercase();
            if msg.contains("no beads database") {
                eprintln!("bd-watcher: no beads database in {}", repo.display());
                eprintln!("Run `bd init` in that directory, or pass --repo <path>.");
                std::process::exit(1);
            }
            if let Some(id) = &args.epic_id {
                if msg.contains("not found") {
                    eprintln!("bd-watcher: epic '{id}' not found in {}", repo.display());
                    eprintln!("Run `bd list --type epic` to see available epics.");
                    std::process::exit(1);
                }
            }
            eprintln!("bd-watcher: {err:#}");
            std::process::exit(1);
        }
    }

    let mut app = App::new(theme, repo.clone(), args.epic_id.clone(), interval_secs);
    // Best-effort GitHub owner/repo lookup for the `v` keybinding.
    // Silent failure is fine — `v` will show a toast if the repo
    // doesn't have a github.com `origin`.
    app.gh_repo = bd_watcher::gh::detect(&repo).await;

    let (tx, mut rx) = mpsc::channel::<PollerMsg>(16);
    let (refresh_tx, mut refresh_rx) = mpsc::channel::<()>(4);

    let runner = BdRunner::new(repo, args.epic_id);
    let poller_tx = tx.clone();
    let poller_handle = tokio::spawn(async move {
        let mut prev: Option<Snapshot> = None;
        let mut ticker = time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                maybe = refresh_rx.recv() => {
                    if maybe.is_none() {
                        break;
                    }
                }
            }
            match runner.fetch().await {
                Ok(snapshot) => {
                    let events = diff(prev.as_ref(), &snapshot);
                    prev = Some(snapshot.clone());
                    if poller_tx
                        .send(PollerMsg::Snapshot { snapshot, events })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    if poller_tx
                        .send(PollerMsg::Error(format!("{err:#}")))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    let mut terminal = setup_terminal().context("failed to set up terminal")?;

    let (input_tx, mut input_rx) = mpsc::channel::<Event>(64);
    let (input_shutdown_tx, input_shutdown_rx) = std::sync::mpsc::channel::<()>();
    let input_handle = std::thread::spawn(move || input_loop(input_tx, input_shutdown_rx));

    let result = run(&mut terminal, &mut app, &mut rx, &mut input_rx, &refresh_tx).await;
    restore_terminal(&mut terminal).ok();

    let _ = input_shutdown_tx.send(());
    drop(refresh_tx);
    drop(tx);
    poller_handle.abort();
    let _ = poller_handle.await;
    let _ = input_handle.join();

    result
}

fn input_loop(tx: mpsc::Sender<Event>, shutdown: std::sync::mpsc::Receiver<()>) {
    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }
        match event::poll(Duration::from_millis(200)) {
            Ok(true) => match event::read() {
                Ok(ev) => {
                    if tx.blocking_send(ev).is_err() {
                        break;
                    }
                }
                // Transient read error: back off and retry. Keeps the thread
                // alive under degenerate stdin (e.g., headless recorders) so
                // the render loop can keep running for the TV dashboard.
                Err(_) => std::thread::sleep(Duration::from_millis(500)),
            },
            Ok(false) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    rx: &mut mpsc::Receiver<PollerMsg>,
    input_rx: &mut mpsc::Receiver<Event>,
    refresh_tx: &mpsc::Sender<()>,
) -> Result<()> {
    terminal.draw(|f| ui::render(app, f))?;

    // Fire an immediate first refresh.
    let _ = refresh_tx.send(()).await;

    loop {
        tokio::select! {
            biased;
            maybe_msg = rx.recv() => {
                match maybe_msg {
                    Some(PollerMsg::Snapshot { snapshot, events }) => {
                        app.apply_snapshot(snapshot, events);
                    }
                    Some(PollerMsg::Error(msg)) => {
                        app.apply_error(msg);
                    }
                    None => break,
                }
            }
            maybe_event = input_rx.recv() => {
                match maybe_event {
                    Some(Event::Key(key)) => {
                        if key.kind == KeyEventKind::Release {
                            continue;
                        }
                        handle_key(app, key, refresh_tx).await;
                    }
                    Some(Event::Resize(_, _)) => {}
                    // Input channel closed: keep rendering on poller events.
                    // Useful under headless recorders / broken stdin.
                    None => tokio::time::sleep(Duration::from_secs(3600)).await,
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
        terminal.draw(|f| ui::render(app, f))?;
    }
    Ok(())
}

/// Opens the selected sub-bead's PR in the browser, reporting outcome
/// (success, missing PR, missing origin, spawn error) via the footer
/// toast. No-op when nothing is selected.
fn open_selected_pr(app: &mut App) {
    let Some(issue) = app.selected_sub_bead() else {
        return;
    };
    let pr = match bd_watcher::gh::parse_pr_number(issue.external_ref.as_deref()) {
        Some(n) => n,
        None => {
            app.set_toast("no PR for this bead".to_string());
            return;
        }
    };
    let gh = match app.gh_repo.as_ref() {
        Some(g) => g.clone(),
        None => {
            app.set_toast("no github.com origin on this repo".to_string());
            return;
        }
    };
    let url = gh.pr_url(pr);
    match bd_watcher::gh::open_in_browser(&url) {
        Ok(_) => app.set_toast(format!("opened PR #{pr}")),
        Err(e) => app.set_toast(format!("open failed: {e}")),
    }
}

async fn handle_key(app: &mut App, key: KeyEvent, refresh_tx: &mpsc::Sender<()>) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.should_quit = true;
        return;
    }

    match app.view {
        View::Filter => match key.code {
            KeyCode::Esc => {
                app.filter.clear();
                app.view = View::Main;
            }
            KeyCode::Enter => {
                app.view = View::Main;
            }
            KeyCode::Backspace => {
                app.filter.pop();
            }
            KeyCode::Char(c) => {
                app.filter.push(c);
            }
            _ => {}
        },
        View::BeadDetail => match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                app.view = View::Main;
            }
            KeyCode::Down | KeyCode::Char('j') => app.scroll_popup(1),
            KeyCode::Up | KeyCode::Char('k') => app.scroll_popup(-1),
            KeyCode::PageDown => app.scroll_popup(10),
            KeyCode::PageUp => app.scroll_popup(-10),
            KeyCode::Home => app.popup_scroll = 0,
            _ => {}
        },
        View::Main => {
            // `gg` chord handling: a lone `g` arms; the next `g` jumps to
            // top, any other key disarms.
            let was_pending_g = app.pending_g;
            app.pending_g = false;
            if app.focus.is_some() {
                // Focused-epic view. Tab cycles focus between the task
                // list and the activity pane; directional keys then
                // act on whichever pane has focus.
                match key.code {
                    KeyCode::Tab => app.toggle_focused_pane(),
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Char('r') => {
                        let _ = refresh_tx.send(()).await;
                    }
                    // Enter opens detail on the selected task no matter
                    // which pane has focus — saves a Tab round-trip.
                    KeyCode::Enter if app.selected_sub_bead().is_some() => {
                        app.open_bead_detail();
                    }
                    // y always copies the selected task id too.
                    KeyCode::Char('y') => {
                        if let Some(id) = app.selected_sub_bead().map(|i| i.id.clone()) {
                            match clipboard::copy(&id) {
                                Ok(_) => app.set_toast(format!("copied {id}")),
                                Err(e) => app.set_toast(format!("copy failed: {e}")),
                            }
                        }
                    }
                    // v opens the selected task's PR in the default browser.
                    KeyCode::Char('v') => open_selected_pr(app),
                    _ => match app.focused_pane {
                        FocusedPane::Tasks => match key.code {
                            KeyCode::Char('g') => {
                                if was_pending_g {
                                    app.jump_first_sub();
                                } else {
                                    app.pending_g = true;
                                }
                            }
                            KeyCode::Char('G') => app.jump_last_sub(),
                            KeyCode::Down | KeyCode::Char('j') => app.move_sub_selection(1),
                            KeyCode::Up | KeyCode::Char('k') => app.move_sub_selection(-1),
                            KeyCode::Home => app.jump_first_sub(),
                            KeyCode::End => app.jump_last_sub(),
                            _ => {}
                        },
                        FocusedPane::Activity => match key.code {
                            KeyCode::Char('g') => {
                                if was_pending_g {
                                    app.jump_activity_bottom();
                                } else {
                                    app.pending_g = true;
                                }
                            }
                            KeyCode::Char('G') => app.jump_activity_top(),
                            KeyCode::Down | KeyCode::Char('j') => app.scroll_activity(-1),
                            KeyCode::Up | KeyCode::Char('k') => app.scroll_activity(1),
                            KeyCode::PageDown => app.scroll_activity(-10),
                            KeyCode::PageUp => app.scroll_activity(10),
                            KeyCode::Home => app.jump_activity_bottom(),
                            KeyCode::End => app.jump_activity_top(),
                            _ => {}
                        },
                    },
                }
            } else {
                // All-epics view (unchanged).
                match key.code {
                    KeyCode::Char('g') => {
                        if was_pending_g {
                            app.jump_to_top();
                        } else {
                            app.pending_g = true;
                        }
                    }
                    KeyCode::Char('G') => app.jump_to_bottom(),
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Char('r') => {
                        let _ = refresh_tx.send(()).await;
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
                    KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
                    KeyCode::Home => app.jump_to_top(),
                    KeyCode::End => app.jump_to_bottom(),
                    KeyCode::Char('/') => {
                        app.filter.clear();
                        app.view = View::Filter;
                    }
                    KeyCode::Char('y') => {
                        if let Some(id) = app.selected_epic_id() {
                            match clipboard::copy(&id) {
                                Ok(_) => app.set_toast(format!("copied {id}")),
                                Err(e) => app.set_toast(format!("copy failed: {e}")),
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
