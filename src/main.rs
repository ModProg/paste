use std::{env, path::PathBuf};

use actix_web::{web::Data, App, HttpServer};
use anyhow::Result;
use bonsaidb::local::config::StorageConfiguration;
use log::{error, info};
use syntect::parsing::SyntaxSet;

use config::Config;
use db::DB;

mod config;
mod db;
mod simple;
mod util;


pub const RESERVED_URLS: &[&str] = &["raw", "download", "delete"];

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("warn"));

    let config_path = env::var_os("PASTEMP_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    info!(
        "Loading config from `{}`, this can be changed by the env variable `FRONTEND_CONFIG`",
        config_path.display()
    );
    if !config_path.is_file() {
        error!("Config file is not an accessible file");
    }

    let config = Config::load(&config_path)?;

    let database = Data::new(DB::new().await?);
    let config = Data::new(config);
    let syntaxes = Data::new(SyntaxSet::load_defaults_newlines());

    HttpServer::new(move || {
        App::new()
            .app_data(database.clone())
            .app_data(config.clone())
            .app_data(syntaxes.clone())
            .service(simple::scope())
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await?;

    Ok(())
}
