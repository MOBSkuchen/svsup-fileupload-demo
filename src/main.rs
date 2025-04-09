mod water;
mod fileupload;

extern crate walkdir;
use actix_web::http::StatusCode;
use actix_web::middleware::{ErrorHandlerResponse, ErrorHandlers};
use std::{fs, io};
use std::path::{Path, PathBuf};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpResponseBuilder, HttpServer};
use rand::{distr::Alphanumeric, Rng};
use std::string::ToString;
use std::sync::Mutex;
use actix_cors::Cors;
use actix_files::NamedFile;
use actix_web::dev::ServiceResponse;
use actix_web::http::header::ContentType;
use tokio::task;
use lazy_static::lazy_static;
use crate::fileupload::{delete, download_file, download_zip, fup_ld_index, get_info, is_entry_owner, load_sesh, upload};
use crate::water::{get_article, get_articles, get_index, get_style, load_err_html};

macro_rules! error_handler_many {
    ($handler:ident, [$($variant:ident),*]) => {
        ErrorHandlers::new()
            $(.handler(StatusCode::$variant, $handler))+
    }
}

const DEFAULT_RND_STR_LEN: usize = 15;
lazy_static! {
    static ref HOSTNAME: Mutex<String> = {
        let host = "localhost:8080".to_string();
        Mutex::new(host)
    };
}

async fn load_css(_req: HttpRequest) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body(get_style()?))
}

#[actix_web::get("/")]
async fn load_index(_req: HttpRequest) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body(get_index()?))
}

async fn load_article(_req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, Error> {
    let article = get_article(path.into_inner())?;
    if article.is_none() {
        Err(actix_web::error::ErrorNotFound("Resource not found"))
    } else {
        Ok(HttpResponse::Ok().body(article.unwrap()))
    }
}

async fn load_articles(_req: HttpRequest) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().content_type(ContentType::html()).body(get_articles()?))
}

async fn load_resource(path: web::Path<String>) -> Result<NamedFile, Error> {
    let path: PathBuf = Path::new("resources").join(path.into_inner());
    if path.exists() {
        Ok(NamedFile::open(path)?)
    } else {
        Err(actix_web::error::ErrorNotFound("Resource not found"))
    }
}

fn random_str(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

fn get_hostname() -> String {
    HOSTNAME.lock().unwrap().to_string()
}

fn get_domain() -> String {
    let hst = HOSTNAME.lock().unwrap();
    hst.split_once(':').unwrap_or((hst.as_str(), "")).0.to_string()
}

fn mod_host() {
    let mut mut_host = HOSTNAME.lock().unwrap();
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        *mut_host = args[1].to_string();
    } else if fs::exists(".host").expect("FAILED TO FIND OUT IF .host EXISTS") {
        *mut_host = fs::read_to_string(".host").expect("FAILED TO READ .host")
    }
}

#[allow(clippy::missing_errors_doc)]
pub fn render_error<B>(res: ServiceResponse<B>) -> Result<ErrorHandlerResponse<B>, Error> {
    let status = res.status();
    let request = res.into_parts().0;

    let new_response =
        HttpResponseBuilder::new(status)
            .insert_header(ContentType::html())
            .body(load_err_html(status.as_u16())?);

    Ok(ErrorHandlerResponse::Response(
        ServiceResponse::new(request, new_response).map_into_right_body(),
    ))
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    mod_host();
    println!("Host: {}", get_hostname());
    fs::create_dir_all("sessions")?;
    task::spawn(fileupload::background_cleanup("sessions"));
    fs::create_dir_all("articles")?;
    fs::create_dir_all("resources")?;
    HttpServer::new(|| App::new()
        .wrap(Cors::permissive())
        .wrap(error_handler_many!(render_error, [BAD_REQUEST, UNAUTHORIZED, FORBIDDEN,
            NOT_FOUND, METHOD_NOT_ALLOWED, NOT_ACCEPTABLE, REQUEST_TIMEOUT, GONE,
            LENGTH_REQUIRED, PAYLOAD_TOO_LARGE, URI_TOO_LONG, UNSUPPORTED_MEDIA_TYPE,
            RANGE_NOT_SATISFIABLE, IM_A_TEAPOT, TOO_MANY_REQUESTS,
            REQUEST_HEADER_FIELDS_TOO_LARGE, MISDIRECTED_REQUEST, UPGRADE_REQUIRED,
            INTERNAL_SERVER_ERROR, NOT_IMPLEMENTED, SERVICE_UNAVAILABLE,
            HTTP_VERSION_NOT_SUPPORTED]))
        .service(load_index)
        .route("/r/style.css", web::get().to(load_css))
        .route("/r/{resource}", web::get().to(load_resource))
        .route("/a/{articles}", web::get().to(load_article))
        .route("/articles", web::get().to(load_articles))
        .route("/f/get-info", web::get().to(get_info))
        .route("/f/upload", web::post().to(upload))
        .route("/f/is-owner", web::post().to(is_entry_owner))
        .route("/f/delete/{session}", web::post().to(delete))
        .route("/f/download/{session}", web::get().to(download_zip))
        .route("/f/download/{session}/{filename}", web::get().to(download_file))
        .route("/f/session/{session}", web::get().to(load_sesh))
        .route("/f/index", web::get().to(fup_ld_index))
    )
        .bind(get_hostname())?
        .run()
        .await
}