use std::{fmt::Display, str};

use actix_multipart::Multipart;
use actix_web::{
    dev::HttpServiceFactory,
    error::{ErrorInternalServerError, ErrorNotFound},
    get,
    http::{
        header::{self, ContentDisposition, DispositionParam, DispositionType},
        StatusCode,
    },
    post,
    web::Data,
    HttpResponse, Responder, ResponseError, Result,
};
use actix_web_codegen::routes;
use actix_web_lab::extract::Path;
use askama::Template;
use askama_actix::TemplateToResponse;
use bonsaidb::local::AsyncDatabase;
use bonsaidb_files::{BonsaiFiles, FileConfig, Truncate};
use futures::{future::ready, StreamExt, TryStreamExt};
use mime_guess::mime::{APPLICATION_OCTET_STREAM, IMAGE};
use rand::distributions::DistString;
use serde::Deserialize;
use syntect::{html::ClassedHTMLGenerator, parsing::SyntaxSet, util::LinesWithEndings};

use crate::util::ReadableAlphanumeric;

const RESERVED_URLS: &[&str] = &["raw", "download", "delete"];

pub fn scope() -> impl HttpServiceFactory {
    (
        delete_entry,
        raw,
        download,
        get_ext,
        post_form,
        index,
        redir_down,
    )
}

#[get("/{smth}/{tail:.*}")]
async fn redir_down() -> impl Responder {
    HttpResponse::Found()
        .append_header((header::LOCATION, ".."))
        .finish()
}

#[get("/")]
async fn index() -> impl Responder {
    #[derive(Template)]
    #[template(path = "upload.html")]
    struct Upload;

    Upload
}

#[routes]
#[get("download/{id}.{ext}")]
#[get("download/{id}")]
async fn download(
    Path(file_name): Path<FileName>,
    database: Data<AsyncDatabase>,
) -> Result<impl Responder> {
    if let Some(file) = BonsaiFiles::load_async(&file_name.id, database.as_ref())
        .await
        .map_err(ErrorInternalServerError)?
    {
        Ok(HttpResponse::Ok()
            .content_type(
                file_name
                    .ext
                    .as_deref()
                    .and_then(|ext| mime_guess::from_ext(ext).first())
                    .unwrap_or(APPLICATION_OCTET_STREAM),
            )
            .insert_header(ContentDisposition {
                disposition: DispositionType::Attachment,
                parameters: vec![DispositionParam::Filename(file_name.to_string())],
            })
            .streaming(
                file.contents()
                    .await
                    .map_err(ErrorInternalServerError)?
                    .map_ok(From::from),
            ))
    } else {
        Err(ErrorNotFound(format!(
            "Entry {} does not exist",
            file_name.id
        )))
    }
}

#[routes]
#[get("raw/{id}.{ext}")]
#[get("raw/{id}")]
async fn raw(
    Path(FileName { id, ext }): Path<FileName>,
    database: Data<AsyncDatabase>,
) -> Result<impl Responder> {
    if let Some(file) = BonsaiFiles::load_async(&id, database.as_ref())
        .await
        .map_err(ErrorInternalServerError)?
    {
        Ok(HttpResponse::Ok()
            .content_type(
                ext.as_deref()
                    .and_then(|ext| mime_guess::from_ext(ext).first())
                    .unwrap_or(APPLICATION_OCTET_STREAM),
            )
            .streaming(
                file.contents()
                    .await
                    .map_err(ErrorInternalServerError)?
                    .map_ok(From::from),
            ))
    } else {
        Err(ErrorNotFound(format!("Entry {id} does not exist")))
    }
}

#[derive(Template)]
#[template(path = "code.html", escape = "none")]
struct Highlighted {
    code: String,
    file_name: FileName,
}

#[derive(Template)]
#[template(path = "code.html")]
struct UnHighlighted {
    code: String,
    file_name: FileName,
}

#[derive(Template)]
#[template(path = "image.html")]
struct Image {
    file_name: FileName,
}

#[derive(Template)]
#[template(path = "wrong_type.html")]
struct WrongType {
    file_name: FileName,
}

#[derive(Template)]
#[template(path = "too_large.html")]
struct TooLarge {
    file_name: FileName,
}

#[derive(Template)]
#[template(path = "404.html")]
struct NotFound;

#[routes]
#[get("{id}.{ext}")]
#[get("{id}")]
async fn get_ext(
    Path(file_name): Path<FileName>,
    database: Data<AsyncDatabase>,
    syntaxes: Data<SyntaxSet>,
) -> Result<impl Responder> {
    Ok(
        if let Some(file) = BonsaiFiles::load_async(&file_name.id, database.as_ref())
            .await
            .map_err(ErrorInternalServerError)?
        {
            let mime = file_name
                .ext
                .as_ref()
                .and_then(|ext| mime_guess::from_ext(ext).first());
            let syntax = file_name
                .ext
                .as_ref()
                .and_then(|ext| syntaxes.find_syntax_by_token(ext));
            let file = file.contents().await.map_err(ErrorInternalServerError)?;

            match mime {
                Some(mime) if mime.type_() == IMAGE => Image { file_name }.to_response(),
                _ if file.len() < 10_000 => {
                    if let Ok(file) =
                        String::from_utf8(file.to_vec().await.map_err(ErrorInternalServerError)?)
                    {
                        if let Some(syntax) = syntax {
                            let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
                                syntax,
                                syntaxes.as_ref(),
                                syntect::html::ClassStyle::SpacedPrefixed { prefix: "code-" },
                            );

                            for line in LinesWithEndings::from(&file) {
                                html_generator.parse_html_for_line_which_includes_newline(line);
                            }
                            Highlighted {
                                code: html_generator.finalize(),
                                file_name,
                            }
                            .to_response()
                        } else {
                            UnHighlighted {
                                code: file,
                                file_name,
                            }
                            .to_response()
                        }
                    } else {
                        WrongType { file_name }.to_response()
                    }
                }
                _ => TooLarge { file_name }.to_response(),
            }
            .customize()
        } else {
            NotFound
                .to_response()
                .customize()
                .with_status(StatusCode::NOT_FOUND)
        },
    )
}

#[derive(Deserialize)]
struct FileName {
    id: String,
    ext: Option<String>,
}

impl Display for FileName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { id, ext } = self;
        write!(f, "{id}")?;
        if let Some(ext) = ext {
            write!(f, ".{ext}")?;
        }
        Ok(())
    }
}

#[routes]
#[delete("{id}.{ext}")]
#[delete("{id}")]
#[get("delete/{id}")]
#[get("delete/{id}.{ext}")]
async fn delete_entry(
    Path(FileName { id, .. }): Path<FileName>,
    database: Data<AsyncDatabase>,
) -> Result<impl Responder> {
    if let Some(file) = BonsaiFiles::load_async(&id, database.as_ref())
        .await
        .map_err(ErrorInternalServerError)?
    {
        file.delete().await.map_err(ErrorInternalServerError)?;
    }
    Ok(HttpResponse::Found()
        .append_header((header::LOCATION, "/"))
        .finish())
}

#[derive(Debug, thiserror::Error)]
enum UploadError {
    #[error("Field `{0}` was too big, maximum is {1}")]
    FieldTooBig(&'static str, usize),
    #[error("Field `{0}` is invalid")]
    InvalidField(String),
    #[error("No text or file")]
    NoData,
}

impl ResponseError for UploadError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

#[post("/")]
async fn post_form(payload: Multipart, database: Data<AsyncDatabase>) -> Result<impl Responder> {
    let database = database.as_ref();
    let mut multipart = payload;
    let mut extension = None;

    let mut tries = 0;
    // TODO auto increase
    let length = 4;
    let file = loop {
        let name = loop {
            let id = ReadableAlphanumeric.sample_string(&mut rand::thread_rng(), length);
            if !RESERVED_URLS.contains(&id.as_str()) {
                break id;
            }
        };
        tries += 1;
        match BonsaiFiles::build(&name).create_async(database).await {
            Ok(file) => break file,
            Err(bonsaidb_files::Error::Database(bonsaidb::core::Error::UniqueKeyViolation {
                ..
            })) if tries > 5 => {
                let file = BonsaiFiles::load_or_create_async(&name, true, database)
                    .await
                    .map_err(ErrorInternalServerError)?;
                file.truncate(0, Truncate::RemovingStart)
                    .await
                    .map_err(ErrorInternalServerError)?;
                break file;
            }
            Err(bonsaidb_files::Error::Database(bonsaidb::core::Error::UniqueKeyViolation {
                ..
            })) => continue,
            Err(err) => return Err(ErrorInternalServerError(err)),
        }
    };

    while let Some(mut field) = multipart.try_next().await? {
        const FILE_LIMIT: usize = 10_000_000;
        let mut limit: usize = FILE_LIMIT;
        match field.name() {
            "data" => {
                if let Some(file_name) = field.content_disposition().get_filename() {
                    if file_name.contains('.') {
                        let ext = file_name
                            .split('.')
                            .next_back()
                            .expect("file name contains '.'")
                            .trim();
                        if !ext.is_empty() {
                            extension = Some(ext.to_owned())
                        }
                    }
                }
                while let Some(data) = field.try_next().await? {
                    if let Some(l) = limit.checked_sub(data.len()) {
                        limit = l;
                    } else {
                        field.for_each(|_| ready(())).await;
                        return Err(UploadError::FieldTooBig("file", FILE_LIMIT).into());
                    }
                    file.append(&data).await.map_err(ErrorInternalServerError)?;
                }
            }
            "extension" => {
                let mut buf = String::new();
                while let Some(data) = field.try_next().await? {
                    buf += str::from_utf8(&data)?;
                    if buf.len() > 20 {
                        field.for_each(|_| ready(())).await;
                        return Err(UploadError::FieldTooBig("extension", 20).into());
                    }
                }
                let buf = buf.trim();
                if !buf.is_empty() {
                    extension = Some(buf.to_owned());
                }
            }
            name => {
                let name = name.to_string();
                field.for_each(|_| ready(())).await;
                multipart
                    .for_each(|field| async {
                        if let Ok(field) = field {
                            field.for_each(|_| ready(())).await;
                        }
                    })
                    .await;
                return Err(UploadError::InvalidField(name).into());
            }
        }
    }

    if file
        .contents()
        .await
        .map_err(ErrorInternalServerError)?
        .is_empty()
    {
        file.delete().await.map_err(ErrorInternalServerError)?;
        return Err(UploadError::NoData.into());
    }

    Ok(HttpResponse::Found()
        .append_header((
            header::LOCATION,
            file.name().to_string() + &extension.map(|e| format!(".{e}")).unwrap_or_default(),
        ))
        .finish())
}
