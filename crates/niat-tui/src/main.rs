//! NIAT Terminal User Interface
//! Interactive console TUI interface for the intent-driven OS.

mod app;
mod ui;
mod agent_client;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
        EnableMouseCapture, Event, KeyCode, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tracing::info;

use app::{App, ConfigField, InputMode};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to file
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/var/log/niat/tui.log")
        .unwrap_or_else(|_| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/niat-tui.log")
                .expect("Cannot open log file")
        });

    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter("info")
        .with_target(false)
        .init();

    info!("NIAT TUI starting");

    // Setup terminal and thoroughly clear all text/scrollback to prevent overlap
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::Purge),
        crossterm::cursor::MoveTo(0, 0),
        EnterAlternateScreen,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create app and run
    let mut app = App::new();

    // Connect to agent kernel
    app.connect_agent().await;

    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        LeaveAlternateScreen,
        DisableMouseCapture,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    // Force system reboot on exit/quit/ctrl+c
    println!("\n[!] NIAT System Rebooting...");
    let _ = std::process::Command::new("/sbin/reboot").args(["-f"]).status();
    let _ = std::process::Command::new("/sbin/reboot").status();
    let _ = std::process::Command::new("reboot").args(["-f"]).status();
    let _ = std::process::Command::new("reboot").status();
    let _ = std::fs::write("/proc/sysrq-trigger", "b");

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        if app.should_quit || app.should_reboot {
            app.execute_reboot().await;
            return Ok(());
        }

        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Paste(text) => {
                    app.paste_text(&text);
                }
                Event::Key(key) => {
                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.execute_reboot().await;
                            return Ok(());
                        }
                        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.clear_messages();
                        }
                        KeyCode::F(3) => {
                            app.open_config_modal();
                        }
                        KeyCode::F(2) => {
                            // Shell breakout
                            disable_raw_mode()?;
                            execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen, DisableMouseCapture)?;

                            info!("Switching to fallback shell");
                            let _ = std::process::Command::new("/bin/sh").status();

                            enable_raw_mode()?;
                            execute!(
                                io::stdout(),
                                EnterAlternateScreen,
                                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                                EnableMouseCapture,
                                EnableBracketedPaste
                            )?;
                            terminal.clear()?;
                        }
                        KeyCode::Up | KeyCode::PageUp => {
                            app.scroll_up(3);
                        }
                        KeyCode::Down | KeyCode::PageDown => {
                            app.scroll_down(3);
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Editing;
                        }
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            app.handle_approval(true).await;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            app.handle_approval(false).await;
                        }
                        _ => {}
                    },
                    InputMode::Editing => {
                        if (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('q')) && key.modifiers.contains(KeyModifiers::CONTROL) {
                            app.execute_reboot().await;
                            return Ok(());
                        }
                        if key.code == KeyCode::Char('l') && key.modifiers.contains(KeyModifiers::CONTROL) {
                            app.clear_messages();
                            continue;
                        }

                        match key.code {
                            KeyCode::F(3) => {
                                app.open_config_modal();
                            }
                            KeyCode::F(2) => {
                                // Shell breakout
                                disable_raw_mode()?;
                                execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen, DisableMouseCapture)?;

                                info!("Switching to fallback shell");
                                let _ = std::process::Command::new("/bin/sh").status();

                                enable_raw_mode()?;
                                execute!(
                                    io::stdout(),
                                    EnterAlternateScreen,
                                    crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                                    EnableMouseCapture,
                                    EnableBracketedPaste
                                )?;
                                terminal.clear()?;
                            }
                            KeyCode::PageUp => {
                                app.scroll_up(5);
                            }
                            KeyCode::PageDown => {
                                app.scroll_down(5);
                            }
                            KeyCode::Enter => {
                                app.submit_input().await;
                            }
                            KeyCode::Backspace => {
                                app.input.pop();
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                            }
                            KeyCode::Char(c) => {
                                app.input.push(c);
                            }
                            _ => {}
                        }
                    }
                    InputMode::ConfigModal => {
                        if let Some(modal) = &mut app.config_modal {
                            match key.code {
                                KeyCode::Esc => {
                                    app.close_config_modal();
                                }
                                KeyCode::Tab | KeyCode::Down => {
                                    modal.active_field = match modal.active_field {
                                        ConfigField::BaseUrl => ConfigField::Model,
                                        ConfigField::Model => ConfigField::ApiKey,
                                        ConfigField::ApiKey => ConfigField::SaveButton,
                                        ConfigField::SaveButton => ConfigField::CancelButton,
                                        ConfigField::CancelButton => ConfigField::BaseUrl,
                                    };
                                }
                                KeyCode::BackTab | KeyCode::Up => {
                                    modal.active_field = match modal.active_field {
                                        ConfigField::BaseUrl => ConfigField::CancelButton,
                                        ConfigField::Model => ConfigField::BaseUrl,
                                        ConfigField::ApiKey => ConfigField::Model,
                                        ConfigField::SaveButton => ConfigField::ApiKey,
                                        ConfigField::CancelButton => ConfigField::SaveButton,
                                    };
                                }
                                KeyCode::Left | KeyCode::Right => {
                                    if modal.active_field == ConfigField::SaveButton {
                                        modal.active_field = ConfigField::CancelButton;
                                    } else if modal.active_field == ConfigField::CancelButton {
                                        modal.active_field = ConfigField::SaveButton;
                                    }
                                }
                                KeyCode::Enter => {
                                    if modal.active_field == ConfigField::CancelButton {
                                        app.close_config_modal();
                                    } else {
                                        app.save_config_modal().await;
                                    }
                                }
                                KeyCode::Backspace => {
                                    match modal.active_field {
                                        ConfigField::BaseUrl => { modal.base_url.pop(); }
                                        ConfigField::Model => { modal.model.pop(); }
                                        ConfigField::ApiKey => { modal.api_key.pop(); }
                                        _ => {}
                                    }
                                }
                                KeyCode::Char('t') | KeyCode::Char('T') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    modal.show_api_key = !modal.show_api_key;
                                }
                                KeyCode::Char(c) => {
                                    if !key.modifiers.contains(KeyModifiers::CONTROL) {
                                        match modal.active_field {
                                            ConfigField::BaseUrl => { modal.base_url.push(c); }
                                            ConfigField::Model => { modal.model.push(c); }
                                            ConfigField::ApiKey => { modal.api_key.push(c); }
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                }
                _ => {}
            }
        }

        // Process any pending responses from agent
        app.poll_responses().await;
    }
}
