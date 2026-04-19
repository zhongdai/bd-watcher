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

use bd_watcher::app::{App, Mode, View};
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

    /// TV mode: read-only, no selection or drill-in.
    #[arg(long)]
    tv: bool,

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
    let mode = if args.tv { Mode::Tv } else { Mode::Computer };
    let env_theme = std::env::var("BD_WATCHER_THEME").ok();
    let theme = theme::resolve(args.theme, env_theme.as_deref(), args.tv);

    let mut app = App::new(
        mode,
        theme,
        repo.clone(),
        args.epic_id.clone(),
        interval_secs,
    );

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

async fn handle_key(app: &mut App, key: KeyEvent, refresh_tx: &mpsc::Sender<()>) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.should_quit = true;
        return;
    }

    match app.mode {
        Mode::Tv => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            _ => {}
        },
        Mode::Computer => match app.view {
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
            View::Main => match key.code {
                KeyCode::Char('q') => app.should_quit = true,
                KeyCode::Char('r') => {
                    let _ = refresh_tx.send(()).await;
                }
                KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
                KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
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
            },
        },
    }
}
