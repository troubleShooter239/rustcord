use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use serde::Serialize;
use serde_json::Value;

pub(crate) const MAX_FILE_BYTES: usize = 1024 * 1024;

pub(crate) struct Profile {
    root: PathBuf,
    lock: File,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Theme {
    file_name: String,
    name: String,
    author: String,
    description: String,
}

impl Profile {
    pub(crate) fn open(root: PathBuf) -> io::Result<Self> {
        let root = std::path::absolute(root)?;
        fs::create_dir_all(&root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("rustcord.lock"))?;
        lock.try_lock().map_err(|error| match error {
            fs::TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                "Rustcord is already using this profile.",
            ),
            fs::TryLockError::Error(error) => error,
        })?;
        let profile = Self { root, lock };
        fs::create_dir_all(profile.settings_dir())?;
        fs::create_dir_all(profile.themes_dir())?;
        Ok(profile)
    }

    pub(crate) fn unlock_on_exit(&self) {
        let _ = self.lock.unlock();
    }

    pub(crate) fn browser_dir(&self) -> PathBuf {
        self.root.join("webview")
    }

    #[cfg(windows)]
    pub(crate) fn backdrop(&self) -> Option<String> {
        read_text(&self.root.join("backdrop")).ok()
    }

    #[cfg(windows)]
    pub(crate) fn save_backdrop(&self, value: &str) -> io::Result<()> {
        replace(&self.root, "backdrop", value.as_bytes())
    }

    pub(crate) fn settings_dir(&self) -> PathBuf {
        self.root.join("settings")
    }

    pub(crate) fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }

    pub(crate) fn settings(&self) -> io::Result<Value> {
        let path = self.settings_dir().join("settings.json");
        match read_text(&path) {
            Ok(settings) => serde_json::from_str(&settings).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(serde_json::json!({})),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn save_settings(&self, settings: &Value) -> io::Result<()> {
        if !settings.is_object() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Vencord settings",
            ));
        }
        let bytes = serde_json::to_vec(settings)?;
        replace(&self.settings_dir(), "settings.json", &bytes)
    }

    pub(crate) fn quick_css_path(&self) -> PathBuf {
        self.settings_dir().join("quickCss.css")
    }

    pub(crate) fn quick_css(&self) -> io::Result<String> {
        match read_text(&self.quick_css_path()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            result => result,
        }
    }

    pub(crate) fn save_css(&self, css: &str) -> io::Result<()> {
        replace(&self.settings_dir(), "quickCss.css", css.as_bytes())
    }

    pub(crate) fn themes(&self) -> io::Result<Vec<Theme>> {
        let mut themes = Vec::new();
        for entry in fs::read_dir(self.themes_dir())? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if !entry.file_type()?.is_file() || !valid_theme_name(&name) {
                continue;
            }
            if let Ok(css) = read_text(&entry.path()) {
                themes.push(theme_info(name, &css));
            }
        }
        themes.sort_unstable_by(|a, b| a.file_name.cmp(&b.file_name));
        Ok(themes)
    }

    pub(crate) fn theme(&self, name: &str) -> io::Result<Option<String>> {
        let path = self.theme_path(name)?;
        match fs::symlink_metadata(&path) {
            Ok(info) if info.is_file() => read_text(&path).map(Some),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn delete_theme(&self, name: &str) -> io::Result<()> {
        let path = self.theme_path(name)?;
        if fs::symlink_metadata(&path)?.is_file() {
            fs::remove_file(path)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a theme file",
            ))
        }
    }

    fn theme_path(&self, name: &str) -> io::Result<PathBuf> {
        if !valid_theme_name(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid theme name",
            ));
        }
        Ok(self.themes_dir().join(name))
    }
}

fn valid_theme_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(value)) if value == name)
        && components.next().is_none()
        && !name.contains(['/', '\\', ':'])
        && name
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("css"))
}

fn read_text(path: &Path) -> io::Result<String> {
    let mut content = String::new();
    File::open(path)?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_string(&mut content)?;
    if content.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Profile file exceeds 1 MiB",
        ));
    }
    let bom_len = content.len() - content.trim_start_matches('\u{feff}').len();
    if bom_len != 0 {
        content.replace_range(..bom_len, "");
    }
    Ok(content)
}

fn replace(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Profile file exceeds 1 MiB",
        ));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(dir)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(dir.join(name))?;
    Ok(())
}

fn theme_info(file_name: String, css: &str) -> Theme {
    let header = css
        .trim_start()
        .strip_prefix("/**")
        .and_then(|s| s.split_once("*/"));
    let field = |key| {
        header.and_then(|(block, _)| {
            block.lines().find_map(|line| {
                let line = line.trim().trim_start_matches('*').trim_start();
                let (name, value) = line.split_once(char::is_whitespace)?;
                (name == key && !value.trim().is_empty()).then(|| value.trim().to_owned())
            })
        })
    };
    Theme {
        name: field("@name").unwrap_or_else(|| file_name.rsplit_once('.').unwrap().0.to_owned()),
        author: field("@author").unwrap_or_else(|| "Unknown Author".into()),
        description: field("@description").unwrap_or_else(|| "A Discord Theme.".into()),
        file_name,
    }
}

#[cfg(test)]
mod tests {
    use super::valid_theme_name;

    #[test]
    fn themes_cannot_escape_the_profile() {
        assert!(valid_theme_name("Example.theme.css"));
        assert!(!valid_theme_name("../settings.css"));
        assert!(!valid_theme_name(r"..\settings.css"));
        assert!(!valid_theme_name("theme.css:stream.css"));
        assert!(!valid_theme_name("theme.js"));
    }
}
