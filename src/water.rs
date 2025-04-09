use std::fmt::format;
use std::fs;
use std::fs::FileType;
use chrono::{DateTime, Local, TimeZone, Utc};
use futures_util::io;
use crate::{get_hostname, random_str};

const STYLE_FILE: &str = include_str!("../style.css");
const FILE_UPLOAD_INDEX: &str = include_str!("../fup-index.html");
const INDEX: &str = include_str!("../index.html");
const FUP_SESSION: &str = include_str!("../fup-session.html");
const ERROR_TEMPLATE: &str = include_str!("../error.html");

fn format_file_size(size: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < units.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, units[unit_index])
}

fn format_utc_time(timestamp: u64) -> String {
    let datetime: DateTime<Utc> = Utc.timestamp_opt(timestamp as i64, 0).unwrap();
    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

pub fn load_all(sid: String, expiration: u64, files: Vec<(String, u64)>, token: Option<String>) -> String {
    let file_items = files
        .into_iter()
        .map(|(filename, filesize)| {
            let str = random_str(12);
            format!(
                "<div class=\"file-item\">
                    <span id=\"{id}\" class=\"file-info\">{filename}</span>
                    <h3 class=\"file-size\">{size}</h3>
                    <button class=\"download-btn\" onclick=\"downloadFile('{filename}', '{id}')\">Download</button>
                 </div>",
                id = str,
                filename = filename,
                size = format_file_size(filesize)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let html_template = include_str!("../fup-session.html");

    html_template
        .replace("{{sid}}", &sid)
        .replace("{{expires}}", &format_utc_time(expiration))
        .replace("{{file_items}}", &file_items)
        .replace("{{hostname}}", &get_hostname())
        .replace(
            "{{delete_button}}",
            &if token.is_some() {
                "<button class=\"delete-all-btn\" onclick=\"deleteAll()\">Delete All</button>".to_string()
            } else {
                "".to_string()
            },
        )
        .replace("{{token}}", &token.unwrap_or("YOU ARE NOT THE OWNER".to_string()))
}

pub fn get_style() -> io::Result<String> {
    if fs::exists("style.css")? {
        fs::read_to_string("style.css")
    } else {
        Ok(STYLE_FILE.to_string())
    }
}

pub fn get_fileupload_index() -> io::Result<String> {
    if fs::exists("fup-index.html")? {
        fs::read_to_string("fup-index.html")
    } else {
        Ok(FILE_UPLOAD_INDEX.to_string())
    }
}

pub fn get_index() -> io::Result<String> {
    let content = if fs::exists("index.html")? {
        fs::read_to_string("index.html")?
    } else {
        INDEX.to_string()
    };
    
    let mut boxes = vec![];
    let articles = get_articles2()?;
    for article in articles {
        let title = article.0;
        boxes.push(format!("<div class=\"article-box\" onclick=\"window.location = '/a/{title}'\">
      <div class=\"article-title\">{title}</div>
      <div class=\"article-meta\">by MOBSkuchen — {}</div>
    </div>", article.1))
    }

    Ok(content.replace("{{articles}}", &boxes.join("\n")))
}

pub fn get_article(article: String) -> io::Result<Option<String>> {
    let path = format!("articles/{article}.html");
    if fs::exists(&path)? {
        Ok(Some(fs::read_to_string(path)?))
    } else {
        Ok(None)
    }
}

pub fn get_articles() -> io::Result<String> {
    let mut files = vec![];
    for rd in fs::read_dir("articles")? {
        let entry = rd?;
        if FileType::is_file(&entry.file_type()?) {
            files.push(entry.file_name().to_str().unwrap().to_string())
        }
    }
    Ok(files.join("\n"))
}

fn get_articles2() -> io::Result<Vec<(String, String)>> {
    let mut files = vec![];

    for rd in fs::read_dir("articles")? {
        let entry = rd?;
        if FileType::is_file(&entry.file_type()?) {
            let metadata = entry.metadata()?;
            
            let created_time = metadata.created().or_else(|_| metadata.modified())?;
            let datetime: DateTime<Local> = created_time.into();
            let german_date = datetime.format("%d.%m.%Y").to_string();

            let filename = entry.file_name().to_str().unwrap().trim_end_matches(".html").to_string();
            files.push((filename, german_date));
        }
    }

    Ok(files)
}

pub fn load_err_html(status: u16) -> io::Result<String> {
    assert_ne!(status, 200);
    
    let content = if fs::exists("error.html")? {
        fs::read_to_string("error.html")?
    } else {
        ERROR_TEMPLATE.to_string()
    };
    Ok(content.replace("{{errid}}", &*status.to_string()))
}