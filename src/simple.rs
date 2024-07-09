use std::{borrow::Borrow, fmt::Display, str};

use actix_multipart::Multipart;
use actix_web::{
    cookie::{Cookie, CookieJar},
    dev::HttpServiceFactory,
    error::ErrorInternalServerError,
    get,
    guard::GuardContext,
    http::{
        header::{self, ContentDisposition, DispositionParam, DispositionType},
        StatusCode,
    },
    post, routes,
    web::{Bytes, Data, Payload},
    HttpResponse, Responder, ResponseError, Result,
};
use actix_web_lab::extract::Path;
use askama::Template;
use askama_actix::TemplateToResponse;
use chrono::{Duration, Utc};
use futures::{future::ready, StreamExt, TryStreamExt};
use futures_util::Stream;
use mime_guess::mime::{self, APPLICATION_OCTET_STREAM, IMAGE};
use rand::distributions::{Alphanumeric, DistString};
use serde::Deserialize;
use syntect::{html::ClassedHTMLGenerator, parsing::SyntaxSet, util::LinesWithEndings};

use crate::{
    config::Config,
    db::{DateTime, DB},
    util::{AddCookieJar, Cookies},
};

const OWNER_COOKIE: &str = "OWNER";

#[derive(Template)]
#[template(path = "404.html")]
struct NotFound;

pub fn scope() -> impl HttpServiceFactory {
    (
        delete_entry,
        raw,
        download,
        get_ext,
        post_form,
        post_raw,
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
async fn download(Path(file_name): Path<FileName>, db: Data<DB>) -> Result<impl Responder> {
    if let Some(file) = db
        .load_file(&file_name.id)
        .await
        .map_err(ErrorInternalServerError)?
    {
        Ok(HttpResponse::Ok()
            .content_type(
                file_name
                    .ext
                    .as_deref()
                    .map(|ext| mime_guess::from_ext(ext).first_or_octet_stream())
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
            )
            .customize())
    } else {
        Ok(NotFound
            .to_response()
            .customize()
            .with_status(StatusCode::NOT_FOUND))
    }
}

#[routes]
#[get("raw/{id}.{ext}")]
#[get("raw/{id}")]
#[get("{id}.{ext}", guard = "no_browser")]
#[get("{id}", guard = "no_browser")]
async fn raw(
    Path(FileName { id, ext }): Path<FileName>,
    database: Data<DB>,
) -> Result<impl Responder> {
    if let Some(file) = database
        .load_file(&id)
        .await
        .map_err(ErrorInternalServerError)?
    {
        Ok(HttpResponse::Ok()
            .content_type(
                ext.as_deref()
                    .map(|ext| mime_guess::from_ext(ext).first_or_octet_stream())
                    .unwrap_or(APPLICATION_OCTET_STREAM),
            )
            .streaming(
                file.contents()
                    .await
                    .map_err(ErrorInternalServerError)?
                    .map_ok(From::from),
            )
            .customize())
    } else {
        Ok(NotFound
            .to_response()
            .customize()
            .with_status(StatusCode::NOT_FOUND))
    }
}

fn no_browser(it: &GuardContext) -> bool {
    const MOZILLA: &[u8] = b"Mozilla";
    !it.head()
        .headers
        .get(header::USER_AGENT)
        .map_or(false, |it| {
            let it = it.as_bytes();
            for i in 0..it.len() - MOZILLA.len() {
                if &it[i..(i + MOZILLA.len())] == MOZILLA {
                    return true;
                }
            }
            false
        })
}

#[routes]
#[get("{id}.{ext}")]
#[get("{id}")]
async fn get_ext(
    Path(file_name): Path<FileName>,
    database: Data<DB>,
    syntaxes: Data<SyntaxSet>,
    Cookies(cookies): Cookies,
) -> Result<impl Responder> {
    Ok(
        if let Some(file) = database
            .load_file(&file_name.id)
            .await
            .map_err(ErrorInternalServerError)?
        {
            let metadata = file.metadata();
            let delete_at = metadata.delete_at;
            let owner = cookies
                .iter()
                .any(|cookie| cookie.name_value() == (OWNER_COOKIE, &metadata.owner));
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
                Some(mime) if mime.type_() == IMAGE => {
                    #[derive(Template)]
                    #[template(path = "image.html")]
                    struct Image {
                        file_name: FileName,
                        delete_at: Option<DateTime>,
                        owner: bool,
                        copy: Option<String>,
                    }

                    Image {
                        file_name,
                        delete_at,
                        owner,
                        copy: None,
                    }
                    .to_response()
                }
                _ if file.len() < 50_000 => {
                    if let Ok(file) =
                        String::from_utf8(file.to_vec().await.map_err(ErrorInternalServerError)?)
                    {
                        #[derive(Template)]
                        #[template(path = "code.html")]
                        struct UnHighlighted {
                            code: String,
                            file_name: FileName,
                            delete_at: Option<DateTime>,
                            owner: bool,
                            copy: Option<String>,
                        }

                        if let Some(syntax) = syntax {
                            let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
                                syntax,
                                syntaxes.as_ref(),
                                syntect::html::ClassStyle::SpacedPrefixed { prefix: "code-" },
                            );

                            for line in LinesWithEndings::from(&file) {
                                // Necessary for the used toml syntax
                                let line = &line.replace('\r', "");
                                if html_generator
                                    .parse_html_for_line_which_includes_newline(line)
                                    .is_err()
                                {
                                    return Ok(UnHighlighted {
                                        code: file.clone(),
                                        file_name,
                                        delete_at,
                                        owner,
                                        copy: Some(file),
                                    }
                                    .to_response()
                                    .customize());
                                }
                            }

                            #[derive(Template)]
                            #[template(path = "code.html", escape = "none")]
                            struct Highlighted {
                                code: String,
                                file_name: FileName,
                                delete_at: Option<DateTime>,
                                owner: bool,
                                copy: Option<String>,
                            }

                            Highlighted {
                                code: html_generator.finalize(),
                                file_name,
                                delete_at,
                                owner,
                                copy: Some(file),
                            }
                            .to_response()
                        } else {
                            UnHighlighted {
                                code: file.clone(),
                                file_name,
                                delete_at,
                                owner,
                                copy: Some(file),
                            }
                            .to_response()
                        }
                    } else {
                        #[derive(Template)]
                        #[template(path = "wrong_type.html")]
                        struct WrongType {
                            file_name: FileName,
                            delete_at: Option<DateTime>,
                            owner: bool,
                            copy: Option<String>,
                        }

                        WrongType {
                            file_name,
                            delete_at,
                            owner,
                            copy: None,
                        }
                        .to_response()
                    }
                }
                _ => {
                    #[derive(Template)]
                    #[template(path = "too_large.html")]
                    struct TooLarge {
                        file_name: FileName,
                        delete_at: Option<DateTime>,
                        owner: bool,
                        copy: Option<String>,
                    }

                    TooLarge {
                        file_name,
                        delete_at,
                        owner,
                        copy: None,
                    }
                    .to_response()
                }
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
#[get("delete/{id}.{ext}")]
#[get("delete/{id}")]
async fn delete_entry(
    Path(FileName { id, .. }): Path<FileName>,
    config: Data<Config>,
    database: Data<DB>,
    Cookies(cookies): Cookies,
) -> Result<impl Responder> {
    let owner = database
        .file_owner(&id)
        .await
        .map_err(ErrorInternalServerError)?;

    if matches!(owner, Some(owner) if
        cookies
            .iter()
            .any(|cookie| cookie.name_value() == (OWNER_COOKIE, &owner))
    ) {
        database
            .delete_at(&id, Utc::now())
            .await
            .map_err(ErrorInternalServerError)?
    } else {
        database
            .delete_at(&id, Utc::now() + config.time_to_delete)
            .await
            .map_err(ErrorInternalServerError)?;
    }

    Ok(HttpResponse::SeeOther()
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

async fn create_file<E: ResponseError + 'static>(
    mut data: impl Stream<Item = Result<Bytes, E>> + Unpin,
    database: &Data<DB>,
    cookies: &mut CookieJar,
    config: &Data<Config>,
) -> Result<String> {
    let owner = if let Some(owner) = cookies.get(OWNER_COOKIE) {
        owner.value().to_owned()
    } else {
        let owner = Alphanumeric.sample_string(&mut rand::thread_rng(), 20);
        cookies.add(Cookie::new(OWNER_COOKIE, owner.clone()));
        owner
    };

    let file = database
        .new_file(owner, Some(config.max_age))
        .await
        .map_err(ErrorInternalServerError)?;

    const FILE_LIMIT: usize = 10_000_000;
    let mut limit: usize = FILE_LIMIT;
    while let Some(data) = data.try_next().await? {
        if let Some(l) = limit.checked_sub(data.len()) {
            limit = l;
        } else {
            return Err(UploadError::FieldTooBig("file", FILE_LIMIT).into());
        }
        file.append(&data).await.map_err(ErrorInternalServerError)?;
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

    Ok(file.name().to_string())
}

fn response(
    name: String,
    cookies: CookieJar,
    extension: Option<String>,
    config: &Data<Config>,
) -> impl Responder {
    let name = name + &extension.map(|e| format!(".{e}")).unwrap_or_default();
    HttpResponse::SeeOther()
        .append_header((header::LOCATION, name.clone()))
        .cookie_delta(&cookies)
        .body(format!(
            "{}{name}\n",
            if !config.base_url.is_empty() && !config.base_url.ends_with('/') {
                format!("{}{}", config.base_url, "/")
            } else {
                config.base_url.to_string()
            }
        ))
}

#[post("/")]
async fn post_raw(
    payload: Payload,
    database: Data<DB>,
    Cookies(mut cookies): Cookies,
    config: Data<Config>,
) -> Result<impl Responder> {
    create_file(payload, &database, &mut cookies, &config)
        .await
        .map(|it| response(it, cookies, None, &config))
}

fn is_form(it: &GuardContext) -> bool {
    it.header::<header::ContentType>().map_or(false, |it| {
        it.0.type_() == mime::MULTIPART && it.0.subtype() == mime::FORM_DATA
    })
}

#[post("/", guard = "is_form")]
async fn post_form(
    payload: Multipart,
    database: Data<DB>,
    Cookies(mut cookies): Cookies,
    config: Data<Config>,
) -> Result<impl Responder> {
    let mut multipart = payload;
    let mut extension = None;
    let mut file = None;

    while let Some(mut field) = multipart.try_next().await? {
        match field.name().unwrap_or_default() {
            "data" => {
                if let Some(file_name) = field
                    .content_disposition()
                    .and_then(ContentDisposition::get_filename)
                {
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
                file = Some(create_file(field, &database, &mut cookies, &config).await?);
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
                return Err(UploadError::InvalidField(name).into());
            }
        }
    }

    Ok(response(
        file.ok_or(UploadError::NoData)?,
        cookies,
        extension,
        &config,
    ))
}
