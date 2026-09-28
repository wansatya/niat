//! TUI rendering — DOS-styled console UI (CP437-era look).

use crate::app::{App, ConfigField, InputMode, MessageSender, PendingConfirmation};
use niat_common::types::{SafetyLevel, NIAT_VERSION};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

// DOS-style chrome on an eye-friendly near-black desktop (no blue flood).
const APP_BG: Color = Color::Rgb(16, 16, 16);
const PANEL_BG: Color = Color::Rgb(16, 16, 16);
const FIELD_BG: Color = Color::Rgb(0, 0, 0);
const BAR_BG: Color = Color::Rgb(170, 170, 170);
const INK: Color = Color::Rgb(0, 0, 0);
const FG: Color = Color::Rgb(255, 255, 255);
const DIM: Color = Color::Rgb(170, 170, 170);
const FAINT: Color = Color::Rgb(0, 170, 170);
const ACCENT: Color = Color::Rgb(255, 255, 85);
const GREEN: Color = Color::Rgb(85, 255, 85);
const YELLOW: Color = Color::Rgb(255, 255, 85);
const RED: Color = Color::Rgb(255, 85, 85);
const TOOL: Color = Color::Rgb(85, 255, 255);
const DANGER: Color = Color::Rgb(170, 0, 0);
const BORDER: Color = Color::Rgb(170, 170, 170);
const BORDER_FOCUS: Color = Color::Rgb(255, 255, 85);
const RULE: Color = Color::Rgb(170, 85, 0);

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

    // Warm espresso backdrop so the whole frame shares one tone.
    f.render_widget(Block::default().style(Style::default().bg(APP_BG)), f.area());

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
    // DOS menu bar: reverse-video strip with brand chip and status.
    let bar = Style::default().fg(INK).bg(BAR_BG);
    let state = if app.connected {
        Span::styled(" ONLINE ", Style::default().fg(INK).bg(GREEN).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" OFFLINE ", Style::default().fg(FG).bg(DANGER).add_modifier(Modifier::BOLD))
    };
    let left = Line::from(vec![
        Span::styled(" NIAT ", Style::default().fg(ACCENT).bg(APP_BG).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" v{} ", NIAT_VERSION), bar),
        state,
        Span::styled(format!(" {} ", app.config.model.model), bar),
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
    f.render_widget(Paragraph::new(left).style(bar), cols[0]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(right_text, bar))).alignment(Alignment::Right).style(bar),
        cols[1],
    );
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    // Lean transcript: no borders or boxes, turns separated by whitespace.
    let mut lines: Vec<Line> = Vec::new();

    for (i, msg) in app.messages.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }

        let mut content_lines: Vec<&str> = msg.content.lines().collect();
        if content_lines.is_empty() {
            content_lines.push("");
        }

        match &msg.sender {
            MessageSender::User => {
                for (j, content_line) in content_lines.iter().enumerate() {
                    let prefix = if j == 0 { "> " } else { "  " };
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
                        Span::styled("* ", faint()),
                        Span::styled(*content_line, dim()),
                    ]));
                }
            }
            MessageSender::Success => {
                for (j, content_line) in content_lines.iter().enumerate() {
                    let prefix = if j == 0 { "[OK] " } else { "     " };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(GREEN)),
                        Span::styled(*content_line, Style::default().fg(FG)),
                    ]));
                }
            }
            MessageSender::Error => {
                for (j, content_line) in content_lines.iter().enumerate() {
                    let prefix = if j == 0 { "[ERR] " } else { "      " };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(RED)),
                        Span::styled(*content_line, Style::default().fg(FG)),
                    ]));
                }
            }
            MessageSender::Tool(name) => {
                lines.push(Line::from(vec![
                    Span::styled(">> ", Style::default().fg(TOOL)),
                    Span::styled(
                        name.as_str(),
                        Style::default().fg(TOOL).add_modifier(Modifier::BOLD),
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
                " ! APPROVAL NEEDED: ",
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                confirm.tool_name.clone(),
                Style::default().fg(TOOL).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" -- {} ", confirm.description), dim()),
            Span::styled("[Y]Approve", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled("  ", dim()),
            Span::styled("[N]Deny", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
        ])
    } else if !app.connected {
        Line::from(vec![
            Span::styled("* STANDALONE", dim()),
            Span::styled(
                " -- agent offline, F2 shell | :config setup | :help commands",
                faint(),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("* ", faint()),
            Span::styled("READY", dim()),
            Span::styled(" -- type below, Enter sends | F3 config | F2 shell", faint()),
        ])
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) -> Option<Position> {
    // DOS command line under a double rule.
    let (rule_color, placeholder) = match app.input_mode {
        InputMode::Normal => (RULE, "Press Enter to type..."),
        InputMode::Editing => (BORDER_FOCUS, "Type intent and press Enter..."),
        InputMode::ConfigModal => (RULE, "Configuration in progress..."),
    };

    let prompt = Span::styled(
        "> ",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    );
    let content = if app.input.is_empty() {
        Line::from(vec![prompt, Span::styled(placeholder, faint())])
    } else {
        Line::from(vec![
            prompt,
            Span::styled(app.input.as_str(), Style::default().fg(FG)),
        ])
    };

    let input = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(rule_color)),
    );
    f.render_widget(input, area);

    if !matches!(app.input_mode, InputMode::Editing) {
        return None;
    }

    // Cursor at the end of the input, on the line below the top rule.
    // The visible prefix "> " is 2 cells wide.
    let input_width: u16 = app.input.chars().count().min(u16::MAX as usize) as u16;
    let x = (area.x + 2 + input_width).min(area.x + area.width.saturating_sub(1));
    Some(Position::new(x, area.y + 1))
}

fn draw_footer(f: &mut Frame, _app: &App, area: Rect) {
    // DOS status bar: gray strip with `KEY=Action` hints.
    let bar = Style::default().bg(BAR_BG);
    let key = |t: &'static str| {
        Span::styled(
            t,
            Style::default().fg(ACCENT).bg(APP_BG).add_modifier(Modifier::BOLD),
        )
    };
    let item = |t: &'static str| Span::styled(t, Style::default().fg(INK).bg(BAR_BG));
    let shortcuts = Line::from(vec![
        key(" Enter "),
        item("=Send  "),
        key(" F3 "),
        item("=Config  "),
        key(" F5 "),
        item("=Reload  "),
        key(" F2 "),
        item("=Shell  "),
        key(" Ctrl+C "),
        item("=Reboot  "),
        key(" :help "),
        item("=Help"),
    ]);

    let footer = Paragraph::new(shortcuts)
        .alignment(Alignment::Center)
        .style(bar);
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

    // DOS dialog: double yellow border with a reverse-video title.
    let frame_block = Block::default()
        .title(Span::styled(
            " Configuration ",
            Style::default().fg(INK).bg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(PANEL_BG));
    f.render_widget(frame_block, modal_area);

    // Description
    let desc = Paragraph::new(Line::from(vec![
        Span::styled(
            " Configure the OpenAI-compatible endpoint, model, and API key:",
            dim(),
        ),
    ]));
    f.render_widget(desc, chunks[0]);

    // Field 1: Base URL
    let url_active = modal.active_field == ConfigField::BaseUrl;
    let url_block = Block::default()
        .title(if url_active { "[>] API base URL " } else { " API base URL " })
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(if url_active { YELLOW } else { BORDER }))
        .style(Style::default().bg(FIELD_BG));
    let url_text = if modal.base_url.is_empty() && url_active {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("https://api.openai.com/v1", faint()),
            Span::styled("█", Style::default().fg(YELLOW)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(&modal.base_url, Style::default().fg(FG)),
            if url_active { Span::styled("█", Style::default().fg(YELLOW)) } else { Span::raw("") },
        ])
    };
    f.render_widget(Paragraph::new(url_text).block(url_block), chunks[1]);

    // Field 2: Model Name
    let model_active = modal.active_field == ConfigField::Model;
    let model_block = Block::default()
        .title(if model_active { "[>] Model name " } else { " Model name " })
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(if model_active { YELLOW } else { BORDER }))
        .style(Style::default().bg(FIELD_BG));
    let model_text = if modal.model.is_empty() && model_active {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("gpt-4o", faint()),
            Span::styled("█", Style::default().fg(YELLOW)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(&modal.model, Style::default().fg(FG)),
            if model_active { Span::styled("█", Style::default().fg(YELLOW)) } else { Span::raw("") },
        ])
    };
    f.render_widget(Paragraph::new(model_text).block(model_block), chunks[2]);

    // Field 3: API Key
    let key_active = modal.active_field == ConfigField::ApiKey;
    let key_block = Block::default()
        .title(if key_active { "[>] API key " } else { " API key " })
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(if key_active { YELLOW } else { BORDER }))
        .style(Style::default().bg(FIELD_BG));

    let masked_key = if modal.show_api_key {
        modal.api_key.clone()
    } else if modal.api_key.is_empty() {
        String::new()
    } else {
        "•".repeat(modal.api_key.len().min(40))
    };
    let key_fg = if modal.show_api_key || modal.api_key.is_empty() { FG } else { YELLOW };

    let key_text = if modal.api_key.is_empty() && key_active {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("sk-...", faint()),
            Span::styled("█", Style::default().fg(YELLOW)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(masked_key, Style::default().fg(key_fg)),
            if key_active { Span::styled("█", Style::default().fg(YELLOW)) } else { Span::raw("") },
        ])
    };
    f.render_widget(Paragraph::new(key_text).block(key_block), chunks[3]);

    // Buttons
    let save_active = modal.active_field == ConfigField::SaveButton;
    let cancel_active = modal.active_field == ConfigField::CancelButton;

    let buttons_line = Line::from(vec![
        Span::raw("    "),
        Span::styled(
            if save_active { "[ Save (Enter) ]" } else { "[ Save ]" },
            Style::default()
                .fg(if save_active { INK } else { DIM })
                .bg(if save_active { ACCENT } else { FIELD_BG })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("        "),
        Span::styled(
            if cancel_active { "[ Cancel (Esc) ]" } else { "[ Cancel ]" },
            Style::default()
                .fg(if cancel_active { FG } else { DIM })
                .bg(if cancel_active { DANGER } else { FIELD_BG })
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(buttons_line).alignment(Alignment::Center), chunks[4]);

    // Modal Helper Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            "Tab: switch field | Ctrl+T: reveal key | Enter: save | Esc: close",
            dim(),
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
        SafetyLevel::Safe => Style::default().fg(INK).bg(GREEN).add_modifier(Modifier::BOLD),
        SafetyLevel::Confirm => Style::default().fg(INK).bg(YELLOW).add_modifier(Modifier::BOLD),
        SafetyLevel::Dangerous => Style::default().fg(FG).bg(DANGER).add_modifier(Modifier::BOLD),
    };

    let safety_label = match confirm.safety_level {
        SafetyLevel::Safe => " [ SAFE ] ",
        SafetyLevel::Confirm => " [ CONFIRM ] ",
        SafetyLevel::Dangerous => " [ DANGER ] ",
    };

    let params_str = serde_json::to_string(&confirm.params).unwrap_or_default();

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status: ", dim()),
            Span::styled(safety_label, safety_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Tool: ", dim()),
            Span::styled(&confirm.tool_name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Action: ", dim()),
            Span::styled(&confirm.description, Style::default().fg(FG)),
        ]),
        Line::from(vec![
            Span::styled("  Args: ", dim()),
            Span::styled(params_str, dim()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Y] Approve ", Style::default().fg(INK).bg(GREEN).add_modifier(Modifier::BOLD)),
            Span::raw("    "),
            Span::styled(" [N] Deny ", Style::default().fg(FG).bg(DANGER).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let modal = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(Span::styled(
                    " Confirmation ",
                    Style::default().fg(FG).bg(DANGER).add_modifier(Modifier::BOLD),
                ))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(DANGER))
                .style(Style::default().bg(PANEL_BG)),
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
    fn draws_dos_desktop_without_modern_glyphs() {
        let app = App::new();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        // DOS desktop: welcome lines, blue backdrop, gray menu/status bars.
        assert!(text.contains("Welcome! Type your intent"));
        assert!(text.contains("Made in Indonesia"));
        // The only horizontal rule in the frame is the input's double top border.
        assert_eq!(text.matches('═').count(), 100);
        assert!(text.contains("STANDALONE"));
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, APP_BG);
        assert_eq!(terminal.backend().buffer()[(90, 0)].bg, BAR_BG);
        assert_eq!(terminal.backend().buffer()[(0, 10)].bg, APP_BG);
        for hint in ["=Send", "=Shell", "=Reboot", "=Reload", "F5"] {
            assert!(text.contains(hint), "footer must hint at {hint:?}");
        }
        for modern in ["◈", "❯", "✓", "✗", "⚙", "◆", "○", "●", "▶", "⚠", "⛔", "✔", "◀", "╭", "╰", "▎"] {
            assert!(!text.contains(modern), "DOS frame must not contain {modern:?}");
        }
    }

    #[test]
    fn dos_transcript_styles_each_sender() {
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
        assert!(text.contains("> hello niat"));
        assert!(text.contains("working on it"));
        assert!(text.contains("disk.check"));
        assert!(text.contains("[ERR]"));
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
        assert!(text.contains("APPROVAL NEEDED"));
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
