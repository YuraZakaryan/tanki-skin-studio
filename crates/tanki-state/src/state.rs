// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::session::{SESSION_KEY, SessionState};
use anyhow::Result;
use rusqlite::{Connection, params};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tanki_pack::pack::{InstalledPack, load_pack};

/// Settings keys of the interface preferences, kept here so every client agrees on them.
const KEY_LANGUAGE: &str = "language";
const KEY_THEME: &str = "theme";
const KEY_UI_SCALE: &str = "ui_scale";

pub struct StateStore {
    conn: Connection,
}
impl StateStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS packs (
                id TEXT NOT NULL, version TEXT NOT NULL, name TEXT NOT NULL,
                path TEXT NOT NULL, imported_at INTEGER NOT NULL,
                PRIMARY KEY(id, version));
            CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS user_targets (
                game TEXT NOT NULL, marker TEXT NOT NULL, kind TEXT NOT NULL,
                id TEXT NOT NULL, grade TEXT NOT NULL,
                PRIMARY KEY(game, marker));
            CREATE TABLE IF NOT EXISTS recent_assets (
                key TEXT PRIMARY KEY, selected_at INTEGER NOT NULL);",
        )?;
        let store = Self { conn };
        store.load_user_targets()?;
        Ok(store)
    }
    pub fn save_pack(&self, pack: &InstalledPack) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO packs(id,version,name,path,imported_at) VALUES(?1,?2,?3,?4,unixepoch())",
            params![pack.manifest.id,pack.manifest.version,pack.manifest.name,pack.path.to_string_lossy()])?;
        self.set_active_pack(&pack.manifest.id, &pack.manifest.version)
    }
    pub fn list_packs(&self) -> Result<Vec<InstalledPack>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM packs ORDER BY imported_at DESC, rowid DESC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            let path = PathBuf::from(row?);
            if let Ok(manifest) = load_pack(&path) {
                result.push(InstalledPack { path, manifest });
            }
        }
        Ok(result)
    }
    pub fn set_active_pack(&self, id: &str, version: &str) -> Result<()> {
        self.set("active_pack", &format!("{id}@{version}"))
    }
    pub fn active_pack(&self) -> Result<Option<InstalledPack>> {
        let Some(key) = self.get("active_pack")? else {
            return Ok(None);
        };
        Ok(self
            .list_packs()?
            .into_iter()
            .find(|p| format!("{}@{}", p.manifest.id, p.manifest.version) == key))
    }
    /// Forget an imported pack and delete its files. The active pack falls back to the newest
    /// remaining one.
    pub fn delete_pack(&self, id: &str, version: &str) -> Result<()> {
        let path: Option<String> = self
            .conn
            .query_row(
                "SELECT path FROM packs WHERE id=?1 AND version=?2",
                params![id, version],
                |row| row.get(0),
            )
            .ok();
        self.conn.execute(
            "DELETE FROM packs WHERE id=?1 AND version=?2",
            params![id, version],
        )?;
        if let Some(path) = path.map(PathBuf::from) {
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else if path.is_file() {
                fs::remove_file(&path)?;
            }
        }
        if self.get("active_pack")?.as_deref() == Some(&format!("{id}@{version}")) {
            match self.list_packs()?.first() {
                Some(next) => self.set_active_pack(&next.manifest.id, &next.manifest.version)?,
                None => {
                    self.conn
                        .execute("DELETE FROM settings WHERE key='active_pack'", [])?;
                }
            }
        }
        Ok(())
    }
    /// The saved interface language code (`en`, `ru`), if the user ever chose one.
    pub fn language(&self) -> Result<Option<String>> {
        self.get(KEY_LANGUAGE)
    }
    pub fn set_language(&self, code: &str) -> Result<()> {
        self.set(KEY_LANGUAGE, code)
    }
    /// The saved colour theme (`dark`, `light`), if the user ever chose one.
    pub fn theme(&self) -> Result<Option<String>> {
        self.get(KEY_THEME)
    }
    pub fn set_theme(&self, name: &str) -> Result<()> {
        self.set(KEY_THEME, name)
    }
    /// The saved session; unreadable or missing data gives the default (never an error).
    pub fn session(&self) -> SessionState {
        match self.get(SESSION_KEY) {
            Ok(Some(text)) => SessionState::from_json(&text),
            _ => SessionState::default(),
        }
    }
    /// Save the session if it differs from what is stored. Returns whether anything was written.
    pub fn save_session(&self, session: &SessionState) -> Result<bool> {
        let text = session.to_json();
        if self.get(SESSION_KEY)?.as_deref() == Some(text.as_str()) {
            return Ok(false);
        }
        self.set(SESSION_KEY, &text)?;
        Ok(true)
    }
    /// The saved interface size as a percentage string (`125`), if the user ever changed it.
    pub fn ui_scale(&self) -> Result<Option<String>> {
        self.get(KEY_UI_SCALE)
    }
    pub fn set_ui_scale(&self, percent: u32) -> Result<()> {
        self.set(KEY_UI_SCALE, &percent.to_string())
    }
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
        Ok(())
    }
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .ok())
    }
    /// Persist a hand-made label and make it effective immediately.
    pub fn save_user_target(
        &self,
        game: &str,
        marker: &str,
        kind: &str,
        id: &str,
        grade: &str,
    ) -> Result<()> {
        tanki_cache::targets::register_user_target(game, marker, kind, id, grade)
            .map_err(anyhow::Error::msg)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO user_targets(game,marker,kind,id,grade) VALUES(?1,?2,?3,?4,?5)",
            params![game, marker, kind, id, grade],
        )?;
        Ok(())
    }
    /// Label one modification and, by the consecutive-id rule, its M0..M3 siblings that exist in the
    /// cache (`exists` tells whether a marker is present) and are not claimed yet.
    /// Returns the grades that were labelled.
    pub fn save_user_item(
        &self,
        game: &str,
        marker: &str,
        kind: &str,
        id: &str,
        grade: &str,
        exists: &dyn Fn(&str) -> bool,
    ) -> Result<Vec<String>> {
        let block = tanki_cache::targets::modification_markers(marker, grade)
            .ok_or_else(|| anyhow::anyhow!("grade must be m0..m3 to label a whole item"))?;
        let mut labelled = Vec::new();
        for (sibling, sibling_grade) in block {
            if sibling != marker
                && (!exists(&sibling) || tanki_cache::targets::is_marker_known(game, &sibling))
            {
                continue;
            }
            self.save_user_target(game, &sibling, kind, id, &sibling_grade)?;
            labelled.push(sibling_grade);
        }
        Ok(labelled)
    }
    pub fn delete_user_target(&self, game: &str, marker: &str) -> Result<()> {
        tanki_cache::targets::unregister_user_target(game, marker);
        self.conn.execute(
            "DELETE FROM user_targets WHERE game=?1 AND marker=?2",
            params![game, marker],
        )?;
        Ok(())
    }
    /// Re-register every stored label; call once after opening the store.
    pub fn load_user_targets(&self) -> Result<usize> {
        let mut stmt = self
            .conn
            .prepare("SELECT game,marker,kind,id,grade FROM user_targets")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut loaded = 0;
        for (game, marker, kind, id, grade) in rows {
            if tanki_cache::targets::register_user_target(&game, &marker, &kind, &id, &grade)
                .is_ok()
            {
                loaded += 1;
            }
        }
        Ok(loaded)
    }
    pub fn record_recent_asset(&self, key: &str) -> Result<()> {
        self.conn.execute("INSERT INTO recent_assets(key,selected_at) VALUES(?1,unixepoch()) ON CONFLICT(key) DO UPDATE SET selected_at=excluded.selected_at", [key])?;
        Ok(())
    }
    pub fn recent_assets(&self, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT key FROM recent_assets ORDER BY selected_at DESC, rowid DESC LIMIT ?1",
        )?;
        Ok(stmt
            .query_map([limit as i64], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
