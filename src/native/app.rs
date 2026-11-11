use std::{env, path::PathBuf};

use serde_json::json;
use tauri::{
    App, AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::NewWindowResponse,
};

use crate::{bridge, profile::Profile};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const DISCORD: &str = "https://discord.com/app";
const TRANSPARENCY_CSS: &str = include_str!("transparency.css");
const VENCORD_BRIDGE: &str = include_str!("vencord.js");
const VENCORD_RENDERER: &str = include_str!("../../.vencord/vencordDesktopRenderer.js");
const VENCORD_CSS: &str = include_str!("../../.vencord/vencordDesktopRenderer.css");

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum Backdrop {
    #[default]
    Opaque,
    Transparent,
    Mica,
    Tabbed,
    Acrylic,
}

impl Backdrop {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "opaque" => Some(Self::Opaque),
            "transparent" => Some(Self::Transparent),
            "mica" => Some(Self::Mica),
            "tabbed" => Some(Self::Tabbed),
            "acrylic" => Some(Self::Acrylic),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct Options {
    tray: bool,
    no_vencord: bool,
    backdrop: Option<Backdrop>,
}

impl Options {
    pub(crate) fn parse() -> Result<Self> {
        let mut options = Self::default();
        for arg in env::args_os().skip(1) {
            match arg.to_str() {
                Some("--tray") => options.tray = true,
                Some("--no-vencord") => options.no_vencord = true,
                Some(value) if value.starts_with("--backdrop=") => {
                    options.backdrop = Some(
                        Backdrop::parse(&value["--backdrop=".len()..])
                            .ok_or("Expected opaque, transparent, mica, tabbed or acrylic.")?,
                    );
                }
                _ => return Err(format!("Unknown option: {}", arg.to_string_lossy()).into()),
            }
        }
        if !cfg!(windows) && options.backdrop.is_some() {
            return Err("The --backdrop option requires Windows.".into());
        }
        Ok(options)
    }
}

pub(crate) fn setup(app: &mut App, options: Options) -> Result<()> {
    let root = match env::var_os("RUSTCORD_DATA_DIR") {
        Some(path) => PathBuf::from(path),
        None => app.path().app_local_data_dir()?,
    };
    let profile = Profile::open(root)?;
    let token = uuid::Uuid::new_v4().to_string();
    #[cfg(windows)]
    let backdrop = options
        .backdrop
        .or_else(|| profile.backdrop().as_deref().and_then(Backdrop::parse))
        .unwrap_or_default();
    #[cfg(not(windows))]
    let backdrop = Backdrop::Opaque;
    let init = initialization_script(&profile, options.no_vencord, backdrop, &token)?;
    app.manage(profile);
    app.manage(bridge::NativeServices::new(token));

    let builder = WebviewWindowBuilder::new(app, "discord", WebviewUrl::External(DISCORD.parse()?))
        .title("Rustcord")
        .inner_size(1280.0, 720.0)
        .min_inner_size(420.0, 320.0)
        .visible(false)
        .initialization_script(init)
        .on_navigation(is_discord_url)
        .on_new_window(|url, _| {
            let _ = bridge::open_external(&url);
            NewWindowResponse::Deny
        });
    let builder = webview_profile(builder, &app.state::<Profile>());
    #[cfg(windows)]
    let builder = {
        use tauri::window::Color;
        if backdrop != Backdrop::Opaque {
            builder
                .transparent(backdrop == Backdrop::Transparent)
                .background_color(Color(0, 0, 0, 0))
        } else {
            builder
        }
    };
    let window = builder.build()?;
    install_menu(
        app,
        !options.no_vencord,
        backdrop,
        options.backdrop.is_none(),
    )?;
    if options.tray {
        install_tray(app)?;
    }
    install_window_handlers(&window, options.tray);
    window.show()?;
    #[cfg(windows)]
    if let Err(error) = crate::windows_webview::apply_backdrop(&window, backdrop) {
        eprintln!("Windows could not apply the selected material: {error}");
        let _ = window.set_title("Rustcord · Transparent");
    }
    Ok(())
}

pub(crate) fn webview_profile<'a, M: Manager<tauri::Wry>>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, M>,
    profile: &Profile,
) -> WebviewWindowBuilder<'a, tauri::Wry, M> {
    let directory = profile.browser_dir();
    #[cfg(target_os = "macos")]
    {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(directory.as_os_str().as_encoded_bytes());
        let mut identifier = [0; 16];
        identifier.copy_from_slice(&digest[..16]);
        builder.data_store_identifier(identifier)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let builder = builder.data_directory(directory);
        #[cfg(windows)]
        let builder = builder.additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI");
        builder
    }
}

fn initialization_script(
    profile: &Profile,
    no_vencord: bool,
    backdrop: Backdrop,
    token: &str,
) -> Result<String> {
    let mut script = String::from(
        "\"use strict\";\nif (window === window.top && location.origin === 'https://discord.com') {\n",
    );
    if backdrop != Backdrop::Opaque {
        let css = serde_json::to_string(TRANSPARENCY_CSS)?;
        script.push_str(&format!(
            "const applyBackdrop = () => {{
                const root = document.documentElement;
                root.dataset.rustcordBackdrop = 'translucent';
                const style = document.createElement('style');
                style.textContent = {css};
                (document.head || root).append(style);
            }};
            if (document.documentElement) applyBackdrop();
            else document.addEventListener('DOMContentLoaded', applyBackdrop, {{ once: true }});\n"
        ));
    }
    if !no_vencord {
        let bootstrap = json!({
            "settings": profile.settings()?,
            "token": token,
            "session": uuid::Uuid::new_v4().to_string(),
            "rendererCss": VENCORD_CSS,
            "version": env!("CARGO_PKG_VERSION"),
            "windows": cfg!(windows)
        });
        script.push_str(&format!(
            "{{ const bootstrap = {bootstrap};\n{VENCORD_BRIDGE}\n}}\n{VENCORD_RENDERER}\n"
        ));
    }
    script.push('}');
    Ok(script)
}

pub(crate) fn is_discord_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("discord.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn install_menu(
    app: &App,
    vencord: bool,
    _backdrop: Backdrop,
    _appearance_enabled: bool,
) -> tauri::Result<()> {
    let editor = MenuItem::with_id(
        app,
        "quick-css",
        "QuickCSS…",
        vencord,
        Some("CmdOrCtrl+Shift+E"),
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Rustcord", true, Some("CmdOrCtrl+Q"))?;
    let submenu = Submenu::with_items(app, "Rustcord", true, &[&editor])?;
    #[cfg(windows)]
    {
        use tauri::menu::CheckMenuItem;
        let appearance = Submenu::new(
            app,
            "Window appearance · restarts Rustcord",
            _appearance_enabled,
        )?;
        for (name, title) in [
            ("opaque", "Opaque"),
            ("transparent", "Transparent"),
            ("mica", "Mica"),
            ("tabbed", "Mica Alt"),
            ("acrylic", "Acrylic"),
        ] {
            let item = CheckMenuItem::with_id(
                app,
                format!("backdrop-{name}"),
                title,
                true,
                Backdrop::parse(name) == Some(_backdrop),
                None::<&str>,
            )?;
            appearance.append(&item)?;
        }
        submenu.append(&appearance)?;
    }
    submenu.append(&quit)?;
    app.set_menu(Menu::with_items(app, &[&submenu])?)?;
    app.on_menu_event(|app, event| match event.id().as_ref() {
        "show" => show_discord(app),
        "quick-css" => {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = bridge::open_editor(&app) {
                    eprintln!("QuickCSS: {error}");
                }
            });
        }
        "quit" => app.exit(0),
        #[cfg(windows)]
        id if id.starts_with("backdrop-") => {
            let name = id["backdrop-".len()..].to_owned();
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if Backdrop::parse(&name).is_some() {
                    match app.state::<Profile>().save_backdrop(&name) {
                        Ok(()) => app.request_restart(),
                        Err(error) => eprintln!("Window appearance: {error}"),
                    }
                }
            });
        }
        _ => {}
    });
    Ok(())
}

fn install_tray(app: &App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Rustcord", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Rustcord", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id("rustcord")
        .tooltip("Rustcord")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_discord(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn show_discord(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("discord") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn install_window_handlers(window: &WebviewWindow, tray: bool) {
    let owned = window.clone();
    #[cfg(windows)]
    let minimized = std::cell::Cell::new(false);
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } if tray => {
            if owned.hide().is_ok() {
                api.prevent_close();
                #[cfg(windows)]
                if !minimized.replace(true) {
                    crate::windows_webview::set_backgrounded(&owned, true);
                }
            }
        }
        WindowEvent::Destroyed => owned.app_handle().exit(0),
        #[cfg(windows)]
        WindowEvent::Resized(_) | WindowEvent::Focused(_) => {
            if let (Ok(iconic), Ok(visible)) = (owned.is_minimized(), owned.is_visible()) {
                let hidden = iconic || !visible;
                if minimized.replace(hidden) != hidden {
                    crate::windows_webview::set_backgrounded(&owned, hidden);
                }
            }
        }
        _ => {}
    });
}
