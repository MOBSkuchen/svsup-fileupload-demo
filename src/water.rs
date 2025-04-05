use std::fs;
use chrono::{DateTime, TimeZone, Utc};
use futures_util::io;
use crate::{get_hostname, random_str};

const STYLE_FILE: &str = include_str!("../style.css");
const INDEX_FILE: &str = include_str!("../index.html");

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

fn load_file(filename: String, filesize: u64) -> String {
    let str = random_str(12);
    format!("<div class=\"file-item\">\n<span id=\"{str}\" class=\"file-info\">{filename}</span>\n<h3 class=\"file-size\">{}</h3>\n<button class=\"download-btn\" onclick=\"downloadFile('{filename}', '{str}')\">Download</button>\n</div>",
            format_file_size(filesize))
}

fn load_top(sid: &String, expiration: u64) -> String {
    format!("<h1 id=\"id\" onClick=\"copyLink()\">{sid}</h1>\n<h2>Expires at: {}</h2>", format_utc_time(expiration))
}

fn load_bottom(is_owner: bool) -> String {
    let mut one = "<button class=\"download-all-btn\" onclick=\"downloadAll()\">Download All</button>".to_string();
    if is_owner {
        one += "<button class=\"delete-all-btn\" onclick=\"deleteAll()\">Delete All</button>";
    }
    one
}

fn load_head(sid: &String) -> String {
    format!("<head>\n<meta charset=\"UTF-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n<title>Viewing: {sid}</title>\n<link rel=\"stylesheet\" href=\"/style.css\">\n</head>")
}

fn load_list(files: Vec<(String, u64)>) -> String {
    let mut one = "<div class=\"file-list\">".to_string();
    for file in files {
        one += load_file(file.0, file.1).as_str();
    }
    one += "</div>";
    one
}

fn load_script(sid: String, token: Option<String>) -> String {
    format!("
<script>
    const sessionId = '{sid}';

    function downloadFile(filename, _id_) {{
        window.location.href = `/download/${{sessionId}}/${{filename}}`;
        const element = document.getElementById(_id_);
        element.style.color = 'gold';
            setTimeout(() => {{
                element.style.color = 'white';
            }}, 1000);
    }}

    function copyLink() {{
        navigator.clipboard.writeText(`{}/session/${{sessionId}}`);
        const element = document.getElementById(\"id\");
        element.style.color = 'pink';
            setTimeout(() => {{
                element.style.color = 'rgb(129, 129, 129)';
            }}, 1000);
    }}

    function downloadAll() {{
        window.location.href = `/download/${{sessionId}}`;
        const element = document.getElementById(\"id\");
        element.style.color = 'green';
            setTimeout(() => {{
                element.style.color = 'rgb(129, 129, 129)';
            }}, 1000);
    }}

    function deleteAll() {{
        fetch(`/delete/${{sessionId}}`, {{ method: 'POST', credentials: \"same-origin\", headers: {{\"token\": '{}'}} }})
            .then(data => {{
                window.location = \"/\";
            }})
            .catch(error => console.error('Error:', error));
    }}
</script>", get_hostname(), token.unwrap_or("YOU ARE NOT THE OWNER".to_string()))
}

pub fn load_all(sid: String, expiration: u64, files: Vec<(String, u64)>, token: Option<String>) -> String {
    let mut one = "<!DOCTYPE html>\n<html lang=\"en\">".to_string();
    one += load_head(&sid).as_str();
    one += "<body>";
    one += load_top(&sid, expiration).as_str();
    one += load_list(files).as_str();
    one += load_bottom(token.is_some()).as_str();
    one += "</body>\n</html>";
    one += load_script(sid, token).as_str();
    one
}

pub fn get_style() -> io::Result<String> {
    if fs::exists("style.css")? {
        fs::read_to_string("style.css")
    } else {
        Ok(STYLE_FILE.to_string())
    }
}

pub fn get_index() -> io::Result<String> {
    if fs::exists("index.html")? {
        fs::read_to_string("index.html")
    } else {
        Ok(INDEX_FILE.to_string())
    }
}