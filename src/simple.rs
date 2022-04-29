use std::str;

use actix_multipart::Multipart;
use actix_web::error::{ErrorInternalServerError, ErrorNotFound};
use actix_web::http::header;
use actix_web::web::Query;
use actix_web::{
    dev::HttpServiceFactory, get, http::StatusCode, post, web::Data, HttpResponse, Responder,
    ResponseError, Result,
};
use actix_web_codegen::routes;
use actix_web_lab::extract::Path;
use bonsaidb::local::AsyncDatabase;
use bonsaidb_files::FileConfig;
use bonsaidb_files::{BonsaiFiles, Truncate};
use futures::future::ready;
use futures::{StreamExt, TryStreamExt};
use mime_guess::mime::{APPLICATION_OCTET_STREAM, IMAGE, TEXT_HTML_UTF_8};
use mime_guess::Mime;
use rand::distributions::DistString;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use syntect::html::ClassedHTMLGenerator;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::util::ReadableAlphanumeric;

pub fn scope() -> impl HttpServiceFactory {
    (delete_entry, get_bin, get_ext, post_form, index)
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::NotFound()
        .content_type(TEXT_HTML_UTF_8)
        .body(format!(
            include_str!("page/index.html"),
            style = include_str!("page/style.css"),
        ))
}

#[serde_as]
#[derive(Deserialize)]
struct MimeQuery {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    mime: Option<Mime>,
}

#[get("{id}.bin")]
async fn get_bin(
    Path(id): Path<String>,
    database: Data<AsyncDatabase>,
    Query(MimeQuery { mime }): Query<MimeQuery>,
) -> Result<impl Responder> {
    if let Some(file) = BonsaiFiles::load_async(&id, database.as_ref().clone())
        .await
        .map_err(ErrorInternalServerError)?
    {
        Ok(HttpResponse::Ok()
            .content_type(mime.unwrap_or(APPLICATION_OCTET_STREAM))
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

#[routes]
#[get("{id}.{ext}")]
#[get("{id}")]
async fn get_ext(
    Path(Id { id, ext }): Path<Id>,
    database: Data<AsyncDatabase>,
    syntaxes: Data<SyntaxSet>,
) -> Result<impl Responder> {
    if let Some(file) = BonsaiFiles::load_async(&id, database.as_ref().clone())
        .await
        .map_err(ErrorInternalServerError)?
    {
        let mime = ext
            .as_ref()
            .and_then(|ext| mime_guess::from_ext(ext).first());
        let syntax = ext
            .as_ref()
            .and_then(|ext| syntaxes.find_syntax_by_token(ext));
        let file = file.contents().await.map_err(ErrorInternalServerError)?;

        match mime {
            Some(mime) if mime.type_() == IMAGE => Ok(HttpResponse::Ok()
                .content_type(TEXT_HTML_UTF_8)
                .body(format!(
                    include_str!("page/get.html"),
                    content = format_args!(
                        include_str!("page/image.html"),
                        image_url = format_args!("../{id}.bin"),
                    ),
                    style = include_str!("page/style.css"),
                    delete_url = format_args!("../{id}/delete"),
                    download_url = format_args!("../{id}.bin"),
                    download_name =
                        id.clone() + &ext.map(|ext| format!(".{ext}")).unwrap_or_default(),
                ))),
            _ if file.len() < 10_000 => {
                if let Ok(file) =
                    String::from_utf8(file.to_vec().await.map_err(ErrorInternalServerError)?)
                {
                    let code = if let Some(syntax) = syntax {
                        let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
                            syntax,
                            syntaxes.as_ref(),
                            syntect::html::ClassStyle::SpacedPrefixed { prefix: "code-" },
                        );

                        for line in LinesWithEndings::from(&file) {
                            html_generator.parse_html_for_line_which_includes_newline(line);
                        }
                        html_generator.finalize()
                    } else {
                        file
                    };
                    Ok(HttpResponse::Ok()
                        .content_type(TEXT_HTML_UTF_8)
                        .body(format!(
                            include_str!("page/get.html"),
                            content = format_args!(
                                include_str!("page/code.html"),
                                code = code,
                                one_half_light = include_str!("page/one_half_light.css"),
                                one_half_dark = include_str!("page/one_half_dark.css")
                            ),
                            style = include_str!("page/style.css"),
                            delete_url = &format!("../{id}/delete"),
                            download_url = &format!("../{id}.bin"),
                            download_name =
                                id.clone() + &ext.map(|ext| format!(".{ext}")).unwrap_or_default(),
                        )))
                } else {
                    Ok(HttpResponse::Ok()
                        .content_type(TEXT_HTML_UTF_8)
                        .body(format!(
                            include_str!("page/get.html"),
                            content = include_str!("page/wrong_type.html"),
                            style = include_str!("page/style.css"),
                            delete_url = &format!("../{id}/delete"),
                            download_url = &format!("../{id}.bin"),
                            download_name =
                                id.clone() + &ext.map(|ext| format!(".{ext}")).unwrap_or_default(),
                        )))
                }
            }
            _ => Ok(HttpResponse::Ok()
                .content_type(TEXT_HTML_UTF_8)
                .body(format!(
                    include_str!("page/get.html"),
                    content = include_str!("page/too_large.html"),
                    style = include_str!("page/style.css"),
                    delete_url = &format!("../{id}/delete"),
                    download_url = &format!("../{id}.bin"),
                    download_name =
                        id.clone() + &ext.map(|ext| format!(".{ext}")).unwrap_or_default(),
                ))),
        }
    } else {
        Ok(HttpResponse::NotFound()
            .content_type(TEXT_HTML_UTF_8)
            .body(format!(
                include_str!("page/404.html"),
                style = include_str!("page/style.css"),
            )))
    }
}

#[derive(Deserialize)]
struct Id {
    id: String,
    ext: Option<String>,
}

#[routes]
#[delete("{id}.{ext}")]
#[delete("{id}")]
#[get("{id}/delete")]
async fn delete_entry(
    Path(Id { id, .. }): Path<Id>,
    database: Data<AsyncDatabase>,
) -> Result<impl Responder> {
    if let Some(file) = BonsaiFiles::load_async(&id, database.as_ref().clone())
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
        let name = ReadableAlphanumeric.sample_string(&mut rand::thread_rng(), length);
        tries += 1;
        match BonsaiFiles::build(&name)
            .create_async(database.clone())
            .await
        {
            Ok(file) => break file,
            Err(bonsaidb_files::Error::Database(bonsaidb::core::Error::UniqueKeyViolation {
                ..
            })) if tries > 5 => {
                let file = BonsaiFiles::load_or_create_async(&name, true, database.clone())
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
