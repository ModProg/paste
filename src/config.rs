use std::path::Path;

use anyhow::{Context, Result};
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub max_age: f64,
    pub time_to_delete: f64,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        Figment::new()
            .merge(Toml::string(include_str!("../config.toml")))
            .merge(Toml::file(&path))
            .merge(Env::prefixed("PASTEMP_"))
            .extract()
            .context("Loading Config")
    }
}
