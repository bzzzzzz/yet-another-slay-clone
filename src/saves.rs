use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use chrono::prelude::*;
use serde_yaml;

use crate::game::GameEngine;

const VERSION: u8 = 1;

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct SavedGameInfo {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub version: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedGame {
    info: SavedGameInfo,
    engine: GameEngine,
}

pub struct SavedGamesCatalog {
    version: u8,
    root: PathBuf,
    prefix: String,
    saved_games: Vec<SavedGameInfo>,
}

pub enum CatalogInitiationErr {
    BadRoot,
    NoRights,
}

impl SavedGamesCatalog {
    pub fn new(root: &str, prefix: &str) -> io::Result<SavedGamesCatalog> {
        let root = Path::new(root);
        if !root.exists() {
            fs::create_dir_all(root)?;
        }
        if !root.is_dir() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut saved_games = Vec::new();
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_file() {
                let file_name = path.file_name().unwrap().to_str();
                if file_name.is_none() {
                    continue;
                }
                let file_name = file_name.unwrap();
                if !file_name.starts_with(prefix) {
                    continue;
                }
                let parts: Vec<_> = file_name.split('_').collect();
                if parts.len() != 4 || parts[0] != prefix {
                    continue;
                }
                let info_version: Result<u8, _> = parts[1].parse();
                if info_version.is_err() {
                    continue;
                }
                let info_version = info_version.unwrap();
                if info_version != VERSION {
                    continue;
                }
                let name = parts[2].to_string();
                let timestamp: Result<DateTime<Utc>, _> =
                    Utc.datetime_from_str(&parts[3], "%Y%m%d%H%M%S.yaml");
                if timestamp.is_err() {
                    continue;
                }
                saved_games.push(SavedGameInfo {
                    name,
                    timestamp: timestamp.unwrap(),
                    version: info_version,
                });
            }
        }
        info!("Successfully initiated saved games catalog with path {:?}, prefix {:?} and existing games {:?}",
            root, prefix, saved_games);
        Ok(SavedGamesCatalog {
            saved_games,
            version: VERSION,
            prefix: prefix.to_owned(),
            root: root.to_owned(),
        })
    }

    pub fn list_saved_games(&self) -> &Vec<SavedGameInfo> {
        &self.saved_games
    }

    pub fn save(&mut self, name: &str, engine: &GameEngine) -> io::Result<SavedGameInfo> {
        info!("Trying to save game as '{}'", name);
        let state = self.create_game_state(name, engine.clone());
        let path = self.save_file_path(&state.info);

        let buffer = File::create(path.as_path()).unwrap();
        serde_yaml::to_writer(buffer, &state).unwrap();

        info!(
            "Successfully saved '{:?}' to {:?}",
            state.info,
            path.as_path()
        );
        self.saved_games.push(state.info.clone());

        Ok(state.info)
    }

    fn save_file_path(&self, saved_game: &SavedGameInfo) -> PathBuf {
        let file_name = format!(
            "{}_{}_{}_{}.yaml",
            self.prefix,
            saved_game.version,
            saved_game.name,
            saved_game.timestamp.format("%Y%m%d%H%M%S")
        );
        self.root.join(file_name)
    }

    fn create_game_state(&self, name: &str, engine: GameEngine) -> SavedGame {
        let timestamp = Utc::now().with_nanosecond(0).unwrap();
        let info = SavedGameInfo {
            timestamp,
            name: String::from(name),
            version: self.version,
        };

        SavedGame { info, engine }
    }

    pub fn load(&self, game: &SavedGameInfo) -> io::Result<GameEngine> {
        let saved_game = self.saved_games.iter().find(|&g| g == game);
        if saved_game.is_none() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        let path = self.save_file_path(game);
        let buffer = File::open(path)?;
        let mut state: SavedGame = serde_yaml::from_reader(buffer).unwrap();

        state.engine.repair();
        Ok(state.engine)
    }
}
