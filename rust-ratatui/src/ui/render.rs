use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::core::{
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

pub fn render_versus(
    frame: &mut Frame,
    left_state: &GameState,
    right_state: &GameState,
    ctx: RenderContext,
) {
    let area = frame.size();
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Length(1), Constraint::Percentage(50)])
        .split(area);
    render_game_at(frame, columns[0], left_state, ctx);
    render_game_at(frame, columns[2], right_state, ctx);
    frame.render_widget(Paragraph::new(""), columns[1]);
}

pub fn render_menu(frame: &mut Frame, title: &str, items: &[String], selected: usize, footer: &str) {
    render_list_screen(frame, title, items, selected, footer, 40);
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

pub fn render_game_over(
    frame: &mut Frame,
    state: &GameState,
    ctx: RenderContext,
    selected: usize,
    title: &str,
) {
    render_game(frame, state, ctx);

    let area = frame.size();
    let width = area.width.min(42).max(24);
    let height = area.height.min(12).max(8);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);

    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let summary_lines = vec![
        Line::from(format!("Score {:06}", state.score)),
        Line::from(format!("Lines {:02}", state.lines)),
        Line::from(format!(
            "{} {:02}:{:02}",
            ctx.time_label,
            ctx.time_secs / 60,
            ctx.time_secs % 60
        )),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    frame.render_widget(Paragraph::new(summary_lines), chunks[0]);

    let items = ["Retry", "Menu", "Quit"];
    render_list(frame, &items, selected, chunks[1]);
}

pub fn render_versus_over(
    frame: &mut Frame,
    left_state: &GameState,
    right_state: &GameState,
    ctx: RenderContext,
    selected: usize,
    title: &str,
) {
    render_versus(frame, left_state, right_state, ctx);

    let area = frame.size();
    let width = area.width.min(48).max(28);
    let height = area.height.min(12).max(8);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);

    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let summary_lines = vec![
        Line::from(format!("P1 Score {:06}", left_state.score)),
        Line::from(format!("P2 Score {:06}", right_state.score)),
        Line::from(format!(
            "{} {:02}:{:02}",
            ctx.time_label,
            ctx.time_secs / 60,
            ctx.time_secs % 60
        )),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    frame.render_widget(Paragraph::new(summary_lines), chunks[0]);

    let items = ["Rematch", "Menu", "Quit"];
    render_list(frame, &items, selected, chunks[1]);
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

pub fn render_pause(
    frame: &mut Frame,
    state: &GameState,
    ctx: RenderContext,
    selected: usize,
) {
    render_game(frame, state, ctx);

    let area = frame.size();
    let width = area.width.min(42).max(24);
    let height = area.height.min(10).max(8);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);

    let block = Block::default().title("PAUSED").borders(Borders::ALL);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0)])
        .split(inner);

    let items = ["Resume", "Menu", "Quit"];
    render_list(frame, &items, selected, chunks[0]);
}

fn render_left(frame: &mut Frame, area: Rect, state: &GameState, ctx: RenderContext) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(0),
            Constraint::Length(6),
        ])
        .split(area);

    let hold_lines = piece_preview_lines(state.hold);
    let hold = Paragraph::new(hold_lines).block(Block::default().title("HOLD").borders(Borders::ALL));
    frame.render_widget(hold, chunks[0]);

    let combo_lines = combo_lines(state);
    let combo = Paragraph::new(combo_lines)
        .block(Block::default().title("COMBO").borders(Borders::ALL));
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
    let score =
        Paragraph::new(score_lines).block(Block::default().title(score_title).borders(Borders::ALL));
    frame.render_widget(score, chunks[2]);
}

fn render_board(frame: &mut Frame, area: Rect, state: &GameState) {
    let block = Block::default().title("BOARD").borders(Borders::ALL);
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
                Span::styled("██", Style::default().fg(piece_color(kind)))
            } else if let Some(kind) = ghost_kind {
                Span::styled(
                    "░░",
                    Style::default()
                        .fg(piece_color(kind))
                        .add_modifier(Modifier::DIM),
                )
            } else {
                Span::styled("· ", Style::default().fg(Color::Gray))
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }

    let board = Paragraph::new(lines);
    frame.render_widget(board, inner);
}

fn render_next(frame: &mut Frame, area: Rect, state: &GameState) {
    let block = Block::default().title("NEXT").borders(Borders::ALL);
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
            rotation: crate::core::piece::Rotation(0),
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
    for y in 0..inner_h {
        let mut spans: Vec<Span> = Vec::new();
        if pad > 0 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
        for x in 0..preview_w {
            if let Some(kind) = grid[y][x] {
                spans.push(Span::styled("██", Style::default().fg(piece_color(kind))));
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
            rotation: crate::core::piece::Rotation(0),
            x: 0,
            y: 0,
        };
        for (x, y) in piece.blocks() {
            if x >= 0 && x < 4 && y >= 0 && y < 4 {
                grid[y as usize][x as usize] = true;
            }
        }
    }

    let mut lines = Vec::with_capacity(4);
    for y in 0..4 {
        let mut spans = Vec::with_capacity(4);
        for x in 0..4 {
            if grid[y][x] {
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
        PieceType::I => Color::Cyan,
        PieceType::O => Color::Yellow,
        PieceType::T => Color::Magenta,
        PieceType::S => Color::Green,
        PieceType::Z => Color::Red,
        PieceType::J => Color::Blue,
        PieceType::L => Color::LightYellow,
    }
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
    let height = (items.len() as u16 + 6).min(area.height.saturating_sub(2)).max(8);
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
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x.saturating_add((area.width.saturating_sub(width)) / 2);
    let y = area.y.saturating_add((area.height.saturating_sub(height)) / 2);
    Rect { x, y, width, height }
}
