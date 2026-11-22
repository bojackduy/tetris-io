use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone)]
pub struct HighScoreEntry {
    pub name: String,
    pub score: u32,
    pub lines: u32,
    pub time_secs: u64,
    pub mode: String,
}

#[derive(Debug, Clone)]
pub struct StoredConfig {
    pub gravity_ms: u64,
    pub frame_ms: u64,
    pub das_ms: u64,
    pub arr_ms: u64,
    pub dcd_ms: u64,
    pub soft_drop_factor: u32,
    pub lock_delay_ms: u64,
    pub lock_reset_limit: u32,
    pub sound_volume: u8,
    pub rotation_sound: bool,
    pub ghost: bool,
    pub grid_style: u8,
    pub key_move_left: String,
    pub key_move_right: String,
    pub key_soft_drop: String,
    pub key_rotate_left: String,
    pub key_rotate_right: String,
    pub key_rotate_180: String,
    pub key_hold: String,
    pub key_hard_drop: String,
    pub key_quit: String,
}

pub struct SqliteStorage {
    conn: Connection,
}

impl SqliteStorage {
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

    pub fn load_config(&self) -> io::Result<Option<StoredConfig>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT gravity_ms, frame_ms, das_ms, arr_ms, dcd_ms, soft_drop_factor,
                        lock_delay_ms, lock_reset_limit,
                        key_move_left, key_move_right, key_soft_drop, key_rotate_left,
                        key_rotate_right, key_rotate_180, key_hold, key_hard_drop, key_quit,
                        sound_volume, rotation_sound, ghost, grid_style
                 FROM config WHERE id = 1",
            )
            .map_err(to_io_err)?;

        stmt.query_row([], |row| {
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
                sound_volume: row.get(17)?,
                rotation_sound: row.get(18)?,
                ghost: row.get(19)?,
                grid_style: row.get(20)?,
            })
        })
        .optional()
        .map_err(to_io_err)
    }

    pub fn save_config(&self, config: &StoredConfig) -> io::Result<()> {
        self.conn
            .execute(
                "INSERT INTO config (
                    id, gravity_ms, frame_ms, das_ms, arr_ms, dcd_ms, soft_drop_factor,
                    lock_delay_ms, lock_reset_limit,
                    key_move_left, key_move_right, key_soft_drop, key_rotate_left,
                    key_rotate_right, key_rotate_180, key_hold, key_hard_drop, key_quit,
                    sound_volume, rotation_sound, ghost, grid_style, profile_version
                ) VALUES (
                    1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                    ?18, ?19, ?20, ?21, 2
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
                    key_quit = excluded.key_quit,
                    sound_volume = excluded.sound_volume,
                    rotation_sound = excluded.rotation_sound,
                    ghost = excluded.ghost,
                    grid_style = excluded.grid_style,
                    profile_version = excluded.profile_version",
                params![
                    config.gravity_ms,
                    config.frame_ms,
                    config.das_ms,
                    config.arr_ms,
                    config.dcd_ms,
                    config.soft_drop_factor,
                    config.lock_delay_ms,
                    config.lock_reset_limit,
                    config.key_move_left,
                    config.key_move_right,
                    config.key_soft_drop,
                    config.key_rotate_left,
                    config.key_rotate_right,
                    config.key_rotate_180,
                    config.key_hold,
                    config.key_hard_drop,
                    config.key_quit,
                    config.sound_volume,
                    config.rotation_sound,
                    config.ghost,
                    config.grid_style,
                ],
            )
            .map_err(to_io_err)?;
        Ok(())
    }

    pub fn insert_high_score(&self, entry: &HighScoreEntry) -> io::Result<()> {
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

    pub fn fetch_high_scores(&self, limit: usize) -> io::Result<Vec<HighScoreEntry>> {
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
                Ok(HighScoreEntry {
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
                    key_quit TEXT NOT NULL,
                    sound_volume INTEGER NOT NULL DEFAULT 60,
                    rotation_sound INTEGER NOT NULL DEFAULT 0,
                    ghost INTEGER NOT NULL DEFAULT 1,
                    grid_style INTEGER NOT NULL DEFAULT 1,
                    profile_version INTEGER NOT NULL DEFAULT 2
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
        let columns = self
            .conn
            .prepare("PRAGMA table_info(config)")
            .map_err(to_io_err)?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(to_io_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_io_err)?;
        // Additive migration keeps scores and custom key bindings intact.
        for (name, definition) in [
            ("sound_volume", "INTEGER NOT NULL DEFAULT 60"),
            ("rotation_sound", "INTEGER NOT NULL DEFAULT 0"),
            ("ghost", "INTEGER NOT NULL DEFAULT 1"),
            ("grid_style", "INTEGER NOT NULL DEFAULT 1"),
            ("profile_version", "INTEGER NOT NULL DEFAULT 1"),
        ] {
            if !columns.iter().any(|column| column == name) {
                self.conn
                    .execute_batch(&format!(
                        "ALTER TABLE config ADD COLUMN {name} {definition}"
                    ))
                    .map_err(to_io_err)?;
            }
        }
        self.conn
            .execute_batch(
                "BEGIN;
                 UPDATE config SET gravity_ms = 900, das_ms = 133, arr_ms = 10, dcd_ms = 0,
                     soft_drop_factor = 120, lock_delay_ms = 500, lock_reset_limit = 0,
                     profile_version = 2
                 WHERE profile_version = 1 AND gravity_ms = 1000 AND das_ms = 167
                     AND arr_ms = 16 AND dcd_ms = 33 AND soft_drop_factor = 6
                     AND lock_delay_ms = 700 AND lock_reset_limit = 10;
                 UPDATE config SET soft_drop_factor = 120, profile_version = 2
                 WHERE profile_version = 1;
                 COMMIT;",
            )
            .map_err(to_io_err)?;
        Ok(())
    }
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
    io::Error::other(err.to_string())
}
