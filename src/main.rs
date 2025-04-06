mod water;
mod fileupload;

extern crate walkdir;
use std::{fs, io};
use actix_web::{App, Error, HttpRequest, HttpResponse, HttpServer};
use std::io::{Write};
use rand::{distr::Alphanumeric, Rng};
use std::string::ToString;
use std::sync::Mutex;
use actix_cors::Cors;
use tokio::task;
use lazy_static::lazy_static;
use crate::water::get_style;

const DEFAULT_RND_STR_LEN: usize = 15;
lazy_static! {
    static ref HOSTNAME: Mutex<String> = {
        let host = "localhost:8080".to_string();
        Mutex::new(host)
    };
}

#[actix_web::get("/r/style.css")]
async fn load_css(_req: HttpRequest) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body(get_style()?))
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

#[actix_web::main]
async fn main() -> io::Result<()> {
    mod_host();
    println!("Host: {}", get_hostname());
    fs::create_dir_all("sessions")?;
    task::spawn(fileupload::background_cleanup("sessions"));
    HttpServer::new(|| App::new()
        .wrap(Cors::permissive())
        .service(load_index)
    )
        .bind(get_hostname())?
        .run()
        .await
}