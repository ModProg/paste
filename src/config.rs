use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use figment::providers::Format;
use figment::{
    providers::{Env, Toml},
    Figment,
};
use serde::Deserialize;
use url::Url;

#[derive(Deserialize)]
pub struct Config {
    pub base_url: Url,
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
