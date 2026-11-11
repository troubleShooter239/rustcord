use std::{io, path::Path, sync::OnceLock, time::Duration};

use reqwest::{Client, Response, header, redirect};
use serde_json::{Value, json};
use tauri::Url;
use tokio::io::AsyncWriteExt;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_TRANSLATION: usize = 256 * 1024;
const MAX_ATTACHMENT: usize = 512 * 1024 * 1024;

fn client() -> Result<&'static Client> {
    static CLIENT: OnceLock<std::result::Result<Client, reqwest::Error>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .redirect(redirect::Policy::none())
                .connect_timeout(Duration::from_secs(8))
                .read_timeout(Duration::from_secs(30))
                .pool_idle_timeout(Duration::from_secs(30))
                .build()
        })
        .as_ref()
        .map_err(|error| error.to_string().into())
}

pub(crate) async fn resolve_redirect(mut url: Url) -> Result<Url> {
    if !https_host(&url, &["spotify.link", "s.team"]) {
        return Ok(url);
    }
    for _ in 0..5 {
        let response = client()?.head(url.clone()).send().await?;
        let Some(location) = response
            .headers()
            .get(header::LOCATION)
            .filter(|_| response.status().is_redirection())
        else {
            return Ok(url);
        };
        url = url.join(location.to_str()?)?;
        if !https_host(&url, &["spotify.link", "spotify.app.link", "s.team"]) {
            return Ok(url);
        }
    }
    Err("Too many short-link redirects.".into())
}

pub(crate) async fn deepl(pro: bool, api_key: &str, payload: String) -> Value {
    let translate = async {
        if payload.len() > MAX_TRANSLATION {
            return Err("Translation request exceeds 256 KiB.".into());
        }
        let url = if pro {
            "https://api.deepl.com/v2/translate"
        } else {
            "https://api-free.deepl.com/v2/translate"
        };
        let response = client()?
            .post(url)
            .header(header::AUTHORIZATION, format!("DeepL-Auth-Key {api_key}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await?;
        let (status, bytes) = translation_response(response).await?;
        Ok(json!({ "status": status, "data": String::from_utf8_lossy(&bytes) }))
    };
    translation_result(translate.await)
}

pub(crate) async fn kagi(token: &str, text: &str, source_lang: &str, target_lang: &str) -> Value {
    let translate = async {
        if text.len() > MAX_TRANSLATION {
            return Err("Translation request exceeds 256 KiB.".into());
        }
        let response = client()?.post("https://translate.kagi.com/api/translate")
            .header(header::COOKIE, format!("kagi_session={token}"))
            .json(&json!({ "text": text, "from": source_lang, "to": target_lang, "model": "standard" }))
            .send().await?;
        let (status, bytes) = translation_response(response).await?;
        Ok(json!({ "status": status, "data": serde_json::from_slice::<Value>(&bytes)? }))
    };
    translation_result(translate.await)
}

async fn translation_response(mut response: Response) -> Result<(u16, Vec<u8>)> {
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > MAX_TRANSLATION {
            return Err("Translation response exceeds 256 KiB.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((status, bytes))
}

fn translation_result(response: Result<Value>) -> Value {
    response.unwrap_or_else(|error| json!({ "status": -1, "data": error.to_string() }))
}

pub(crate) async fn save_attachment(url: Url, downloads: &Path) -> Result<()> {
    if !https_host(&url, &["cdn.discordapp.com", "media.discordapp.net"]) {
        return Err("Expected a Discord attachment URL.".into());
    }
    let filename = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty() && name.len() <= 180)
        .filter(|name| !name.contains([':', '\\', '/', '<', '>', '"', '|', '?', '*']))
        .filter(|name| !name.ends_with(['.', ' ']))
        .ok_or("Invalid attachment filename.")?
        .to_owned();
    let mut response = client()?.get(url).send().await?.error_for_status()?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > MAX_ATTACHMENT as u64)
    {
        return Err("Attachment unavailable or too large.".into());
    }
    let temporary = tempfile::NamedTempFile::new_in(downloads)?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    let mut length = 0;
    while let Some(chunk) = response.chunk().await? {
        length += chunk.len();
        if length > MAX_ATTACHMENT {
            return Err("Attachment exceeds 512 MiB.".into());
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    mark_download(temporary.path())?;

    let path = Path::new(&filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("Invalid attachment filename.")?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map_or_else(String::new, |value| format!(".{value}"));
    let mut temporary = temporary;
    for index in 0..100 {
        let name = if index == 0 {
            filename.clone()
        } else {
            format!("{stem} ({index}){extension}")
        };
        match temporary.persist_noclobber(downloads.join(name)) {
            Ok(_) => return Ok(()),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                temporary = error.file
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err("Could not select an unused attachment filename.".into())
}

#[cfg(windows)]
fn mark_download(path: &Path) -> io::Result<()> {
    let mut zone = path.as_os_str().to_owned();
    zone.push(":Zone.Identifier");
    std::fs::write(zone, "[ZoneTransfer]\r\nZoneId=3\r\n")
}

#[cfg(target_os = "macos")]
fn mark_download(path: &Path) -> io::Result<()> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?;
    let status = std::process::Command::new("/usr/bin/xattr")
        .args(["-w", "com.apple.quarantine"])
        .arg(format!("0081;{:x};Rustcord;", timestamp.as_secs()))
        .arg(path)
        .status()?;
    if !status.success() {
        return Err(io::Error::other("Could not add macOS download quarantine."));
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn mark_download(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn https_host(url: &Url, allowed: &[&str]) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| allowed.contains(&host))
}
