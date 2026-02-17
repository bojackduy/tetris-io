use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::input::{keycode_label, KeyBindings};
use tetris_domain::{
    board::{BOARD_WIDTH, VISIBLE_HEIGHT, VISIBLE_START_Y},
    piece::{Piece, PieceType},
    rules::ghost_piece,
    state::GameState,
};

#[derive(Debug, Clone, Copy)]
pub struct RenderContext {
    pub time_secs: u64,
    pub time_label: &'static str,
    pub enhanced_input: bool,
}

pub fn render_game(frame: &mut Frame, state: &GameState, ctx: RenderContext) {
    let area = frame.size();
    render_game_at(frame, area, state, ctx);
}

fn render_game_at(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let board_height = VISIBLE_HEIGHT as u16 + 2;
    let board_width = BOARD_WIDTH as u16 * 2 + 2;
    let left_width: u16 = 14;
    let right_width: u16 = 12;
    let total_width = left_width + board_width + right_width;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(board_height),
            Constraint::Min(0),
        ])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total_width),
            Constraint::Min(0),
        ])
        .split(rows[1]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(board_width),
            Constraint::Length(right_width),
        ])
        .split(cols[1]);

    let left = columns[0];
    let board_area = columns[1];
    let right = columns[2];

    render_left(frame, left, state, ctx);
    render_board(frame, board_area, state);
    render_next(frame, right, state);
}

/// Menu item indices for the spatial layout
const MENU_SOLO_START: usize = 0;
const MENU_SOLO_END: usize = 3; // inclusive: New Game, Blitz, 40 Lines, Cheese
const MENU_MULTI_START: usize = 4;
const MENU_MULTI_END: usize = 5; // inclusive: Host, Join
const MENU_META_START: usize = 6;
const MENU_META_END: usize = 8; // inclusive: High Scores, Settings, Quit

pub fn render_menu(
    frame: &mut Frame,
    _title: &str,
    items: &[String],
    selected: usize,
    footer: &str,
    _bindings: KeyBindings,
) {
    let area = frame.size();
    frame.render_widget(Clear, area);

    // Calculate layout dimensions
    let content_width = 58u16.min(area.width.saturating_sub(4));
    let content_height = 26u16.min(area.height.saturating_sub(2));

    // Center the content
    let h_pad = (area.width.saturating_sub(content_width)) / 2;
    let v_pad = (area.height.saturating_sub(content_height)) / 2;

    let content_area = Rect {
        x: area.x + h_pad,
        y: area.y + v_pad,
        width: content_width,
        height: content_height,
    };

    // Split vertically: title area, cards row, meta bar, footer
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Title + rules
            Constraint::Length(1),  // Spacing
            Constraint::Length(10), // SOLO + MULTI cards
            Constraint::Length(1),  // Spacing
            Constraint::Length(5),  // Meta bar (High Scores, Settings, Quit)
            Constraint::Length(1),  // Spacing
            Constraint::Min(0),     // Footer
        ])
        .split(content_area);

    let title_area = sections[0];
    let cards_area = sections[2];
    let meta_area = sections[4];
    let footer_area = sections[6];

    // === TITLE SECTION ===
    let title_lines = ascii_title_lines();
    let title_block = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top rule
            Constraint::Length(3), // ASCII art
            Constraint::Length(1), // Bottom rule
        ])
        .split(title_area);

    frame.render_widget(
        Paragraph::new(horizontal_rule(content_width))
            .alignment(ratatui::layout::Alignment::Center),
        title_block[0],
    );
    frame.render_widget(
        Paragraph::new(title_lines).alignment(ratatui::layout::Alignment::Center),
        title_block[1],
    );
    frame.render_widget(
        Paragraph::new(horizontal_rule(content_width))
            .alignment(ratatui::layout::Alignment::Center),
        title_block[2],
    );

    // === CARDS ROW: SOLO (left) + MULTI (right) ===
    let cards_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // SOLO card
            Constraint::Length(2),      // Gap
            Constraint::Percentage(40), // MULTI card
        ])
        .split(cards_area);

    let solo_area = cards_cols[0];
    let multi_area = cards_cols[2];

    // SOLO card
    let solo_block = Block::default()
        .title(Span::styled(
            " SOLO ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let solo_inner = solo_block.inner(solo_area);
    frame.render_widget(solo_block, solo_area);

    // Render SOLO items (indices 0-3)
    let solo_items: Vec<Line> = items
        .iter()
        .enumerate()
        .take(MENU_SOLO_END + 1)
        .skip(MENU_SOLO_START)
        .map(|(idx, item)| {
            let is_selected = idx == selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if is_selected { "▸ " } else { "  " };

            // Parse item to separate label from value (for Blitz/40Lines/Cheese)
            let (label, value) = parse_menu_item(item);
            if let Some(val) = value {
                // Adjustable item: show arrows hint
                let arrow_hint = if is_selected {
                    format!("◂{}▸", val)
                } else {
                    val
                };
                let label_width =
                    solo_inner.width.saturating_sub(arrow_hint.len() as u16 + 4) as usize;
                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(format!("{:<width$}", label, width = label_width), style),
                    Span::styled(arrow_hint, style),
                ])
            } else {
                Line::from(Span::styled(format!("{}{}", prefix, label), style))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(solo_items), solo_inner);

    // MULTI card
    let multi_block = Block::default()
        .title(Span::styled(
            " MULTI ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let multi_inner = multi_block.inner(multi_area);
    frame.render_widget(multi_block, multi_area);

    // Render MULTI items (indices 4-5)
    let multi_items: Vec<Line> = items
        .iter()
        .enumerate()
        .take(MENU_MULTI_END + 1)
        .skip(MENU_MULTI_START)
        .map(|(idx, item)| {
            let is_selected = idx == selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if is_selected { "▸ " } else { "  " };
            // Shorten labels for the narrower card
            let label = item.replace("Multiplayer", "");
            Line::from(Span::styled(format!("{}{}", prefix, label.trim()), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(multi_items), multi_inner);

    // === META BAR (horizontal) ===
    let meta_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let meta_inner = meta_block.inner(meta_area);
    frame.render_widget(meta_block, meta_area);

    // Render meta items horizontally (indices 6-8)
    let meta_spans: Vec<Span> = items
        .iter()
        .enumerate()
        .take(MENU_META_END + 1)
        .skip(MENU_META_START)
        .enumerate()
        .flat_map(|(i, (idx, item))| {
            let is_selected = idx == selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let prefix = if is_selected { "▸" } else { " " };
            let mut spans = vec![Span::styled(format!("{} {}", prefix, item), style)];
            if i < 2 {
                spans.push(Span::styled("    ", Style::default()));
            }
            spans
        })
        .collect();

    let meta_line = Line::from(meta_spans);
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), meta_line, Line::from("")])
            .alignment(ratatui::layout::Alignment::Center),
        meta_inner,
    );

    // === FOOTER ===
    let footer_text = if footer.is_empty() {
        "↑↓ navigate   ←→ adjust   ⏎ select   q quit"
    } else {
        footer
    };
    let footer_lines: Vec<Line> = footer_text
        .lines()
        .map(|l| Line::from(Span::styled(l, Style::default().fg(Color::DarkGray))))
        .collect();
    frame.render_widget(
        Paragraph::new(footer_lines).alignment(ratatui::layout::Alignment::Center),
        footer_area,
    );
}

/// Parse a menu item string to extract label and optional value
/// e.g., "Blitz                      2:00" -> ("Blitz", Some("2:00"))
fn parse_menu_item(item: &str) -> (String, Option<String>) {
    let trimmed = item.trim();
    // Check if it contains multiple whitespace-separated parts
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() >= 2 {
        // Check if the last part looks like a value (number or time)
        let last = parts.last().unwrap();
        if last.chars().all(|c| c.is_ascii_digit() || c == ':') {
            let label = parts[..parts.len() - 1].join(" ");
            return (label, Some(last.to_string()));
        }
    }
    (trimmed.to_string(), None)
}

#[allow(dead_code)]
fn movement_help_rows(bindings: KeyBindings) -> Vec<(String, String)> {
    vec![
        ("Move Left".to_string(), keycode_label(bindings.move_left)),
        ("Move Right".to_string(), keycode_label(bindings.move_right)),
        ("Soft Drop".to_string(), keycode_label(bindings.soft_drop)),
        ("Hard Drop".to_string(), keycode_label(bindings.hard_drop)),
        (
            "Rotate Left".to_string(),
            keycode_label(bindings.rotate_left),
        ),
        (
            "Rotate Right".to_string(),
            keycode_label(bindings.rotate_right),
        ),
        ("Rotate 180".to_string(), keycode_label(bindings.rotate_180)),
        ("Hold".to_string(), keycode_label(bindings.hold)),
    ]
}

pub fn render_settings(
    frame: &mut Frame,
    title: &str,
    items: &[String],
    selected: usize,
    footer: &str,
) {
    render_list_screen(frame, title, items, selected, footer, 56);
}

pub fn render_info_screen(frame: &mut Frame, title: &str, lines: &[String], footer: &str) {
    let area = frame.size();
    let width = area.width.clamp(24, 60);
    let height = (lines.len() as u16 + 6)
        .min(area.height.saturating_sub(2))
        .max(8);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);

    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let content_lines: Vec<Line> = lines.iter().map(|l| Line::from(l.clone())).collect();
    let footer_lines: Vec<Line> = if footer.is_empty() {
        Vec::new()
    } else {
        footer.lines().map(Line::from).collect()
    };
    let footer_height = footer_lines.len() as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer_height)])
        .split(inner);

    frame.render_widget(Paragraph::new(content_lines), chunks[0]);
    if footer_height > 0 {
        frame.render_widget(Paragraph::new(footer_lines), chunks[1]);
    }
}

pub fn render_game_over(
    frame: &mut Frame,
    state: &GameState,
    ctx: RenderContext,
    selected: usize,
    _title: &str,
) {
    // Render dimmed game in background
    render_game_dimmed(frame, state, ctx);

    let area = frame.size();

    // Calculate board position to overlay inside it
    let board_height = VISIBLE_HEIGHT as u16 + 2;
    let board_width = BOARD_WIDTH as u16 * 2 + 2;
    let left_width: u16 = 14;
    let right_width: u16 = 12;
    let total_width = left_width + board_width + right_width;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(board_height),
            Constraint::Min(0),
        ])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total_width),
            Constraint::Min(0),
        ])
        .split(rows[1]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(board_width),
            Constraint::Length(right_width),
        ])
        .split(cols[1]);

    let board_area = columns[1];

    // Create overlay inside the board area
    let overlay_width = board_area.width.saturating_sub(2).min(20);
    let overlay_height = board_area.height.saturating_sub(2).min(14);
    let overlay = centered_rect(board_area, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);

    // Split inner into: title, content (2-col), hint
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Min(0),    // Content (stats + actions)
            Constraint::Length(1), // Hint
        ])
        .split(inner);

    // Title - spaced "G A M E  O V E R"
    let title = Line::from(Span::styled(
        "GAME OVER",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), title]).alignment(ratatui::layout::Alignment::Center),
        sections[0],
    );

    // Content area - 2 columns: stats left, actions right
    let content_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55), // Stats
            Constraint::Percentage(45), // Actions
        ])
        .split(sections[1]);

    // Stats column (left)
    let stats = vec![
        Line::from(""),
        Line::from(Span::styled(
            "SCORE",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format_score_with_commas(state.score)),
        Line::from(""),
        Line::from(Span::styled(
            "LEVEL",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format!("{}", state.level)),
        Line::from(""),
        Line::from(Span::styled(
            "LINES",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format!("{}", state.lines)),
        Line::from(""),
        Line::from(Span::styled(
            ctx.time_label,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format!(
            "{:02}:{:02}",
            ctx.time_secs / 60,
            ctx.time_secs % 60
        )),
    ];
    frame.render_widget(Paragraph::new(stats), content_cols[0]);

    // Actions column (right)
    let actions_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(content_cols[1]);

    let items = ["Retry", "Menu", "Quit"];
    render_list(frame, &items, selected, actions_area[1]);

    // Hint
    let hint = Line::from(Span::styled(
        "↵ select",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(vec![hint]).alignment(ratatui::layout::Alignment::Center),
        sections[2],
    );
}

pub fn render_game_over_custom(
    frame: &mut Frame,
    state: &GameState,
    ctx: RenderContext,
    selected: usize,
    title: &str,
    items: &[&str],
) {
    // Render dimmed game in background
    render_game_dimmed(frame, state, ctx);

    let area = frame.size();

    // Calculate board position to overlay inside it
    let board_height = VISIBLE_HEIGHT as u16 + 2;
    let board_width = BOARD_WIDTH as u16 * 2 + 2;
    let left_width: u16 = 14;
    let right_width: u16 = 12;
    let total_width = left_width + board_width + right_width;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(board_height),
            Constraint::Min(0),
        ])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total_width),
            Constraint::Min(0),
        ])
        .split(rows[1]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(board_width),
            Constraint::Length(right_width),
        ])
        .split(cols[1]);

    let board_area = columns[1];

    // Create overlay inside the board area
    let overlay_width = board_area.width.saturating_sub(2).min(20);
    let overlay_height = board_area.height.saturating_sub(2).min(14);
    let overlay = centered_rect(board_area, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay);

    // Determine border color based on title (win=green, lose=red, draw=yellow)
    let border_color = if title.to_lowercase().contains("win") {
        Color::Green
    } else if title.to_lowercase().contains("lose") || title.to_lowercase().contains("lost") {
        Color::Red
    } else {
        Color::Yellow
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);

    // Split inner into: title, content (2-col), hint
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Min(0),    // Content (stats + actions)
            Constraint::Length(1), // Hint
        ])
        .split(inner);

    // Title
    let title_line = Line::from(Span::styled(
        title.to_uppercase(),
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), title_line])
            .alignment(ratatui::layout::Alignment::Center),
        sections[0],
    );

    // Content area - 2 columns: stats left, actions right
    let content_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55), // Stats
            Constraint::Percentage(45), // Actions
        ])
        .split(sections[1]);

    // Stats column (left)
    let stats = vec![
        Line::from(""),
        Line::from(Span::styled(
            "SCORE",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format_score_with_commas(state.score)),
        Line::from(""),
        Line::from(Span::styled(
            "LEVEL",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format!("{}", state.level)),
        Line::from(""),
        Line::from(Span::styled(
            "LINES",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format!("{}", state.lines)),
        Line::from(""),
        Line::from(Span::styled(
            ctx.time_label,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(format!(
            "{:02}:{:02}",
            ctx.time_secs / 60,
            ctx.time_secs % 60
        )),
    ];
    frame.render_widget(Paragraph::new(stats), content_cols[0]);

    // Actions column (right)
    let actions_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(content_cols[1]);

    render_list(frame, items, selected, actions_area[1]);

    // Hint
    let hint = Line::from(Span::styled(
        "↵ select",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(vec![hint]).alignment(ratatui::layout::Alignment::Center),
        sections[2],
    );
}

fn combo_lines(state: &GameState) -> Vec<Line<'static>> {
    let label = match &state.last_clear_label {
        Some(label) if !label.is_empty() => label,
        _ => return vec![Line::from("(empty)")],
    };
    label
        .split(" | ")
        .map(|part| Line::from(part.to_string()))
        .collect()
}

pub fn render_pause(frame: &mut Frame, state: &GameState, ctx: RenderContext, selected: usize) {
    // Render the game with dimmed colors
    render_game_dimmed(frame, state, ctx);

    let area = frame.size();

    // Calculate board position to overlay inside it
    let board_height = VISIBLE_HEIGHT as u16 + 2;
    let board_width = BOARD_WIDTH as u16 * 2 + 2;
    let left_width: u16 = 14;
    let right_width: u16 = 12;
    let total_width = left_width + board_width + right_width;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(board_height),
            Constraint::Min(0),
        ])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total_width),
            Constraint::Min(0),
        ])
        .split(rows[1]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(board_width),
            Constraint::Length(right_width),
        ])
        .split(cols[1]);

    let board_area = columns[1];

    // Create overlay inside the board area
    let overlay_width = board_area.width.saturating_sub(4).min(18);
    let overlay_height = board_area.height.saturating_sub(4).min(16);
    let overlay = centered_rect(board_area, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);

    // Split inner into: title, stats, separator, actions, hint
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Length(4), // Stats
            Constraint::Length(1), // Separator
            Constraint::Length(4), // Actions
            Constraint::Min(0),    // Hint
        ])
        .split(inner);

    // Title
    let title = Line::from(vec![
        Span::styled("═", Style::default().fg(Color::Yellow)),
        Span::styled(
            " PAUSED ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("═", Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), title]).alignment(ratatui::layout::Alignment::Center),
        sections[0],
    );

    // Stats
    let stats = vec![
        Line::from(format!(
            " Score  {:>6}",
            format_score_with_commas(state.score)
        )),
        Line::from(format!(" Level  {:>6}", state.level)),
        Line::from(format!(" Lines  {:>6}", state.lines)),
        Line::from(format!(
            " {}  {:>4}:{:02}",
            ctx.time_label,
            ctx.time_secs / 60,
            ctx.time_secs % 60
        )),
    ];
    frame.render_widget(Paragraph::new(stats), sections[1]);

    // Separator
    let sep = Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(vec![sep]), sections[2]);

    // Actions
    let items = ["Resume", "Menu", "Quit"];
    render_list(frame, &items, selected, sections[3]);

    // Hint
    let hint = Line::from(Span::styled(
        "P resume",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(vec![hint]).alignment(ratatui::layout::Alignment::Center),
        sections[4],
    );
}

fn render_game_dimmed(frame: &mut Frame, state: &GameState, ctx: RenderContext) {
    let area = frame.size();
    render_game_at_dimmed(frame, area, state, ctx);
}

fn render_game_at_dimmed(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let board_height = VISIBLE_HEIGHT as u16 + 2;
    let board_width = BOARD_WIDTH as u16 * 2 + 2;
    let left_width: u16 = 14;
    let right_width: u16 = 12;
    let total_width = left_width + board_width + right_width;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(board_height),
            Constraint::Min(0),
        ])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total_width),
            Constraint::Min(0),
        ])
        .split(rows[1]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(board_width),
            Constraint::Length(right_width),
        ])
        .split(cols[1]);

    let left = columns[0];
    let board_area = columns[1];
    let right = columns[2];

    render_left_dimmed(frame, left, state, ctx);
    render_board_dimmed(frame, board_area, state);
    render_next_dimmed(frame, right, state);
}

fn render_left_dimmed(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let dim_style = Style::default().add_modifier(Modifier::DIM);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(0),
            Constraint::Length(6),
        ])
        .split(area);

    let hold_lines = piece_preview_lines_dimmed(state.hold);
    let hold = Paragraph::new(hold_lines).block(
        Block::default()
            .title(Span::styled("HOLD", dim_style))
            .borders(Borders::ALL)
            .border_style(dim_style),
    );
    frame.render_widget(hold, chunks[0]);

    let combo_lines = combo_lines(state);
    let combo = Paragraph::new(combo_lines)
        .block(
            Block::default()
                .title(Span::styled("COMBO", dim_style))
                .borders(Borders::ALL)
                .border_style(dim_style),
        )
        .style(dim_style);
    frame.render_widget(combo, chunks[1]);

    let pending_garbage = state.pending_garbage();
    let (lines_label, lines_value) = if state.garbage_remaining > 0 {
        ("Garbage", state.garbage_remaining)
    } else if pending_garbage > 0 {
        ("Incoming", pending_garbage)
    } else {
        ("Lines", state.lines)
    };
    let score_lines = vec![
        Line::from(format!("Score {:06}", state.score)),
        Line::from(format!("Level {:02}", state.level)),
        Line::from(format!("{} {:02}", lines_label, lines_value)),
        Line::from(format!(
            "{} {:02}:{:02}",
            ctx.time_label,
            ctx.time_secs / 60,
            ctx.time_secs % 60
        )),
    ];
    let input_label = if ctx.enhanced_input { "ENH" } else { "BASIC" };
    let score_title = format!("Score [{}]", input_label);
    let score = Paragraph::new(score_lines)
        .block(
            Block::default()
                .title(Span::styled(score_title, dim_style))
                .borders(Borders::ALL)
                .border_style(dim_style),
        )
        .style(dim_style);
    frame.render_widget(score, chunks[2]);
}

fn render_board_dimmed(frame: &mut Frame, area: Rect, state: &GameState) {
    let block = Block::default()
        .title(Span::styled(
            "BOARD",
            Style::default().add_modifier(Modifier::DIM),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().add_modifier(Modifier::DIM));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::with_capacity(VISIBLE_HEIGHT);
    let ghost = ghost_piece(state);
    let ghost_blocks = ghost.blocks();
    let active_blocks = state.active.blocks();

    for y in VISIBLE_START_Y as i32..(VISIBLE_START_Y + VISIBLE_HEIGHT) as i32 {
        let mut spans: Vec<Span> = Vec::with_capacity(BOARD_WIDTH * 2);
        for x in 0..BOARD_WIDTH as i32 {
            let active_kind = block_at(active_blocks, x, y).map(|_| state.active.kind);
            let ghost_kind = block_at(ghost_blocks, x, y).map(|_| state.active.kind);
            let filled_kind = state.board.get(x, y);

            let span = if let Some(kind) = active_kind {
                Span::styled(
                    "░░",
                    Style::default()
                        .fg(piece_color(kind))
                        .add_modifier(Modifier::DIM),
                )
            } else if let Some(kind) = filled_kind {
                let color = if state.board.is_garbage_cell(x, y) {
                    garbage_color()
                } else {
                    piece_color(kind)
                };
                Span::styled("░░", Style::default().fg(color).add_modifier(Modifier::DIM))
            } else if let Some(kind) = ghost_kind {
                Span::styled(
                    "░░",
                    Style::default()
                        .fg(piece_color(kind))
                        .add_modifier(Modifier::DIM),
                )
            } else {
                Span::styled("· ", Style::default().fg(Color::DarkGray))
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }

    let board = Paragraph::new(lines);
    frame.render_widget(board, inner);
}

fn render_next_dimmed(frame: &mut Frame, area: Rect, state: &GameState) {
    let dim_style = Style::default().add_modifier(Modifier::DIM);
    let block = Block::default()
        .title(Span::styled("NEXT", dim_style))
        .borders(Borders::ALL)
        .border_style(dim_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let inner_h = inner.height as usize;
    let inner_w = inner.width as usize;
    let preview_h = 4usize;
    let preview_w = 4usize;
    let next = state.peek_next(5);

    let total_preview_h = preview_h * 5;
    let gap = if inner_h > total_preview_h {
        (inner_h - total_preview_h) / 6
    } else {
        0
    };
    let top_pad = gap;

    let mut grid: Vec<Vec<Option<PieceType>>> = vec![vec![None; preview_w]; inner_h];
    for (i, kind) in next.iter().enumerate() {
        let base_y = top_pad + i * (preview_h + gap);
        if base_y + preview_h > inner_h {
            break;
        }
        let piece = Piece {
            kind: *kind,
            rotation: tetris_domain::piece::Rotation(0),
            x: 0,
            y: 0,
        };
        for (x, y) in piece.blocks() {
            if x >= 0 && x < preview_w as i32 && y >= 0 && y < preview_h as i32 {
                let gy = base_y + y as usize;
                let gx = x as usize;
                grid[gy][gx] = Some(*kind);
            }
        }
    }

    let inner_w_chars = inner_w as i32;
    let preview_w_chars = (preview_w * 2) as i32;
    let pad = ((inner_w_chars - preview_w_chars).max(0) / 2) as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(inner_h);
    for row in grid.iter().take(inner_h) {
        let mut spans: Vec<Span> = Vec::new();
        if pad > 0 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
        for cell in row.iter().take(preview_w) {
            if let Some(kind) = cell {
                spans.push(Span::styled(
                    "░░",
                    Style::default()
                        .fg(piece_color(*kind))
                        .add_modifier(Modifier::DIM),
                ));
            } else {
                spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(spans));
    }

    let widget = Paragraph::new(lines);
    frame.render_widget(widget, inner);
}

fn piece_preview_lines_dimmed(kind: Option<PieceType>) -> Vec<Line<'static>> {
    let mut grid = [[false; 4]; 4];
    let mut color = Color::White;
    if let Some(kind) = kind {
        color = piece_color(kind);
        let piece = Piece {
            kind,
            rotation: tetris_domain::piece::Rotation(0),
            x: 0,
            y: 0,
        };
        for (x, y) in piece.blocks() {
            if (0..4).contains(&x) && (0..4).contains(&y) {
                grid[y as usize][x as usize] = true;
            }
        }
    }

    let mut lines = Vec::with_capacity(4);
    for row in &grid {
        let mut spans = Vec::with_capacity(4);
        for &cell in row {
            if cell {
                spans.push(Span::styled(
                    "░░",
                    Style::default().fg(color).add_modifier(Modifier::DIM),
                ));
            } else {
                spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn render_left(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let dim_header = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Hold (reduced - no border)
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Combo
            Constraint::Length(1), // Spacer
            Constraint::Length(5), // Score panel (reduced - no border)
        ])
        .split(area);

    // Hold - borderless
    let hold_header = Line::from(Span::styled("HOLD", dim_header));
    let mut hold_lines = vec![hold_header];
    hold_lines.extend(piece_preview_lines(state.hold));
    frame.render_widget(Paragraph::new(hold_lines), chunks[0]);

    // Combo - borderless
    let combo_header = Line::from(Span::styled("COMBO", dim_header));
    let combo_content = combo_lines(state);
    let mut combo_lines_full = vec![combo_header];
    combo_lines_full.extend(combo_content);
    frame.render_widget(Paragraph::new(combo_lines_full), chunks[2]);

    // Score panel - borderless with formatted values
    let pending_garbage = state.pending_garbage();
    let (lines_label, lines_value) = if state.garbage_remaining > 0 {
        ("Garbage", state.garbage_remaining)
    } else if pending_garbage > 0 {
        ("Incoming", pending_garbage)
    } else {
        ("Lines", state.lines)
    };

    let score_lines = vec![
        Line::from(vec![
            Span::styled("Score ", dim_header),
            Span::raw(format_score_with_commas(state.score)),
        ]),
        Line::from(vec![
            Span::styled("Level ", dim_header),
            Span::raw(format!("{}", state.level)),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", lines_label), dim_header),
            Span::raw(format!("{}", lines_value)),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", ctx.time_label), dim_header),
            Span::raw(format!(
                "{:02}:{:02}",
                ctx.time_secs / 60,
                ctx.time_secs % 60
            )),
        ]),
    ];
    frame.render_widget(Paragraph::new(score_lines), chunks[4]);
}

fn render_board(frame: &mut Frame, area: Rect, state: &GameState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::with_capacity(VISIBLE_HEIGHT);
    let ghost = ghost_piece(state);
    let ghost_blocks = ghost.blocks();
    let active_blocks = state.active.blocks();

    for y in VISIBLE_START_Y as i32..(VISIBLE_START_Y + VISIBLE_HEIGHT) as i32 {
        let mut spans: Vec<Span> = Vec::with_capacity(BOARD_WIDTH * 2);
        for x in 0..BOARD_WIDTH as i32 {
            let active_kind = block_at(active_blocks, x, y).map(|_| state.active.kind);
            let ghost_kind = block_at(ghost_blocks, x, y).map(|_| state.active.kind);
            let filled_kind = state.board.get(x, y);

            let span = if let Some(kind) = active_kind {
                Span::styled("██", Style::default().fg(piece_color(kind)))
            } else if let Some(kind) = filled_kind {
                let color = if state.board.is_garbage_cell(x, y) {
                    garbage_color()
                } else {
                    piece_color(kind)
                };
                Span::styled("██", Style::default().fg(color))
            } else if let Some(kind) = ghost_kind {
                Span::styled(
                    "░░",
                    Style::default()
                        .fg(piece_color(kind))
                        .add_modifier(Modifier::DIM),
                )
            } else {
                Span::styled("· ", Style::default().fg(Color::DarkGray))
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }

    let board = Paragraph::new(lines);
    frame.render_widget(board, inner);
}

fn render_next(frame: &mut Frame, area: Rect, state: &GameState) {
    let dim_header = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);

    // Reserve first line for header
    let header_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let inner = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(1),
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("NEXT", dim_header))),
        header_area,
    );

    let inner_h = inner.height as usize;
    let inner_w = inner.width as usize;
    let preview_h = 3usize; // Compact preview
    let preview_w = 4usize;
    let next = state.peek_next(5);

    let total_preview_h = preview_h * 5;
    let gap = if inner_h > total_preview_h {
        (inner_h - total_preview_h) / 6
    } else {
        0
    };
    let top_pad = gap;

    let mut grid: Vec<Vec<Option<PieceType>>> = vec![vec![None; preview_w]; inner_h];
    for (i, kind) in next.iter().enumerate() {
        let base_y = top_pad + i * (preview_h + gap);
        if base_y + preview_h > inner_h {
            break;
        }
        let piece = Piece {
            kind: *kind,
            rotation: tetris_domain::piece::Rotation(0),
            x: 0,
            y: 0,
        };
        for (x, y) in piece.blocks() {
            if x >= 0 && x < preview_w as i32 && y >= 0 && y < preview_h as i32 {
                let gy = base_y + y as usize;
                let gx = x as usize;
                if gy < inner_h {
                    grid[gy][gx] = Some(*kind);
                }
            }
        }
    }

    let inner_w_chars = inner_w as i32;
    let preview_w_chars = (preview_w * 2) as i32;
    let pad = ((inner_w_chars - preview_w_chars).max(0) / 2) as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(inner_h);
    for row in grid.iter().take(inner_h) {
        let mut spans: Vec<Span> = Vec::new();
        if pad > 0 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
        for cell in row.iter().take(preview_w) {
            if let Some(kind) = cell {
                spans.push(Span::styled("██", Style::default().fg(piece_color(*kind))));
            } else {
                spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(spans));
    }

    let widget = Paragraph::new(lines);
    frame.render_widget(widget, inner);
}

fn piece_preview_lines(kind: Option<PieceType>) -> Vec<Line<'static>> {
    let mut grid = [[false; 4]; 4];
    let mut color = Color::White;
    if let Some(kind) = kind {
        color = piece_color(kind);
        let piece = Piece {
            kind,
            rotation: tetris_domain::piece::Rotation(0),
            x: 0,
            y: 0,
        };
        for (x, y) in piece.blocks() {
            if (0..4).contains(&x) && (0..4).contains(&y) {
                grid[y as usize][x as usize] = true;
            }
        }
    }

    let mut lines = Vec::with_capacity(4);
    for row in &grid {
        let mut spans = Vec::with_capacity(4);
        for &cell in row {
            if cell {
                spans.push(Span::styled("██", Style::default().fg(color)));
            } else {
                spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn block_at(blocks: [(i32, i32); 4], x: i32, y: i32) -> Option<()> {
    for (bx, by) in blocks {
        if bx == x && by == y {
            return Some(());
        }
    }
    None
}

fn piece_color(kind: PieceType) -> Color {
    match kind {
        PieceType::I => Color::Rgb(68, 175, 223),
        PieceType::O => Color::Rgb(244, 208, 62),
        PieceType::T => Color::Rgb(175, 59, 159),
        PieceType::S => Color::Rgb(82, 181, 79),
        PieceType::Z => Color::Rgb(187, 64, 80),
        PieceType::J => Color::Rgb(24, 101, 181),
        PieceType::L => Color::Rgb(244, 135, 44),
    }
}

fn garbage_color() -> Color {
    Color::DarkGray
}

fn ascii_title_lines() -> Vec<Line<'static>> {
    let style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    vec![
        Line::from(Span::styled("▀▀█▀▀ █▀▀ ▀▀█▀▀ █▀▀▄ █ █▀▀", style)),
        Line::from(Span::styled("  █   █▀▀   █   █▄▄▀ █ ▀▀█", style)),
        Line::from(Span::styled("  █   ▀▀▀   █   █  █ █ ▀▀▀", style)),
    ]
}

fn format_score_with_commas(score: u32) -> String {
    let s = score.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

fn horizontal_rule(width: u16) -> Line<'static> {
    let rule = "─".repeat(width as usize);
    Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray)))
}

fn render_list_screen(
    frame: &mut Frame,
    title: &str,
    items: &[String],
    selected: usize,
    footer: &str,
    width_hint: u16,
) {
    let area = frame.size();
    let width = width_hint.min(area.width.saturating_sub(2)).max(20);
    let height = (items.len() as u16 + 6)
        .min(area.height.saturating_sub(2))
        .max(8);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);

    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let footer_lines: Vec<Line> = if footer.is_empty() {
        Vec::new()
    } else {
        footer.lines().map(Line::from).collect()
    };
    let footer_height = footer_lines.len() as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer_height)])
        .split(inner);

    render_list(frame, items, selected, chunks[0]);

    if footer_height > 0 {
        frame.render_widget(Paragraph::new(footer_lines), chunks[1]);
    }
}

fn render_list<T: ToString>(frame: &mut Frame, items: &[T], selected: usize, area: Rect) {
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(selected.min(items.len() - 1)));
    }
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| ListItem::new(item.to_string()))
        .collect();
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area
        .x
        .saturating_add((area.width.saturating_sub(width)) / 2);
    let y = area
        .y
        .saturating_add((area.height.saturating_sub(height)) / 2);
    Rect {
        x,
        y,
        width,
        height,
    }
}
