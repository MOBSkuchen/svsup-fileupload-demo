mod water;
extern crate walkdir;
use std::{fs, io};
use std::env::temp_dir;
use std::fs::{File};
use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_web::{cookie, web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder};
use futures_util::{StreamExt};
use std::io::{Read, Seek, Write};
use std::path::Path;
use actix_web::cookie::{Cookie};
use actix_web::http::header::{HeaderValue};
use rand::{distr::Alphanumeric, Rng};
use zip::result::ZipError;
use zip::write::{ExtendedFileOptions, FileOptions};
use walkdir::{WalkDir, DirEntry};
use std::{time::Duration};
use std::fs::OpenOptions;
use std::string::ToString;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use actix_cors::Cors;
use actix_web::web::Redirect;
use tokio::task;
use tokio::time::sleep;
use lazy_static::lazy_static;
use crate::water::{get_index, get_style, load_all};

const DEFAULT_RND_STR_LEN: usize = 15;
lazy_static! {
    static ref HOSTNAME: Mutex<String> = {
        let host = "localhost:8080".to_string();
        Mutex::new(host)
    };
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

const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;
const MAX_FILES: usize = 10;

fn delete_directory_contents<P: AsRef<Path>>(dir: P) -> io::Result<()> {
    if dir.as_ref().is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

fn cleanup(session_id: &String) -> io::Result<()> {
    delete_directory_contents(format!("sessions/{session_id}"))?;
    fs::remove_dir(format!("sessions/{session_id}"))
}

fn get_expiration_time<P: AsRef<Path> + std::fmt::Debug>(path: P) -> io::Result<Option<u64>> {
    let expiration_file = path.as_ref().join(".expiration");
    if !expiration_file.exists() {
        return Ok(None);
    }

    let mut file = OpenOptions::new().read(true).open(&expiration_file)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Parse as f64 first, then convert to u64 (by truncating)
    if let Ok(expiration_float) = contents.trim().parse::<f64>() {
        // Truncate the float to get the integer value (seconds)
        let expiration_time = expiration_float.trunc() as u64;
        return Ok(Some(expiration_time));
    }

    Ok(None)  // If the parsing fails, return None
}

async fn wait_for_handles_to_close<P: AsRef<Path>>(path: P) {
    while fs::remove_dir_all(&path).is_err() {
        sleep(Duration::from_secs(1)).await;
    }
}

async fn cleanup_expired_folders<P: AsRef<Path>>(folder: P) -> io::Result<()> {
    for entry in fs::read_dir(&folder)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let t = get_expiration_time(&path);
            if let Some(expiration_time) = t? {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                if now >= expiration_time {
                    println!("Deleting expired folder: {:?}", path);
                    wait_for_handles_to_close(&path).await;
                }
            }
        }
    }
    Ok(())
}

async fn background_cleanup(folder_path: &'static str) {
    loop {
        if let Err(e) = cleanup_expired_folders(folder_path).await { eprintln!("Error cleaning up folders: {}", e); }
        sleep(Duration::from_secs(2)).await; // Run every 60 seconds
    }
}

async fn session(req: HttpRequest, mut payload: Multipart) -> Result<HttpResponse, Error> {
    let session_id = random_str(DEFAULT_RND_STR_LEN);
    fs::create_dir(format!("sessions/{session_id}"))?;
    let expiration_opt = req.headers().get("expiration");
    if expiration_opt.is_none() {
        cleanup(&session_id).expect("Failed to remove stuff");
        return Ok(HttpResponse::BadRequest().body("Key not found, expiration"))
    }
    let expiration = expiration_opt.unwrap().to_str().unwrap().to_string().parse::<u64>();
    if expiration.is_err() {
        return Ok(HttpResponse::BadRequest().body("Key is not a u64, expiration"))
    }
    let access_token = random_str(DEFAULT_RND_STR_LEN);

    let mut file_count = 0;
    while let Some(field) = payload.next().await {
        let mut field = field?;
        let content_disposition = field.content_disposition();
        let filename = content_disposition.unwrap().get_filename().unwrap_or("default.bin");
        if filename == ".token" || filename == ".expiration" {
            return Ok(HttpResponse::BadRequest().body("Got filename with reserved name (.token or .expiration)"))
        }
        let filepath = Path::new(format!("sessions/{session_id}").as_str()).join(filename);

        let mut file = File::create(filepath)?;
        let mut total_size = 0;
        while let Some(chunk) = field.next().await {
            let data = chunk?;
            total_size += data.len();
            if total_size > MAX_FILE_SIZE {
                cleanup(&session_id).expect("Failed to remove stuff");
                return Ok(HttpResponse::BadRequest().body("File too large"));
            }
            file.write_all(&data)?;
        }

        file_count += 1;
        if file_count > MAX_FILES {
            cleanup(&session_id).expect("Failed to remove stuff");
            return Ok(HttpResponse::BadRequest().body("Too many files"));
        }
    }

    let expiration = expiration.unwrap();

    let mut cookie = Cookie::new(&session_id, &access_token);
    cookie.set_max_age(Some(cookie::time::Duration::seconds(expiration as i64)));
    cookie.set_domain(get_domain());

    fs::write(format!("sessions/{session_id}/.token"), &access_token)?;
    fs::write(format!("sessions/{session_id}/.expiration"), (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + expiration).to_string())?;

    Ok(
        HttpResponse::Ok()
            .append_header(("session".to_string(), session_id.clone()))
            .append_header(("token".to_string(), access_token.clone()))
            .cookie(cookie)
            .body("OK")
    )
}

async fn delete(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, Error> {
    let session_id = path.into_inner();
    let token_opt = req.headers().get("token");
    let mut remove = Cookie::new(&session_id, "none");
    remove.set_domain(get_domain());
    remove.set_path("/");
    remove.make_removal();
    if token_opt.is_none() {
        return Ok(HttpResponse::BadRequest().body("Key not found, token"))
    }
    let token = token_opt.unwrap().to_str().unwrap().to_string();
    if fs::read_to_string(format!("sessions/{session_id}/.token")).is_ok_and(|t| {token == t}) {
        cleanup(&session_id).expect("Failed to remove entry");
        Ok(HttpResponse::Ok().cookie(remove).body("Removed successfully"))
    }
    else {
        Ok(HttpResponse::Forbidden().body("Invalid auth token or non existent session"))
    }
}

async fn is_entry_owner(req: HttpRequest) -> Result<HttpResponse, Error> {
    let session_opt = req.headers().get("session");
    if session_opt.is_none() {
        return Ok(HttpResponse::BadRequest().body("Key not found, session"))
    }
    let session_id = session_opt.unwrap().to_str().unwrap().to_string();
    let token_opt = req.headers().get("token");
    if token_opt.is_none() {
        return Ok(HttpResponse::BadRequest().body("Key not found, token"))
    }
    let token = token_opt.unwrap().to_str().unwrap().to_string();
    if fs::read_to_string(format!("sessions/{session_id}/.token")).is_ok_and(|t| {token == t}) {
        Ok(HttpResponse::Ok().body("You are the owner / creator"))
    }
    else {
        Ok(HttpResponse::Forbidden().body("Invalid auth token or non existent session"))
    }
}

pub fn list_files_with_sizes<P: AsRef<Path>>(dir: P) -> io::Result<Vec<(String, u64)>> {
    let mut files = Vec::new();
    if dir.as_ref().is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name != ".token" && file_name != ".expiration" {
                        let metadata = fs::metadata(&path)?;
                        files.push((file_name.to_string(), metadata.len()));
                    }
                }
            }
        }
    }
    Ok(files)
}

fn zip_dir<T>(it: &mut dyn Iterator<Item=DirEntry>, prefix: &str, writer: T)
              -> zip::result::ZipResult<()>
where T: Write+Seek
{
    let mut zip = zip::ZipWriter::new(writer);
    let options: FileOptions<ExtendedFileOptions> = FileOptions::default();
    
    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let name = path.strip_prefix(Path::new(prefix)).unwrap();
        if path.is_file() {
            zip.start_file_from_path(name, options.clone())?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            zip.add_directory_from_path(name, options.clone())?;
        }
    }
    zip.finish()?;
    Ok(())
}

fn doit(src_dir: &str, dst_file: &str) -> zip::result::ZipResult<()> {
    if !Path::new(src_dir).is_dir() {
        return Err(ZipError::FileNotFound);
    }

    let path = Path::new(dst_file);
    let file = File::create(path).expect("Failed to create zip file");

    let walkdir = WalkDir::new(src_dir);
    let it = walkdir.into_iter().filter_entry(|entry: &DirEntry| {
        let filename = entry.file_name().to_string_lossy();
        filename != ".token" && filename != ".expiration"
    });

    zip_dir(&mut it.filter_map(|e| e.ok()), src_dir, file)?;

    Ok(())
}

async fn get_info(req: HttpRequest) -> Result<HttpResponse, Error> {
    let session_opt = req.headers().get("session");
    if session_opt.is_none() {
        return Ok(HttpResponse::BadRequest().body("Key not found, session"))
    }
    let session_id = session_opt.unwrap().to_str().unwrap().to_string();
    if !fs::exists(format!("sessions/{session_id}"))? {
        return Ok(HttpResponse::NotFound().body("Non existent session"))
    }
    let token_opt = req.headers().get("token");
    let is_owner = token_opt.is_some() && fs::read_to_string(format!("sessions/{session_id}/.token")).is_ok_and(|t| {token_opt.unwrap().to_str().unwrap() == t});
    let exp = fs::read_to_string(format!("sessions/{session_id}/.expiration"))?;
    let files_and_sizes: Vec<String> = list_files_with_sizes(format!("sessions/{session_id}"))?.iter()
        .map(|x| {format!("{} {}", x.1, x.0)}).collect();
    let mut resp = HttpResponse::Ok().body(files_and_sizes.join("\n"));
    let headers = resp.headers_mut();
    headers.insert("expiration".parse().unwrap(), HeaderValue::from_str(&exp.to_string())?);
    headers.insert("owner".parse().unwrap(), HeaderValue::from_str(&is_owner.to_string())?);
    Ok(resp)
}

async fn download_file(req: HttpRequest, path: web::Path<(String, String)>) -> Result<HttpResponse, Error> {
    let (session_id, filename) = path.into_inner();
    let path = format!("sessions/{session_id}/{filename}");
    if !fs::exists(&path)? {
        return Ok(HttpResponse::NotFound().body("Non existent session or file within session"))
    }
    match NamedFile::open(&path) {
        Ok(named_file) => {
            let response = named_file.prefer_utf8(true).use_last_modified(true).into_response(&req);
            Ok(HttpResponse::Ok()
                .append_header(("Content-Disposition", format!("attachment; filename=\"{}\"", filename)))
                .body(response.into_body()))
        }
        Err(_) => Ok(HttpResponse::NotFound().body("File not found")),
    }
}

async fn download_zip(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, Error> {
    let session_id = path.into_inner();
    let path = format!("sessions/{session_id}");
    if !fs::exists(&path)? {
        return Ok(HttpResponse::NotFound().body("Non existent session or file within session"))
    }
    let filename = temp_dir().join(random_str(50)).to_str().unwrap().to_string();
    doit(&path, &filename).expect("Failed to save zip");
    match NamedFile::open(&filename) {
        Ok(named_file) => {
            let mut response = named_file.use_last_modified(false).prefer_utf8(true).into_response(&req);
            let headers = response.headers_mut();
            headers.insert(
                "Content-Disposition".parse().unwrap(),
                format!("attachment; filename=\"{}.zip\"", session_id).parse()?,
            );

            Ok(response)
        }
        Err(_) => { 
            Ok(HttpResponse::NotFound().body("File not found")) },
    }
}

#[actix_web::get("/")]
async fn load_index(req: HttpRequest) -> Result<HttpResponse, Error> {
    for cookie in req.cookies().unwrap().iter() {
        if fs::exists(format!("sessions/{}", cookie.name()))? {
            return Ok(Redirect::to(format!("/session/{}", cookie.name())).respond_to(&req).map_into_boxed_body())
        }
    }
    Ok(HttpResponse::Ok().body(get_index()?))
}

async fn load_sesh(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, Error> {
    let session_id = path.into_inner();
    let exists = fs::exists(format!("sessions/{session_id}"))?;
    let cookie_x = req.cookie(&session_id);

    // Doesn't work, not reaaalllyyy a point in updating this
    // if !exists && cookie_x.is_some() {
    //     let mut response = HttpResponse::Ok().finish();
    //     response.add_removal_cookie(&Cookie::named(session_id).)?;
    //     return Ok(response);
    // }
    
    if !exists {
        return Ok(HttpResponse::NotFound().finish())
    }
    
    let owner = if cookie_x.is_some() {
        let token = cookie_x.unwrap().value().to_string();
        if fs::read_to_string(format!("sessions/{session_id}/.token"))? == token {
            Some(token)
        } else {
            None
        }
    } else {None};

    let files = list_files_with_sizes(format!("sessions/{session_id}"))?;
    let expiration = get_expiration_time(format!("sessions/{session_id}"))?;
    let html = load_all(session_id, expiration.expect("No expiration!!"), files, owner);
    
    Ok(HttpResponse::Ok().body(html))
}

async fn load_css(_req: HttpRequest) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body(get_style()?))
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
    task::spawn(background_cleanup("sessions"));
    HttpServer::new(|| App::new()
        .wrap(Cors::permissive())
        .service(load_index)
        .route("/upload", web::post().to(session))
        .route("/is-owner", web::get().to(is_entry_owner))
        .route("/delete/{session}", web::post().to(delete))
        .route("/get-info", web::get().to(get_info))
        .route("/style.css", web::get().to(load_css))
        .route("/download/{session}/{filename}", web::get().to(download_file))
        .route("/download/{session}", web::get().to(download_zip))
        .route("/session/{session}", web::get().to(load_sesh))
    )
        .bind(get_hostname())?
        .run()
        .await
}