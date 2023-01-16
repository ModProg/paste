use std::path::Path;

use anyhow::{Context, Result};
use chrono::Duration;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;
use serde_with::{serde_as, DurationSeconds};

#[serde_as]
#[derive(Deserialize)]
pub struct Config {
    #[serde_as(as = "DurationSeconds<i64>")]
    pub max_age: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub time_to_delete: Duration,
    pub base_url: String,
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
