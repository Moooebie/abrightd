//! A minimal live brightness indicator.
//!
//! Reads the running daemon's `org.abrightd` `Status` snapshot and renders the
//! ambient lux, the chosen backlight level and the current adjustment
//! direction.  Rendering is done with `ratatui`, which draws into a buffer
//! clipped to the terminal size, so the display always refreshes in place and
//! never wraps.  Enabled with the `tui` feature.

use std::collections::HashMap;
use std::io::Stdout;
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;
use zbus::{Connection, Proxy};

type Backend = CrosstermBackend<Stdout>;

/// Fixed width of the brightness bar, regardless of terminal width.
const BAR_WIDTH: usize = 20;

/// Run the indicator until the user quits (`q`, `Esc` or `Ctrl-C`).
pub async fn run(interval_ms: u64) -> anyhow::Result<()> {
    let connection = Connection::session()
        .await
        .context("connecting to the session bus")?;
    let proxy = Proxy::new(&connection, "org.abrightd", "/org/abrightd", "org.abrightd").await?;

    enable_raw_mode().context("enabling raw mode (is this a terminal?)")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = run_loop(&mut terminal, &proxy, interval_ms.max(50)).await;

    // Always restore the terminal.
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
    result
}

async fn run_loop(
    terminal: &mut Terminal<Backend>,
    proxy: &Proxy<'_>,
    interval_ms: u64,
) -> anyhow::Result<()> {
    let mut prev_raw: Option<f32> = None;

    loop {
        tokio::time::sleep(Duration::from_millis(interval_ms)).await;

        // Drain pending key events without blocking.
        while event::poll(Duration::ZERO)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let quit = matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL));
                    if quit {
                        return Ok(());
                    }
                }
            }
        }

        match proxy
            .call::<_, _, HashMap<String, String>>("Status", &())
            .await
        {
            Ok(status) => {
                draw(&mut *terminal, &status, prev_raw)?;
                if let Some(raw) = get(&status, "last_observed_lux") {
                    prev_raw = Some(raw);
                }
            }
            Err(err) => draw_waiting(&mut *terminal, &err.to_string())?,
        }
    }
}

fn draw(
    terminal: &mut Terminal<Backend>,
    status: &HashMap<String, String>,
    prev_raw: Option<f32>,
) -> anyhow::Result<()> {
    let lux = get(status, "lux");
    let raw = get(status, "last_observed_lux");
    let slow = get(status, "slow_lux");
    let fast = get(status, "fast_lux");
    let target = get(status, "controller_brightness");
    let output = get(status, "output_brightness");
    let enabled = status.get("enabled").map(String::as_str).unwrap_or("?");
    let profile = status.get("profile").map(String::as_str).unwrap_or("?");
    let learned = status
        .get("user_points")
        .map(|v| !v.is_empty())
        .unwrap_or(false);

    let ratio = output.unwrap_or(0.0).clamp(0.0, 1.0);
    let light = trend(prev_raw, raw);
    let adjust = ramping(target, output);

    terminal.draw(|frame| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title
                Constraint::Length(1), // sensor (latest sample)
                Constraint::Length(1), // ambient (accepted + slow/fast)
                Constraint::Length(2), // "brightness" + bar
                Constraint::Length(1), // target
                Constraint::Length(1), // adjust
                Constraint::Length(1), // state
                Constraint::Min(0),    // spacer
                Constraint::Length(1), // help
            ])
            .split(frame.area());

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("abrightd", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  ·  AOSP auto-brightness"),
            ])),
            chunks[0],
        );

        frame.render_widget(
            Paragraph::new(Line::from(format!(
                "sensor   {}   (latest sample)",
                fmt_lux(raw)
            ))),
            chunks[1],
        );

        frame.render_widget(
            Paragraph::new(Line::from(format!(
                "ambient  {}   slow {}  fast {}   {}",
                fmt_lux(lux),
                fmt_num(slow),
                fmt_num(fast),
                light
            ))),
            chunks[2],
        );

        // Bar on its own line, percentage at the right edge of the bar.
        let filled = filled_cells(ratio);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("brightness"),
                Line::from(vec![
                    Span::raw("  "),
                    Span::raw("█".repeat(filled)),
                    Span::raw(" ".repeat(BAR_WIDTH.saturating_sub(filled))),
                    Span::raw(format!("  {:>5.1}%", ratio * 100.0)),
                ]),
            ]),
            chunks[3],
        );

        frame.render_widget(
            Paragraph::new(Line::from(format!(
                "target {}{}",
                fmt_pct(target),
                if learned { "   learned" } else { "" }
            ))),
            chunks[4],
        );

        frame.render_widget(
            Paragraph::new(Line::from(format!("adjust   {adjust}"))),
            chunks[5],
        );

        frame.render_widget(
            Paragraph::new(Line::from(format!("enabled={enabled}  profile={profile}"))),
            chunks[6],
        );

        frame.render_widget(
            Paragraph::new(Line::from("q / Esc / Ctrl-C to quit")),
            chunks[8],
        );
    })?;
    Ok(())
}

fn draw_waiting(terminal: &mut Terminal<Backend>, err: &str) -> anyhow::Result<()> {
    terminal.draw(|frame| {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("abrightd", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("  ·  AOSP auto-brightness"),
                ]),
                Line::from(""),
                Line::from("waiting for the org.abrightd service on the session bus…"),
                Line::from(err.to_string()),
                Line::from(""),
                Line::from("q / Esc / Ctrl-C to quit"),
            ]),
            frame.area(),
        );
    })?;
    Ok(())
}

fn get(status: &HashMap<String, String>, key: &str) -> Option<f32> {
    status.get(key).and_then(|v| v.parse().ok())
}

fn filled_cells(ratio: f32) -> usize {
    let ratio = if ratio.is_nan() {
        0.0
    } else {
        ratio.clamp(0.0, 1.0)
    };
    (ratio * BAR_WIDTH as f32).round() as usize
}

fn fmt_lux(value: Option<f32>) -> String {
    match value {
        Some(v) if !v.is_nan() => {
            if v.abs() < 10.0 {
                format!("{v:.3} lx")
            } else {
                format!("{v:.1} lx")
            }
        }
        _ => "— lx".to_string(),
    }
}

fn fmt_num(value: Option<f32>) -> String {
    match value {
        Some(v) if !v.is_nan() => {
            if v.abs() < 10.0 {
                format!("{v:.3}")
            } else {
                format!("{v:.1}")
            }
        }
        _ => "—".to_string(),
    }
}

fn fmt_pct(value: Option<f32>) -> String {
    match value {
        Some(v) if !v.is_nan() => format!("{:.1}%", v * 100.0),
        _ => "—".to_string(),
    }
}

fn trend(prev: Option<f32>, current: Option<f32>) -> &'static str {
    match (prev, current) {
        (Some(p), Some(c)) if c - p > 0.5 => "▲ rising",
        (Some(p), Some(c)) if c - p < -0.5 => "▼ falling",
        _ => "● steady",
    }
}

fn ramping(target: Option<f32>, output: Option<f32>) -> &'static str {
    match (target, output) {
        (Some(t), Some(o)) if !t.is_nan() && !o.is_nan() => {
            if t - o > 0.002 {
                "▲ brightening"
            } else if t - o < -0.002 {
                "▼ darkening"
            } else {
                "● steady"
            }
        }
        _ => "● steady",
    }
}
