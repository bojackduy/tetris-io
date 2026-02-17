use std::io;
use std::path::PathBuf;

use tetris_adapter_storage_sqlite::{HighScoreEntry, SqliteStorage, StoredConfig};
use tetris_adapter_tui::input::{KeyBindings, keycode_from_storage, keycode_to_storage};
use tetris_application::ports::{ConfigPort, HighScorePort};

use crate::app::config::AppConfig;

#[derive(Debug, Clone)]
pub struct HighScore {
    pub name: String,
    pub score: u32,
    pub lines: u32,
    pub time_secs: u64,
    pub mode: String,
}

pub struct Storage {
    inner: SqliteStorage,
}

impl Storage {
    pub fn open_default() -> io::Result<Self> {
        Ok(Self {
            inner: SqliteStorage::open_default()?,
        })
    }

    pub fn open(path: PathBuf) -> io::Result<Self> {
        Ok(Self {
            inner: SqliteStorage::open(path)?,
        })
    }

    pub fn load_config(&self) -> io::Result<Option<AppConfig>> {
        let row = self.inner.load_config()?;
        let Some(stored) = row else {
            return Ok(None);
        };

        let defaults = AppConfig::default();
        let key_bindings = KeyBindings {
            move_left: keycode_from_storage(&stored.key_move_left)
                .unwrap_or(defaults.key_bindings.move_left),
            move_right: keycode_from_storage(&stored.key_move_right)
                .unwrap_or(defaults.key_bindings.move_right),
            soft_drop: keycode_from_storage(&stored.key_soft_drop)
                .unwrap_or(defaults.key_bindings.soft_drop),
            rotate_left: keycode_from_storage(&stored.key_rotate_left)
                .unwrap_or(defaults.key_bindings.rotate_left),
            rotate_right: keycode_from_storage(&stored.key_rotate_right)
                .unwrap_or(defaults.key_bindings.rotate_right),
            rotate_180: keycode_from_storage(&stored.key_rotate_180)
                .unwrap_or(defaults.key_bindings.rotate_180),
            hold: keycode_from_storage(&stored.key_hold).unwrap_or(defaults.key_bindings.hold),
            hard_drop: keycode_from_storage(&stored.key_hard_drop)
                .unwrap_or(defaults.key_bindings.hard_drop),
            quit: keycode_from_storage(&stored.key_quit).unwrap_or(defaults.key_bindings.quit),
        };

        let config = AppConfig {
            gravity_ms: stored.gravity_ms,
            frame_ms: stored.frame_ms,
            das_ms: stored.das_ms,
            arr_ms: stored.arr_ms,
            dcd_ms: stored.dcd_ms,
            soft_drop_factor: stored.soft_drop_factor,
            lock_delay_ms: stored.lock_delay_ms,
            lock_reset_limit: stored.lock_reset_limit,
            key_bindings,
        };

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

        let stored = StoredConfig {
            gravity_ms: config.gravity_ms,
            frame_ms: config.frame_ms,
            das_ms: config.das_ms,
            arr_ms: config.arr_ms,
            dcd_ms: config.dcd_ms,
            soft_drop_factor: config.soft_drop_factor,
            lock_delay_ms: config.lock_delay_ms,
            lock_reset_limit: config.lock_reset_limit,
            key_move_left: keycode_to_storage(move_left),
            key_move_right: keycode_to_storage(move_right),
            key_soft_drop: keycode_to_storage(soft_drop),
            key_rotate_left: keycode_to_storage(rotate_left),
            key_rotate_right: keycode_to_storage(rotate_right),
            key_rotate_180: keycode_to_storage(rotate_180),
            key_hold: keycode_to_storage(hold),
            key_hard_drop: keycode_to_storage(hard_drop),
            key_quit: keycode_to_storage(quit),
        };

        self.inner.save_config(&stored)
    }

    pub fn insert_high_score(&self, entry: &HighScore) -> io::Result<()> {
        let record = HighScoreEntry {
            name: entry.name.clone(),
            score: entry.score,
            lines: entry.lines,
            time_secs: entry.time_secs,
            mode: entry.mode.clone(),
        };
        self.inner.insert_high_score(&record)
    }

    pub fn fetch_high_scores(&self, limit: usize) -> io::Result<Vec<HighScore>> {
        let rows = self.inner.fetch_high_scores(limit)?;
        Ok(rows
            .into_iter()
            .map(|row| HighScore {
                name: row.name,
                score: row.score,
                lines: row.lines,
                time_secs: row.time_secs,
                mode: row.mode,
            })
            .collect())
    }
}

impl ConfigPort<AppConfig> for Storage {
    type Error = io::Error;

    fn load_config(&self) -> Result<Option<AppConfig>, Self::Error> {
        Storage::load_config(self)
    }

    fn save_config(&self, config: &AppConfig) -> Result<(), Self::Error> {
        Storage::save_config(self, config)
    }
}

impl HighScorePort<HighScore> for Storage {
    type Error = io::Error;

    fn insert_high_score(&self, score: &HighScore) -> Result<(), Self::Error> {
        Storage::insert_high_score(self, score)
    }

    fn fetch_high_scores(&self, limit: usize) -> Result<Vec<HighScore>, Self::Error> {
        Storage::fetch_high_scores(self, limit)
    }
}
