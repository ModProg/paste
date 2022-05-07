use crate::{util::ReadableAlphanumeric, StorageConfiguration, RESERVED_URLS};
use bonsaidb::{
    core::schema,
    local::{config::Builder, AsyncDatabase},
};
use bonsaidb_files::{
    direct::{Async, File},
    BonsaiFiles, FileConfig, FilesSchema, Truncate,
};
use rand::distributions::DistString;

type Result<T> = std::result::Result<T, bonsaidb::core::Error>;
type FileResult<T> = std::result::Result<T, bonsaidb_files::Error>;

#[derive(Debug, schema::Schema)]
#[schema(name = "paste", include=[FilesSchema<BonsaiFiles>])]
struct Schema;

pub struct DB(pub AsyncDatabase);

impl DB {
    pub async fn new() -> Result<Self> {
        Ok(Self(
            AsyncDatabase::open::<Schema>(StorageConfiguration::new("data.bonsaidb")).await?,
        ))
    }

    pub async fn load_file(&self, name: &str) -> FileResult<Option<File<Async<AsyncDatabase>>>> {
        // TODO delete file if marked for deletion
        BonsaiFiles::load_async(name, &self.0).await
    }

    pub async fn new_file(&self) -> FileResult<File<Async<AsyncDatabase>>> {
        let mut tries = 0;
        // TODO auto increase
        let length = 4;
        Ok(loop {
            let name = loop {
                let id = ReadableAlphanumeric.sample_string(&mut rand::thread_rng(), length);
                if !RESERVED_URLS.contains(&id.as_str()) {
                    break id;
                }
            };
            tries += 1;
            match BonsaiFiles::build(&name).create_async(&self.0).await {
                Ok(file) => break file,
                Err(bonsaidb_files::Error::Database(
                    bonsaidb::core::Error::UniqueKeyViolation { .. },
                )) if tries > 5 => {
                    let file = BonsaiFiles::load_or_create_async(&name, true, &self.0).await?;
                    file.truncate(0, Truncate::RemovingStart).await?;
                    break file;
                }
                Err(bonsaidb_files::Error::Database(
                    bonsaidb::core::Error::UniqueKeyViolation { .. },
                )) => continue,
                Err(err) => return Err(err),
            }
        })
    }
}
