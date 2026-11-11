use std::{fs::OpenOptions, net::UdpSocket, sync::Mutex};

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::{app::is_discord_url, plugin_network, profile::Profile};

pub(crate) struct NativeServices {
    token: String,
    overlay: Mutex<Option<UdpSocket>>,
}

impl NativeServices {
    pub(crate) fn new(token: String) -> Self {
        Self {
            token,
            overlay: Mutex::new(None),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "method", content = "args", rename_all = "snake_case")]
pub(crate) enum Request {
    SaveSettings {
        settings: Value,
    },
    GetCss,
    SaveCss {
        css: String,
    },
    OpenCssFile,
    OpenEditor,
    OpenSettingsFolder,
    ListThemes,
    GetTheme {
        name: String,
    },
    DeleteTheme {
        name: String,
    },
    OpenThemesFolder,
    OpenExternal {
        url: Url,
    },
    ResolveRedirect {
        url: Url,
    },
    SaveAttachment {
        url: Url,
    },
    Deepl {
        pro: bool,
        api_key: String,
        payload: String,
    },
    Kagi {
        token: String,
        text: String,
        source_lang: String,
        target_lang: String,
    },
    SendOverlay {
        data: serde_json::Map<String, Value>,
    },
    CloseOverlay,
    Relaunch,
}

#[tauri::command]
pub(crate) async fn vencord(
    window: WebviewWindow,
    app: AppHandle,
    token: String,
    request: Request,
) -> Result<Value, String> {
    if window.label() != "discord"
        || token != app.state::<NativeServices>().token
        || !is_discord_url(&window.url().map_err(error)?)
    {
        return Err("Vencord commands require the Discord window.".into());
    }
    match request {
        Request::ResolveRedirect { url } => plugin_network::resolve_redirect(url)
            .await
            .map(|url| json!(url))
            .map_err(error),
        Request::SaveAttachment { url } => {
            plugin_network::save_attachment(url, &app.path().download_dir().map_err(error)?)
                .await
                .map_err(error)?;
            Ok(Value::Null)
        }
        Request::Deepl {
            pro,
            api_key,
            payload,
        } => Ok(plugin_network::deepl(pro, &api_key, payload).await),
        Request::Kagi {
            token,
            text,
            source_lang,
            target_lang,
        } => Ok(plugin_network::kagi(&token, &text, &source_lang, &target_lang).await),
        request => tauri::async_runtime::spawn_blocking(move || run_request(&app, request))
            .await
            .map_err(error)?,
    }
}

fn run_request(app: &AppHandle, request: Request) -> Result<Value, String> {
    let profile = app.state::<Profile>();
    match request {
        Request::SaveSettings { settings } => profile.save_settings(&settings).map_err(error)?,
        Request::GetCss => return profile.quick_css().map(Value::String).map_err(error),
        Request::SaveCss { css } => save_css(app, &profile, &css)?,
        Request::OpenCssFile => {
            let path = profile.quick_css_path();
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(error)?;
            open::that_detached(path).map_err(error)?;
        }
        Request::OpenEditor => open_editor(app)?,
        Request::OpenSettingsFolder => {
            open::that_detached(profile.settings_dir()).map_err(error)?
        }
        Request::ListThemes => return profile.themes().map(|themes| json!(themes)).map_err(error),
        Request::GetTheme { name } => {
            return profile
                .theme(&name)
                .map(|theme| json!(theme))
                .map_err(error);
        }
        Request::DeleteTheme { name } => profile.delete_theme(&name).map_err(error)?,
        Request::OpenThemesFolder => open::that_detached(profile.themes_dir()).map_err(error)?,
        Request::OpenExternal { url } => open_vencord_link(&url)?,
        Request::SendOverlay { mut data } => {
            if let Some(kind) = data.get("type").cloned() {
                data.insert("messageType".into(), kind);
            }
            let bytes = serde_json::to_vec(&data).map_err(error)?;
            if bytes.len() > 16 * 1024 {
                return Err("XSOverlay packet exceeds 16 KiB.".into());
            }
            let native = app.state::<NativeServices>();
            let mut socket = native.overlay.lock().map_err(error)?;
            if socket.is_none() {
                *socket = Some(UdpSocket::bind("127.0.0.1:0").map_err(error)?);
            }
            if let Some(socket) = socket.as_ref() {
                socket.send_to(&bytes, "127.0.0.1:42069").map_err(error)?;
            }
        }
        Request::CloseOverlay => {
            let native = app.state::<NativeServices>();
            native.overlay.lock().map_err(error)?.take();
        }
        Request::Relaunch => app.request_restart(),
        Request::ResolveRedirect { .. }
        | Request::SaveAttachment { .. }
        | Request::Deepl { .. }
        | Request::Kagi { .. } => unreachable!("network request routed to the file worker"),
    }
    Ok(Value::Null)
}

pub(crate) fn open_editor(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("quick-css") {
        window.set_focus().map_err(error)?;
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(app, "quick-css", WebviewUrl::App("index.html".into()))
        .title("QuickCSS · Rustcord")
        .inner_size(720.0, 520.0)
        .min_inner_size(360.0, 260.0)
        .on_navigation(|url| {
            matches!(
                (url.scheme(), url.host_str()),
                ("tauri", Some("localhost")) | ("http" | "https", Some("tauri.localhost"))
            ) && url.port().is_none()
                && matches!(url.path(), "/" | "/index.html")
        });
    crate::app::webview_profile(builder, &app.state::<Profile>())
        .build()
        .map_err(error)?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn editor_load(
    window: WebviewWindow,
    profile: State<'_, Profile>,
) -> Result<String, String> {
    if window.label() != "quick-css" {
        return Err("Expected the QuickCSS editor.".into());
    }
    profile.quick_css().map_err(error)
}

#[tauri::command]
pub(crate) async fn editor_save(
    window: WebviewWindow,
    app: AppHandle,
    css: String,
) -> Result<(), String> {
    if window.label() != "quick-css" {
        return Err("Expected the QuickCSS editor.".into());
    }
    tauri::async_runtime::spawn_blocking(move || save_css(&app, &app.state::<Profile>(), &css))
        .await
        .map_err(error)?
}

fn save_css(app: &AppHandle, profile: &Profile, css: &str) -> Result<(), String> {
    profile.save_css(css).map_err(error)?;
    if let Some(discord) = app.get_webview_window("discord") {
        let css = serde_json::to_string(css).map_err(error)?;
        discord
            .eval(format!("window.__rustcordQuickCssChanged?.({css})"))
            .map_err(error)?;
    }
    Ok(())
}

pub(crate) fn open_external(url: &Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Only HTTP and HTTPS links can open in the system browser.".into());
    }
    open::that_detached(url.as_str()).map_err(error)
}

fn open_vencord_link(url: &Url) -> Result<(), String> {
    let allowed = match url.scheme() {
        "steam" if url.host_str() == Some("openurl") => {
            Url::parse(url.path().trim_start_matches('/')).is_ok_and(|target| {
                target.scheme() == "https"
                    && matches!(
                        target.host_str(),
                        Some(
                            "steamcommunity.com"
                                | "store.steampowered.com"
                                | "help.steampowered.com"
                        )
                    )
            })
        }
        "spotify" => {
            matches!(
                url.host_str(),
                Some("track" | "album" | "artist" | "playlist" | "user" | "episode" | "prerelease")
            ) || (url.host_str().is_none() && url.path().starts_with("user:"))
        }
        "tidal" => matches!(
            url.host_str(),
            Some("track" | "album" | "artist" | "playlist" | "user" | "video" | "mix")
        ),
        "itunes" => url.host_str() == Some("music.apple.com"),
        "com.epicgames.launcher" => url.host_str() == Some("store"),
        _ => false,
    };
    if allowed && url.username().is_empty() && url.password().is_none() && url.port().is_none() {
        open::that_detached(url.as_str()).map_err(error)
    } else {
        open_external(url)
    }
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
