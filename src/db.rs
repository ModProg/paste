use bonsaidb::{
    core::schema::{self, Qualified},
    local::{config::Builder, AsyncDatabase},
};
use bonsaidb_files::{
    direct::{self, Async},
    FileConfig, FilesSchema, Truncate,
};
use chrono::{Duration, Utc};
use rand::distributions::DistString;
use serde::{Deserialize, Serialize};

use crate::{util::ReadableAlphanumeric, StorageConfiguration, RESERVED_URLS};

type Result<T = ()> = std::result::Result<T, bonsaidb::core::Error>;
pub type DateTime = chrono::DateTime<Utc>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metadata {
    pub delete_at: Option<DateTime>,
    pub owner: String,
}

pub struct Files;
impl FileConfig for Files {
    type Metadata = Metadata;

    const BLOCK_SIZE: usize = 65_536;

    fn files_name() -> schema::CollectionName {
        Qualified::private("files")
    }

    fn blocks_name() -> schema::CollectionName {
        Qualified::private("blocks")
    }
}

type File = direct::File<Async<AsyncDatabase>, Files>;

#[derive(Debug, schema::Schema)]
#[schema(name = "paste", include=[FilesSchema<Files>])]
struct Schema;

pub struct DB(AsyncDatabase);

impl DB {
    pub async fn new() -> Result<Self> {
        Ok(Self(
            AsyncDatabase::open::<Schema>(StorageConfiguration::new("data.bonsaidb")).await?,
        ))
    }

    pub async fn delete_at(&self, name: &str, delete_at: DateTime) -> Result {
        if let Some(mut file) = Files::load_async(name, &self.0).await? {
            let metadata = file.metadata().expect("all files have metadata").clone();
            file.update_metadata(Metadata {
                delete_at: Some(delete_at),
                ..metadata
            })
            .await?;
        }
        Ok(())
    }

    pub async fn file_owner(&self, name: &str) -> Result<Option<String>> {
        Ok(Files::load_async(name, &self.0)
            .await?
            .map(|m| m.metadata().expect("all files have metadata").owner.clone()))
    }

    pub async fn load_file(&self, name: &str) -> Result<Option<File>> {
        if let Some(file) = Files::load_async(name, &self.0).await? {
            if let Some(delete_at) = file.metadata().and_then(|m| m.delete_at) {
                if Utc::now() > delete_at {
                    file.delete().await?;
                    return Ok(None);
                }
            }
            Ok(Some(file))
        } else {
            Ok(None)
        }
    }

    pub async fn new_file(&self, owner: String, ttl: Option<Duration>) -> Result<File> {
        let mut tries = 0;
        // TODO auto increase
        let length = 4;
        let mut file = loop {
            let name = loop {
                let id = ReadableAlphanumeric.sample_string(&mut rand::thread_rng(), length);
                if !RESERVED_URLS.contains(&id.as_str()) {
                    break id;
                }
            };
            tries += 1;
            match Files::build(&name).create_async(&self.0).await {
                Ok(file) => break file,
                Err(bonsaidb_files::Error::Database(
                    bonsaidb::core::Error::UniqueKeyViolation { .. },
                )) if tries > 5 => {
                    let file = Files::load_or_create_async(&name, true, &self.0).await?;
                    file.truncate(0, Truncate::RemovingStart).await?;
                    break file;
                }
                Err(bonsaidb_files::Error::Database(
                    bonsaidb::core::Error::UniqueKeyViolation { .. },
                )) => continue,
                Err(err) => return Err(err.into()),
            }
        };

        file.update_metadata(Some(Metadata {
            delete_at: ttl.map(|ttl| Utc::now() + ttl),
            owner,
        }))
        .await?;

        Ok(file)
    }
}
