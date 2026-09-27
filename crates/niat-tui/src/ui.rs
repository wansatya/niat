//! TUI rendering — modern opencode-style minimal aesthetic.

use crate::app::{App, ConfigField, InputMode, MessageSender, PendingConfirmation};
use niat_common::types::{SafetyLevel, NIAT_VERSION};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

const FG: Color = Color::Rgb(226, 232, 240);
const DIM: Color = Color::Rgb(100, 116, 139);
const FAINT: Color = Color::Rgb(71, 85, 105);
const ACCENT: Color = Color::Rgb(165, 180, 252);
const ACCENT_DIM: Color = Color::Rgb(129, 140, 248);
const GREEN: Color = Color::Rgb(134, 239, 172);
const YELLOW: Color = Color::Rgb(253, 224, 71);
const RED: Color = Color::Rgb(252, 165, 165);
const PURPLE: Color = Color::Rgb(216, 180, 254);
const BORDER: Color = Color::Rgb(51, 65, 85);
const BORDER_FOCUS: Color = Color::Rgb(100, 116, 139);

fn dim() -> Style {
    Style::default().fg(DIM)
}
fn faint() -> Style {
    Style::default().fg(FAINT)
}

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_messages(f, app, chunks[1]);
    draw_permission_line(f, app, chunks[2]);
    let cursor = draw_input(f, app, chunks[3]);
    draw_footer(f, app, chunks[4]);

    if let Some(m) = &app.config_modal {
        draw_config_modal(f, m);
    } else if let Some(c) = &app.pending_confirm {
        draw_confirm_modal(f, c);
    } else if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let (dot, dot_color, net_label) = if app.connected {
        ("●", GREEN, "connected")
    } else {
        ("○", RED, "offline")
    };
    let left = Line::from(vec![
        Span::styled(" niat", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" v{} ", NIAT_VERSION), dim()),
        Span::styled("· ", faint()),
        Span::styled(format!("{} {} ", dot, net_label), Style::default().fg(dot_color)),
        Span::styled("· ", faint()),
        Span::styled(app.config.model.model.clone(), Style::default().fg(ACCENT)),
    ]);
    let right_text = match &app.status {
        Some(s) => {
            let mem = format!("{} / {} MB", s.mem_used_mb, s.mem_total_mb);
            match (&s.network_interface, &s.network_ip) {
                (Some(iface), Some(ip)) => format!("{} {} · {} ", iface, ip, mem),
                _ => format!("{} ", mem),
            }
        }
        None => String::new(),
    };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(32)])
        .split(area);
    f.render_widget(Paragraph::new(left), cols[0]);
    f.render_widget(Paragraph::new(Line::from(Span::styled(right_text, faint()))).alignment(Alignment::Right), cols[1]);
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    // Lean transcript: no side borders or boxes, only faint row dividers.
    let divider = "─".repeat(area.width as usize);
    let mut lines: Vec<Line> = Vec::new();

    for (i, msg) in app.messages.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(Span::styled(divider.as_str(), faint())));
        }

        let mut content_lines: Vec<&str> = msg.content.lines().collect();
        if content_lines.is_empty() {
            content_lines.push("");
        }

        match &msg.sender {
            MessageSender::User => {
                for (j, content_line) in content_lines.iter().enumerate() {
                    let prefix = if j == 0 { "❯ " } else { "  " };
                    lines.push(Line::from(vec![
                        Span::styled(
                            prefix,
                            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(*content_line, Style::default().fg(FG)),
                    ]));
                }
            }
            MessageSender::Agent => {
                for content_line in &content_lines {
                    lines.push(Line::from(Span::styled(
                        *content_line,
                        Style::default().fg(FG),
                    )));
                }
            }
            MessageSender::System => {
                for content_line in &content_lines {
                    lines.push(Line::from(vec![
                        Span::styled("· ", faint()),
                        Span::styled(*content_line, dim()),
                    ]));
                }
            }
            MessageSender::Success => {
                for (j, content_line) in content_lines.iter().enumerate() {
                    let prefix = if j == 0 { "✓ " } else { "  " };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(GREEN)),
                        Span::styled(*content_line, Style::default().fg(FG)),
                    ]));
                }
            }
            MessageSender::Error => {
                for (j, content_line) in content_lines.iter().enumerate() {
                    let prefix = if j == 0 { "✗ " } else { "  " };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(RED)),
                        Span::styled(*content_line, Style::default().fg(FG)),
                    ]));
                }
            }
            MessageSender::Tool(name) => {
                lines.push(Line::from(vec![
                    Span::styled("⚙ ", Style::default().fg(PURPLE)),
                    Span::styled(
                        name.as_str(),
                        Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
                    ),
                ]));
                for content_line in &content_lines {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(*content_line, dim()),
                    ]));
                }
            }
        }
    }

    let messages = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));

    f.render_widget(messages, area);
}

fn draw_permission_line(f: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(confirm) = &app.pending_confirm {
        Line::from(vec![
            Span::styled(
                " ⚠ approval needed: ",
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                confirm.tool_name.clone(),
                Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" — {} ", confirm.description), dim()),
            Span::styled("[Y] approve", Style::default().fg(GREEN)),
            Span::styled(" · ", faint()),
            Span::styled("[N] deny", Style::default().fg(RED)),
        ])
    } else if !app.connected {
        Line::from(vec![
            Span::styled(" ○ standalone", faint()),
            Span::styled(
                " — agent offline, F2 for shell · :config to setup · :help for commands",
                dim(),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(" ◆ ", Style::default().fg(ACCENT_DIM)),
            Span::styled("intent mode", dim()),
            Span::styled(" — type below · Enter to send · F3 config · F2 shell", faint()),
        ])
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) -> Option<Position> {
    // Lean composer: a top row-border only, no side borders or boxes.
    let (rule_color, placeholder) = match app.input_mode {
        InputMode::Normal => (BORDER, "Press Enter to type..."),
        InputMode::Editing => (BORDER_FOCUS, "State your intent or type :help..."),
        InputMode::ConfigModal => (BORDER, "Configuration in progress..."),
    };

    let content = if app.input.is_empty() {
        Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(placeholder, dim()),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.input.as_str(), Style::default().fg(FG)),
        ])
    };

    let input = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(rule_color)),
    );
    f.render_widget(input, area);

    if !matches!(app.input_mode, InputMode::Editing) {
        return None;
    }

    // Cursor at the end of the input, on the line below the top rule.
    // The visible prefix is "❯ " (2 cells).
    let input_width: u16 = app.input.chars().count().min(u16::MAX as usize) as u16;
    let x = (area.x + 2 + input_width).min(area.x + area.width.saturating_sub(1));
    Some(Position::new(x, area.y + 1))
}

fn draw_footer(f: &mut Frame, _app: &App, area: Rect) {
    let shortcuts = Line::from(vec![
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Send   ", Style::default().fg(Color::White)),
        Span::styled(
            " F3 ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(147, 51, 234))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Config   ", Style::default().fg(Color::White)),
        Span::styled(
            " F2 ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Shell   ", Style::default().fg(Color::White)),
        Span::styled(
            " :reboot ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Reboot   ", Style::default().fg(Color::White)),
        Span::styled(
            " Ctrl+C / :quit ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Force Reboot   ", Style::default().fg(Color::White)),
        Span::styled(
            " :help ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Help ", Style::default().fg(Color::White)),
    ]);

    let footer = Paragraph::new(shortcuts)
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Rgb(15, 20, 32)));
    f.render_widget(footer, area);
}

fn draw_config_modal(f: &mut Frame, modal: &crate::app::ConfigModalState) {
    let area = f.area();
    let modal_width = 74.min(area.width.saturating_sub(4));
    let modal_height = 19.min(area.height.saturating_sub(2));

    let modal_area = Rect::new(
        (area.width - modal_width) / 2,
        (area.height - modal_height) / 2,
        modal_width,
        modal_height,
    );

    f.render_widget(Clear, modal_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title / Desc
            Constraint::Length(3), // URL
            Constraint::Length(3), // Model
            Constraint::Length(3), // API Key
            Constraint::Length(3), // Buttons
            Constraint::Length(1), // Hint
        ])
        .margin(1)
        .split(modal_area);

    // Frame box
    let frame_block = Block::default()
        .title(Span::styled(
            " ⚙ NIAT Model & API Configuration ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Color::Rgb(20, 27, 45)));
    f.render_widget(frame_block, modal_area);

    // Description
    let desc = Paragraph::new(Line::from(vec![
        Span::styled(
            " Configure OpenAI-compatible LLM endpoint, model name, and API key:",
            Style::default().fg(Color::Rgb(200, 220, 255)),
        ),
    ]));
    f.render_widget(desc, chunks[0]);

    // Field 1: Base URL
    let url_active = modal.active_field == ConfigField::BaseUrl;
    let url_block = Block::default()
        .title(if url_active { " ▶ API Base URL " } else { " API Base URL " })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if url_active { Color::LightGreen } else { Color::Rgb(71, 85, 105) }))
        .style(Style::default().bg(Color::Rgb(12, 16, 26)));
    let url_text = if modal.base_url.is_empty() && url_active {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("https://api.openai.com/v1", Style::default().fg(Color::DarkGray)),
            Span::styled("█", Style::default().fg(Color::LightGreen)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(&modal.base_url, Style::default().fg(Color::White)),
            if url_active { Span::styled("█", Style::default().fg(Color::LightGreen)) } else { Span::raw("") },
        ])
    };
    f.render_widget(Paragraph::new(url_text).block(url_block), chunks[1]);

    // Field 2: Model Name
    let model_active = modal.active_field == ConfigField::Model;
    let model_block = Block::default()
        .title(if model_active { " ▶ Model Name " } else { " Model Name " })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if model_active { Color::LightGreen } else { Color::Rgb(71, 85, 105) }))
        .style(Style::default().bg(Color::Rgb(12, 16, 26)));
    let model_text = if modal.model.is_empty() && model_active {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("gpt-4o", Style::default().fg(Color::DarkGray)),
            Span::styled("█", Style::default().fg(Color::LightGreen)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(&modal.model, Style::default().fg(Color::White)),
            if model_active { Span::styled("█", Style::default().fg(Color::LightGreen)) } else { Span::raw("") },
        ])
    };
    f.render_widget(Paragraph::new(model_text).block(model_block), chunks[2]);

    // Field 3: API Key
    let key_active = modal.active_field == ConfigField::ApiKey;
    let key_block = Block::default()
        .title(if key_active { " ▶ API Key (sk-...) [Ctrl+T to toggle visibility] " } else { " API Key (sk-...) " })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if key_active { Color::LightGreen } else { Color::Rgb(71, 85, 105) }))
        .style(Style::default().bg(Color::Rgb(12, 16, 26)));

    let masked_key = if modal.show_api_key {
        modal.api_key.clone()
    } else if modal.api_key.is_empty() {
        String::new()
    } else {
        "•".repeat(modal.api_key.len().min(40))
    };

    let key_text = if modal.api_key.is_empty() && key_active {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("sk-...", Style::default().fg(Color::DarkGray)),
            Span::styled("█", Style::default().fg(Color::LightGreen)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(masked_key, Style::default().fg(Color::LightYellow)),
            if key_active { Span::styled("█", Style::default().fg(Color::LightGreen)) } else { Span::raw("") },
        ])
    };
    f.render_widget(Paragraph::new(key_text).block(key_block), chunks[3]);

    // Buttons
    let save_active = modal.active_field == ConfigField::SaveButton;
    let cancel_active = modal.active_field == ConfigField::CancelButton;

    let buttons_line = Line::from(vec![
        Span::raw("    "),
        Span::styled(
            if save_active { " ▶ [ Save & Apply (Enter) ] ◀ " } else { "   [ Save & Apply ]   " },
            Style::default()
                .fg(if save_active { Color::Black } else { Color::White })
                .bg(if save_active { Color::LightGreen } else { Color::Rgb(22, 101, 52) })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("        "),
        Span::styled(
            if cancel_active { " ▶ [ Cancel (Esc) ] ◀ " } else { "   [ Cancel ]   " },
            Style::default()
                .fg(if cancel_active { Color::Black } else { Color::White })
                .bg(if cancel_active { Color::LightRed } else { Color::Rgb(153, 27, 27) })
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(buttons_line).alignment(Alignment::Center), chunks[4]);

    // Modal Helper Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            "[Tab / ↑↓] Switch Field │ [Enter] Save & Apply │ [Esc] Close",
            Style::default().fg(Color::Rgb(148, 163, 184)),
        ),
    ])).alignment(Alignment::Center);
    f.render_widget(hint, chunks[5]);
}

fn draw_confirm_modal(f: &mut Frame, confirm: &PendingConfirmation) {
    let area = f.area();
    let modal_width = 66.min(area.width.saturating_sub(4));
    let modal_height = 15.min(area.height.saturating_sub(4));

    let modal_area = Rect::new(
        (area.width - modal_width) / 2,
        (area.height - modal_height) / 2,
        modal_width,
        modal_height,
    );

    f.render_widget(Clear, modal_area);

    let safety_style = match confirm.safety_level {
        SafetyLevel::Safe => Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD),
        SafetyLevel::Confirm => Style::default().fg(Color::Black).bg(Color::LightYellow).add_modifier(Modifier::BOLD),
        SafetyLevel::Dangerous => Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD),
    };

    let safety_label = match confirm.safety_level {
        SafetyLevel::Safe => " ✔ SAFE OPERATION ",
        SafetyLevel::Confirm => " ⚠ REQUIRES CONFIRMATION ",
        SafetyLevel::Dangerous => " ⛔ DANGEROUS OPERATION ",
    };

    let params_str = serde_json::to_string(&confirm.params).unwrap_or_default();

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Status: "),
            Span::styled(safety_label, safety_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  Target Tool: "),
            Span::styled(&confirm.tool_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("  Intent:      "),
            Span::styled(&confirm.description, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::raw("  Parameters:  "),
            Span::styled(params_str, Style::default().fg(Color::LightCyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("      [Y] Approve & Execute      ", Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)),
            Span::raw("    "),
            Span::styled("      [N] Deny Action      ", Style::default().fg(Color::Black).bg(Color::LightRed).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let modal = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(Span::styled(
                    " ⚠ ACTION CONFIRMATION REQUIRED ",
                    Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD),
                ))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::LightYellow))
                .style(Style::default().bg(Color::Rgb(30, 24, 28))),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(modal, modal_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::PendingConfirmation;
    use ratatui::{backend::TestBackend, Terminal};

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn draws_lean_workspace_without_box_borders() {
        let app = App::new();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        // Lean transcript: welcome lines plus row dividers, no boxed chrome.
        assert!(text.contains("Welcome! Type your intent"));
        assert!(text.contains("────"));
        assert!(text.contains("standalone"));
        for heavy in ["◈", "❯ YOU", "◆ NIAT AGENT", "│", "▎", "╭", "╰"] {
            assert!(!text.contains(heavy), "lean frame must not contain {heavy:?}");
        }
    }

    #[test]
    fn lean_transcript_styles_each_sender() {
        use crate::app::Message;
        let mut app = App::new();
        app.messages.clear();
        for (sender, content) in [
            (MessageSender::User, "hello niat"),
            (MessageSender::Agent, "working on it"),
            (MessageSender::Tool("disk.check".into()), "5 GB free"),
            (MessageSender::Error, "boom"),
        ] {
            app.messages.push(Message {
                sender,
                content: content.into(),
                timestamp: "00:00:00".into(),
            });
        }
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("❯ hello niat"));
        assert!(text.contains("working on it"));
        assert!(text.contains("disk.check"));
        assert!(text.contains("✗"));
        assert!(text.contains("boom"));
        assert!(!text.contains("00:00:00"), "lean transcript shows no timestamps");
    }

    #[test]
    fn permission_line_shows_pending_approval() {
        let mut app = App::new();
        app.pending_confirm = Some(PendingConfirmation {
            request_id: "req-1".into(),
            tool_name: "disk.format".into(),
            description: "Format disk".into(),
            params: serde_json::json!({}),
            safety_level: SafetyLevel::Dangerous,
        });
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("approval needed"));
        assert!(text.contains("disk.format"));
    }

    #[test]
    fn editing_mode_positions_cursor_at_end_of_input() {
        let mut app = App::new();
        app.input_mode = InputMode::Editing;
        app.input = "check disk".into();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        // Layout heights 1 + 24 + 1 + 3 + 1: input area starts at y=26,
        // cursor goes on the line below its top rule: x = 0 + 2 + 10.
        terminal.backend_mut().assert_cursor_position(Position::new(12, 27));
    }

    #[test]
    fn normal_mode_hides_cursor() {
        let mut app = App::new();
        app.input_mode = InputMode::Normal;
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        terminal
            .backend_mut()
            .assert_cursor_position(Position::ORIGIN);
    }
}
