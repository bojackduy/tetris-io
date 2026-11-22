use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::adapters::input::{KeyBindings, keycode_label};
use crate::domain::{
    board::{BOARD_WIDTH, VISIBLE_HEIGHT, VISIBLE_START_Y},
    piece::{Piece, PieceType},
    rules::ghost_piece,
    state::GameState,
};

const LEFT_COL_WIDTH: u16 = 18;
const GARBAGE_BAR_WIDTH: u16 = 1;
const BOARD_BOX_WIDTH: u16 = BOARD_WIDTH as u16 * 2 + 2;
const RIGHT_COL_WIDTH: u16 = 14;
const TOTAL_PLAY_WIDTH: u16 =
    LEFT_COL_WIDTH + GARBAGE_BAR_WIDTH + BOARD_BOX_WIDTH + RIGHT_COL_WIDTH;

#[derive(Debug, Clone, Copy)]
pub struct RenderContext {
    pub time_ms: u64,
    pub time_label: &'static str,
    pub enhanced_input: bool,
    pub ghost: bool,
    pub grid_style: u8,
    pub key_presses: u32,
    pub lock_delay_ms: u64,
    pub mode_label: &'static str,
    pub lines_target: Option<u32>,
}

fn format_time_jstris(time_ms: u64) -> String {
    let total_secs = time_ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let centis = (time_ms % 1000) / 10;
    format!("{mins}:{secs:02}.{centis:02}")
}

fn calc_pps(pieces: u32, time_ms: u64) -> f64 {
    if pieces == 0 || time_ms == 0 {
        0.0
    } else {
        f64::from(pieces) / (time_ms as f64 / 1000.0)
    }
}

fn calc_kpp(key_presses: u32, pieces: u32) -> f64 {
    if pieces == 0 {
        0.0
    } else {
        f64::from(key_presses) / f64::from(pieces)
    }
}

pub fn render_game(frame: &mut Frame, state: &GameState, ctx: RenderContext) {
    let area = frame.area();
    render_game_at(frame, area, state, ctx);
}

fn playfield_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let board_height = VISIBLE_HEIGHT as u16 + 2;
    let show_status = area.height > board_height;
    let status_h = u16::from(show_status);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(board_height),
            Constraint::Length(status_h),
            Constraint::Min(0),
        ])
        .split(area);

    let total_w = TOTAL_PLAY_WIDTH.min(area.width);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total_w),
            Constraint::Min(0),
        ])
        .split(rows[1]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(LEFT_COL_WIDTH),
            Constraint::Length(GARBAGE_BAR_WIDTH),
            Constraint::Length(BOARD_BOX_WIDTH),
            Constraint::Length(RIGHT_COL_WIDTH),
        ])
        .split(cols[1]);

    (columns[0], columns[1], columns[2], columns[3], rows[2])
}

fn render_game_at(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let (left, garbage_bar, board_area, right, status_area) = playfield_layout(area);
    render_left(frame, left, state, ctx);
    render_garbage_bar(frame, garbage_bar, state);
    render_board(frame, board_area, state, ctx);
    render_next(frame, right, state);
    if status_area.height > 0 {
        render_status_bar(frame, status_area, ctx);
    }
}

fn render_garbage_bar(frame: &mut Frame, area: Rect, state: &GameState) {
    let pending = state.pending_garbage().min(VISIBLE_HEIGHT as u32) as usize;
    let mut lines = Vec::with_capacity(VISIBLE_HEIGHT + 2);
    lines.push(Line::from(" "));
    for row in 0..VISIBLE_HEIGHT {
        let from_bottom = VISIBLE_HEIGHT - row;
        if from_bottom <= pending {
            lines.push(Line::from(Span::styled(
                "█",
                Style::default().fg(Color::Rgb(215, 15, 55)),
            )));
        } else {
            lines.push(Line::from(" "));
        }
    }
    lines.push(Line::from(" "));
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, ctx: RenderContext) {
    let input_span = if ctx.enhanced_input {
        Span::styled(
            "● ENHANCED DAS/ARR",
            Style::default().fg(Color::Rgb(89, 177, 1)),
        )
    } else {
        Span::styled(
            "▲ LEGACY INPUT (OS repeat — terminal lacks key-release)",
            Style::default()
                .fg(Color::Rgb(227, 159, 2))
                .add_modifier(Modifier::BOLD),
        )
    };
    let line = Line::from(vec![
        Span::styled(
            format!("{}  ", ctx.mode_label),
            Style::default()
                .fg(Color::Rgb(15, 155, 215))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "F4: Restart  P/Esc: Pause   ",
            Style::default().fg(Color::DarkGray),
        ),
        input_span,
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
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
    let area = frame.area();
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
    let area = frame.area();
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
    title: &str,
) {
    let items = ["Retry", "Menu", "Quit"];
    render_game_over_custom(frame, state, ctx, selected, title, &items);
}

pub fn render_game_over_custom(
    frame: &mut Frame,
    state: &GameState,
    ctx: RenderContext,
    selected: usize,
    title: &str,
    items: &[&str],
) {
    render_game_dimmed(frame, state, ctx);

    let area = frame.area();
    let overlay_width = area.width.saturating_sub(4).min(38);
    let overlay_height = area.height.saturating_sub(2).min(15);
    let overlay = centered_rect(area, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay);

    let lower = title.to_lowercase();
    let border_color =
        if lower.contains("win") || lower.contains("40 lines") || lower.contains("cheese") {
            Color::Rgb(89, 177, 1)
        } else if lower.contains("lose") || lower.contains("lost") || lower.contains("game over") {
            Color::Rgb(215, 15, 55)
        } else {
            Color::Rgb(227, 159, 2)
        };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Min(8),    // Stats + actions
            Constraint::Length(1), // Hint
        ])
        .split(inner);

    let title_line = Line::from(Span::styled(
        title.to_uppercase(),
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), title_line]).alignment(Alignment::Center),
        sections[0],
    );

    let content_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62), // Jstris run summary stats
            Constraint::Percentage(38), // Actions
        ])
        .split(sections[1]);

    let label_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);
    let pps = calc_pps(state.pieces_placed, ctx.time_ms);
    let kpp = calc_kpp(ctx.key_presses, state.pieces_placed);
    let stat_row = |label: &'static str, value: String| {
        Line::from(vec![
            Span::styled(format!(" {label:<8}"), label_style),
            Span::raw(value),
        ])
    };
    let stats = vec![
        stat_row(ctx.time_label, format_time_jstris(ctx.time_ms)),
        stat_row("Lines", format!("{}", state.lines)),
        stat_row("Pieces", format!("{}", state.pieces_placed)),
        stat_row("PPS", format!("{pps:.2}")),
        stat_row("KPP", format!("{kpp:.2}")),
        stat_row("Finesse", format!("{}", state.finesse_faults)),
        stat_row("Attack", format!("{}", state.attack)),
        stat_row("Score", format_score_with_commas(state.score)),
    ];
    frame.render_widget(Paragraph::new(stats), content_cols[0]);

    let actions_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(content_cols[1]);
    render_list(frame, items, selected, actions_area[1]);

    let hint = Line::from(Span::styled(
        "↵ select   F4 restart",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(vec![hint]).alignment(Alignment::Center),
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
    render_game_dimmed(frame, state, ctx);

    let area = frame.area();
    let overlay_width = area.width.saturating_sub(4).min(30);
    let overlay_height = area.height.saturating_sub(2).min(17);
    let overlay = centered_rect(area, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(227, 159, 2)));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Length(7), // Jstris stats
            Constraint::Length(1), // Separator
            Constraint::Length(4), // Actions
            Constraint::Min(0),    // Hint
        ])
        .split(inner);

    let title = Line::from(vec![
        Span::styled("═", Style::default().fg(Color::Rgb(227, 159, 2))),
        Span::styled(
            " PAUSED ",
            Style::default()
                .fg(Color::Rgb(227, 159, 2))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("═", Style::default().fg(Color::Rgb(227, 159, 2))),
    ]);
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), title]).alignment(Alignment::Center),
        sections[0],
    );

    let pps = calc_pps(state.pieces_placed, ctx.time_ms);
    let kpp = calc_kpp(ctx.key_presses, state.pieces_placed);
    let stats = vec![
        Line::from(format!(
            " {:<8}{:>14}",
            ctx.time_label,
            format_time_jstris(ctx.time_ms)
        )),
        Line::from(format!(" {:<8}{:>14}", "Lines", state.lines)),
        Line::from(format!(" {:<8}{:>14.2}", "PPS", pps)),
        Line::from(format!(" {:<8}{:>14.2}", "KPP", kpp)),
        Line::from(format!(" {:<8}{:>14}", "Finesse", state.finesse_faults)),
        Line::from(format!(" {:<8}{:>14}", "Attack", state.attack)),
        Line::from(format!(
            " {:<8}{:>14}",
            "Score",
            format_score_with_commas(state.score)
        )),
    ];
    frame.render_widget(Paragraph::new(stats), sections[1]);

    let sep = Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(vec![sep]), sections[2]);

    let items = ["Resume", "Menu", "Quit"];
    render_list(frame, &items, selected, sections[3]);

    let hint = Line::from(Span::styled(
        "P/Esc resume   F4 restart",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(vec![hint]).alignment(Alignment::Center),
        sections[4],
    );
}

fn render_game_dimmed(frame: &mut Frame, state: &GameState, ctx: RenderContext) {
    let area = frame.area();
    render_game_at_dimmed(frame, area, state, ctx);
}

fn render_game_at_dimmed(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let (left, _garbage_bar, board_area, right, _status) = playfield_layout(area);
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
            "{} {}",
            ctx.time_label,
            format_time_jstris(ctx.time_ms)
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
            rotation: crate::domain::piece::Rotation(0),
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
            rotation: crate::domain::piece::Rotation(0),
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
    let val_style = Style::default().fg(Color::White);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Hold box
            Constraint::Length(1), // Spacer
            Constraint::Length(4), // Clear / Combo / B2B callout
            Constraint::Length(1), // Spacer
            Constraint::Min(9),    // Jstris live stats
        ])
        .split(area);

    // Hold box (dimmed when hold is already used this turn, matching Jstris)
    let hold_block = Block::default()
        .title(Span::styled(" HOLD ", dim_header))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));
    let hold_inner = hold_block.inner(chunks[0]);
    frame.render_widget(hold_block, chunks[0]);
    let hold_lines = if state.hold_used {
        piece_preview_lines_compact(state.hold, true)
    } else {
        piece_preview_lines_compact(state.hold, false)
    };
    frame.render_widget(Paragraph::new(hold_lines), hold_inner);

    // Clear / Spin / B2B / Combo callout
    let mut callout_lines = Vec::new();
    if let Some(label) = &state.last_clear_label {
        for part in label.split(" | ").take(4) {
            let color = if part.contains("Perfect") {
                Color::Rgb(227, 159, 2)
            } else if part.contains("T-Spin") {
                Color::Rgb(175, 41, 138)
            } else if part.contains("B2B") {
                Color::Rgb(15, 155, 215)
            } else if part.contains("Combo") {
                Color::Rgb(89, 177, 1)
            } else {
                Color::White
            };
            callout_lines.push(Line::from(Span::styled(
                part.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
        }
    }
    frame.render_widget(Paragraph::new(callout_lines), chunks[2]);

    // Jstris live stats
    let pending_garbage = state.pending_garbage();
    let (lines_label, lines_value) = if state.garbage_remaining > 0 {
        ("Cheese", state.garbage_remaining)
    } else if let Some(target) = ctx.lines_target {
        ("Left", target.saturating_sub(state.lines))
    } else if pending_garbage > 0 {
        ("Incom", pending_garbage)
    } else {
        ("Lines", state.lines)
    };

    let pps = calc_pps(state.pieces_placed, ctx.time_ms);
    let kpp = calc_kpp(ctx.key_presses, state.pieces_placed);

    let stat_row = |label: &'static str, value: String| {
        Line::from(vec![
            Span::styled(format!("{label:<8}"), dim_header),
            Span::styled(value, val_style),
        ])
    };

    let score_lines = vec![
        stat_row(ctx.time_label, format_time_jstris(ctx.time_ms)),
        stat_row(lines_label, format!("{lines_value}")),
        stat_row("Attack", format!("{}", state.attack)),
        stat_row("PPS", format!("{pps:.2}")),
        stat_row("KPP", format!("{kpp:.2}")),
        stat_row("Finesse", format!("{}", state.finesse_faults)),
        stat_row("Pieces", format!("{}", state.pieces_placed)),
        stat_row("Score", format_score_with_commas(state.score)),
    ];
    frame.render_widget(Paragraph::new(score_lines), chunks[4]);
}

fn render_board(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let border_color = if state.pending_garbage() >= 4 {
        Color::Rgb(215, 15, 55)
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::with_capacity(VISIBLE_HEIGHT);
    let ghost_blocks = if ctx.ghost {
        Some(ghost_piece(state).blocks())
    } else {
        None
    };
    let active_blocks = state.active.blocks();

    let lock_urgent = state.grounded
        && ctx.lock_delay_ms > 0
        && state.lock_delay_ms.saturating_mul(10) >= ctx.lock_delay_ms.saturating_mul(7);

    for (row_idx, y) in
        (VISIBLE_START_Y as i32..(VISIBLE_START_Y + VISIBLE_HEIGHT) as i32).enumerate()
    {
        let mut spans: Vec<Span> = Vec::with_capacity(BOARD_WIDTH);
        for x in 0..BOARD_WIDTH as i32 {
            let active_kind = block_at(active_blocks, x, y).map(|_| state.active.kind);
            let ghost_kind = ghost_blocks
                .and_then(|blocks| block_at(blocks, x, y))
                .map(|_| state.active.kind);
            let filled_kind = state.board.get(x, y);

            let span = if let Some(kind) = active_kind {
                let glyph = if lock_urgent { "▓▓" } else { "██" };
                let style = if lock_urgent {
                    Style::default()
                        .fg(piece_color(kind))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(piece_color(kind))
                };
                Span::styled(glyph, style)
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
                empty_grid_span(ctx.grid_style, row_idx)
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }

    let board = Paragraph::new(lines);
    frame.render_widget(board, inner);
}

fn empty_grid_span(grid_style: u8, row_idx: usize) -> Span<'static> {
    let style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);
    match grid_style {
        0 => Span::raw("  "),
        1 => Span::styled("· ", style),
        2 => {
            if row_idx.is_multiple_of(2) {
                Span::styled("· ", style)
            } else {
                Span::raw("  ")
            }
        }
        3 => Span::styled("│ ", style),
        _ => Span::styled("┼─", style),
    }
}

fn piece_preview_lines_compact(kind: Option<PieceType>, dimmed: bool) -> Vec<Line<'static>> {
    let mut grid = [[false; 4]; 3];
    let mut color = Color::White;
    if let Some(kind) = kind {
        color = piece_color(kind);
        let piece = Piece {
            kind,
            rotation: crate::domain::piece::Rotation(0),
            x: 0,
            y: 0,
        };
        for (x, y) in piece.blocks() {
            if (0..4).contains(&x) && (0..3).contains(&y) {
                grid[y as usize][x as usize] = true;
            }
        }
    }

    let style = if dimmed {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(color)
    };
    let glyph = if dimmed { "░░" } else { "██" };
    let mut lines = Vec::with_capacity(3);
    for row in &grid {
        let mut spans = Vec::with_capacity(5);
        spans.push(Span::raw("  "));
        for &cell in row {
            if cell {
                spans.push(Span::styled(glyph, style));
            } else {
                spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn render_next(frame: &mut Frame, area: Rect, state: &GameState) {
    let dim_header = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);

    let block = Block::default()
        .title(Span::styled(" NEXT ", dim_header))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let inner_h = inner.height as usize;
    let inner_w = inner.width as usize;
    let preview_h = 3usize;
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
            rotation: crate::domain::piece::Rotation(0),
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

fn block_at(blocks: [(i32, i32); 4], x: i32, y: i32) -> Option<()> {
    for (bx, by) in blocks {
        if bx == x && by == y {
            return Some(());
        }
    }
    None
}

fn piece_color(kind: PieceType) -> Color {
    // Exact Jstris default palette from shipped game.js GameCore.colors.
    match kind {
        PieceType::I => Color::Rgb(15, 155, 215), // #0F9BD7
        PieceType::O => Color::Rgb(227, 159, 2),  // #E39F02
        PieceType::T => Color::Rgb(175, 41, 138), // #AF298A
        PieceType::S => Color::Rgb(89, 177, 1),   // #59B101
        PieceType::Z => Color::Rgb(215, 15, 55),  // #D70F37
        PieceType::J => Color::Rgb(33, 65, 198),  // #2141C6
        PieceType::L => Color::Rgb(227, 91, 2),   // #E35B02
    }
}

fn garbage_color() -> Color {
    Color::Rgb(153, 153, 153) // #999999
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
    let area = frame.area();
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
