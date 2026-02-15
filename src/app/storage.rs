use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::app::config::AppConfig;
use crate::ui::input::{KeyBindings, keycode_from_storage, keycode_to_storage};

#[derive(Debug, Clone)]
pub struct HighScore {
    pub name: String,
    pub score: u32,
    pub lines: u32,
    pub time_secs: u64,
    pub mode: String,
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open_default() -> io::Result<Self> {
        let path = default_db_path();
        Self::open(path)
    }

    pub fn open(path: PathBuf) -> io::Result<Self> {
        let conn = Connection::open(path).map_err(to_io_err)?;
        let storage = Self { conn };
        storage.init()?;
        Ok(storage)
    }

    pub fn load_config(&self) -> io::Result<Option<AppConfig>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT gravity_ms, frame_ms, das_ms, arr_ms, dcd_ms, soft_drop_factor,
                        lock_delay_ms, lock_reset_limit,
                        key_move_left, key_move_right, key_soft_drop, key_rotate_left,
                        key_rotate_right, key_rotate_180, key_hold, key_hard_drop, key_quit
                 FROM config WHERE id = 1",
            )
            .map_err(to_io_err)?;

        let row = stmt
            .query_row([], |row| {
                Ok(StoredConfig {
                    gravity_ms: row.get(0)?,
                    frame_ms: row.get(1)?,
                    das_ms: row.get(2)?,
                    arr_ms: row.get(3)?,
                    dcd_ms: row.get(4)?,
                    soft_drop_factor: row.get(5)?,
                    lock_delay_ms: row.get(6)?,
                    lock_reset_limit: row.get(7)?,
                    key_move_left: row.get(8)?,
                    key_move_right: row.get(9)?,
                    key_soft_drop: row.get(10)?,
                    key_rotate_left: row.get(11)?,
                    key_rotate_right: row.get(12)?,
                    key_rotate_180: row.get(13)?,
                    key_hold: row.get(14)?,
                    key_hard_drop: row.get(15)?,
                    key_quit: row.get(16)?,
                })
            })
            .optional()
            .map_err(to_io_err)?;

        let Some(stored) = row else {
            return Ok(None);
        };

        let mut config = AppConfig::default();
        config.gravity_ms = stored.gravity_ms;
        config.frame_ms = stored.frame_ms;
        config.das_ms = stored.das_ms;
        config.arr_ms = stored.arr_ms;
        config.dcd_ms = stored.dcd_ms;
        config.soft_drop_factor = stored.soft_drop_factor;
        config.lock_delay_ms = stored.lock_delay_ms;
        config.lock_reset_limit = stored.lock_reset_limit;

        let KeyBindings {
            move_left,
            move_right,
            soft_drop,
            rotate_left,
            rotate_right,
            rotate_180,
            hold,
            hard_drop,
            quit,
        } = config.key_bindings;

        config.key_bindings.move_left =
            keycode_from_storage(&stored.key_move_left).unwrap_or(move_left);
        config.key_bindings.move_right =
            keycode_from_storage(&stored.key_move_right).unwrap_or(move_right);
        config.key_bindings.soft_drop =
            keycode_from_storage(&stored.key_soft_drop).unwrap_or(soft_drop);
        config.key_bindings.rotate_left =
            keycode_from_storage(&stored.key_rotate_left).unwrap_or(rotate_left);
        config.key_bindings.rotate_right =
            keycode_from_storage(&stored.key_rotate_right).unwrap_or(rotate_right);
        config.key_bindings.rotate_180 =
            keycode_from_storage(&stored.key_rotate_180).unwrap_or(rotate_180);
        config.key_bindings.hold = keycode_from_storage(&stored.key_hold).unwrap_or(hold);
        config.key_bindings.hard_drop =
            keycode_from_storage(&stored.key_hard_drop).unwrap_or(hard_drop);
        config.key_bindings.quit = keycode_from_storage(&stored.key_quit).unwrap_or(quit);

        Ok(Some(config))
    }

    pub fn save_config(&self, config: &AppConfig) -> io::Result<()> {
        let KeyBindings {
            move_left,
            move_right,
            soft_drop,
            rotate_left,
            rotate_right,
            rotate_180,
            hold,
            hard_drop,
            quit,
        } = config.key_bindings;

        self.conn
            .execute(
                "INSERT INTO config (
                    id, gravity_ms, frame_ms, das_ms, arr_ms, dcd_ms, soft_drop_factor,
                    lock_delay_ms, lock_reset_limit,
                    key_move_left, key_move_right, key_soft_drop, key_rotate_left,
                    key_rotate_right, key_rotate_180, key_hold, key_hard_drop, key_quit
                ) VALUES (
                    1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
                )
                ON CONFLICT(id) DO UPDATE SET
                    gravity_ms = excluded.gravity_ms,
                    frame_ms = excluded.frame_ms,
                    das_ms = excluded.das_ms,
                    arr_ms = excluded.arr_ms,
                    dcd_ms = excluded.dcd_ms,
                    soft_drop_factor = excluded.soft_drop_factor,
                    lock_delay_ms = excluded.lock_delay_ms,
                    lock_reset_limit = excluded.lock_reset_limit,
                    key_move_left = excluded.key_move_left,
                    key_move_right = excluded.key_move_right,
                    key_soft_drop = excluded.key_soft_drop,
                    key_rotate_left = excluded.key_rotate_left,
                    key_rotate_right = excluded.key_rotate_right,
                    key_rotate_180 = excluded.key_rotate_180,
                    key_hold = excluded.key_hold,
                    key_hard_drop = excluded.key_hard_drop,
                    key_quit = excluded.key_quit",
                params![
                    config.gravity_ms,
                    config.frame_ms,
                    config.das_ms,
                    config.arr_ms,
                    config.dcd_ms,
                    config.soft_drop_factor,
                    config.lock_delay_ms,
                    config.lock_reset_limit,
                    keycode_to_storage(move_left),
                    keycode_to_storage(move_right),
                    keycode_to_storage(soft_drop),
                    keycode_to_storage(rotate_left),
                    keycode_to_storage(rotate_right),
                    keycode_to_storage(rotate_180),
                    keycode_to_storage(hold),
                    keycode_to_storage(hard_drop),
                    keycode_to_storage(quit),
                ],
            )
            .map_err(to_io_err)?;
        Ok(())
    }

    pub fn insert_high_score(&self, entry: &HighScore) -> io::Result<()> {
        let created_at = now_epoch_secs();
        self.conn
            .execute(
                "INSERT INTO high_scores (name, score, lines, time_secs, mode, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    &entry.name,
                    entry.score,
                    entry.lines,
                    entry.time_secs,
                    &entry.mode,
                    created_at
                ],
            )
            .map_err(to_io_err)?;
        Ok(())
    }

    pub fn fetch_high_scores(&self, limit: usize) -> io::Result<Vec<HighScore>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT name, score, lines, time_secs, mode
                 FROM high_scores
                 ORDER BY score DESC, created_at DESC
                 LIMIT ?1",
            )
            .map_err(to_io_err)?;

        let rows = stmt
            .query_map([limit as u32], |row| {
                Ok(HighScore {
                    name: row.get(0)?,
                    score: row.get(1)?,
                    lines: row.get(2)?,
                    time_secs: row.get(3)?,
                    mode: row.get(4)?,
                })
            })
            .map_err(to_io_err)?;

        let mut scores = Vec::new();
        for row in rows {
            scores.push(row.map_err(to_io_err)?);
        }
        Ok(scores)
    }

    fn init(&self) -> io::Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS config (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    gravity_ms INTEGER NOT NULL,
                    frame_ms INTEGER NOT NULL,
                    das_ms INTEGER NOT NULL,
                    arr_ms INTEGER NOT NULL,
                    dcd_ms INTEGER NOT NULL,
                    soft_drop_factor INTEGER NOT NULL,
                    lock_delay_ms INTEGER NOT NULL,
                    lock_reset_limit INTEGER NOT NULL,
                    key_move_left TEXT NOT NULL,
                    key_move_right TEXT NOT NULL,
                    key_soft_drop TEXT NOT NULL,
                    key_rotate_left TEXT NOT NULL,
                    key_rotate_right TEXT NOT NULL,
                    key_rotate_180 TEXT NOT NULL,
                    key_hold TEXT NOT NULL,
                    key_hard_drop TEXT NOT NULL,
                    key_quit TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS high_scores (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    score INTEGER NOT NULL,
                    lines INTEGER NOT NULL,
                    time_secs INTEGER NOT NULL,
                    mode TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );",
            )
            .map_err(to_io_err)?;
        Ok(())
    }
}

struct StoredConfig {
    gravity_ms: u64,
    frame_ms: u64,
    das_ms: u64,
    arr_ms: u64,
    dcd_ms: u64,
    soft_drop_factor: u32,
    lock_delay_ms: u64,
    lock_reset_limit: u32,
    key_move_left: String,
    key_move_right: String,
    key_soft_drop: String,
    key_rotate_left: String,
    key_rotate_right: String,
    key_rotate_180: String,
    key_hold: String,
    key_hard_drop: String,
    key_quit: String,
}

fn default_db_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("tetris.db")
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn to_io_err(err: rusqlite::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err.to_string())
}
