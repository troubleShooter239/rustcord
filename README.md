# Rustcord

Rustcord is a Tauri 2 desktop client for the Discord web app. It uses WebView2 on Windows,
WebKitGTK on Linux, and WKWebView on macOS. It starts with one webview and opens the QuickCSS
editor on demand. On Windows it requests a lower WebView2 memory target when minimized or hidden
in the tray.

## Build and run

Use Node.js 22 or newer, Rust 1.90 or newer and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. Linux builds
also need `libssl-dev` or the distribution's equivalent. macOS requires version 14 or newer for
isolated WKWebView profiles.

```sh
node scripts/prepare-vencord.mjs
cargo build --release --locked
```

Run `target/release/rustcord` on Linux or macOS, or `target\release\rustcord.exe` on Windows.
For native packages, install the Tauri 2 CLI and run `cargo tauri build` on the target OS.

## Vencord and customization

The build embeds the [desktop Vencord renderer](https://github.com/Vendicated/Vencord/tree/d07030c0968e31e41caa0c35fda0ef8ae63a079e).
The preparation script verifies SHA-256 hashes against a pinned build snapshot. Plugin settings,
local themes and QuickCSS use files in the Rustcord profile. Set `RUSTCORD_DATA_DIR` before launch
to choose that directory. Sign in again to initialize the browser session in your Rustcord profile.
The profile keeps `settings/settings.json`, `settings/quickCss.css`, and the `themes/` directory
in the same layout as Vesktop.

Open the editor with **Rustcord → QuickCSS**. Save applies the styles to the current Discord page.
Open the themes folder from Vencord to add local `.css` files, then reload Discord. Start with
`--tray` to hide the main window on close, or with `--no-vencord` for an unmodified session.

The Rust bridge provides native methods for XSOverlay, Translate, OpenInApp and DirectFileDownload.
Vencord updates ship with Rustcord builds. Full parity with Vesktop still needs the CSP overrides
used by some external themes and plugins, embed hooks, Rich Presence and platform media integration.

## Windows transparency

Select **Rustcord → Window appearance** to save a material and restart. The default is opaque.
Use a launch flag for a one-off test.

```powershell
.\target\release\rustcord.exe --backdrop=transparent
```

Modes are `opaque`, `transparent`, `mica`, `tabbed` (Mica Alt), and `acrylic`.
Rustcord makes the WebView2 background transparent and applies built-in Discord styles so that
the native background can show through. Themes can paint over these styles. Windows controls
availability of backdrop materials, including the effect of the system transparency setting.
If Windows replaces a requested material, the title changes to **Rustcord · Transparent**.
System appearance mods such as Windhawk's Translucent Windows can override DWM materials.

## Measure resource use

Use the same Discord screen, window state, and Vencord configuration on both clients. Wait for
startup activity to settle before sampling. Run the measurement separately for the visible and
minimized states.

```powershell
.\scripts\measure-client.ps1 -RootPid (Get-Process rustcord).Id -Seconds 60
```

The script includes browser, GPU, and renderer descendants of the selected process. It reports
private committed memory, summed working sets (which can count shared pages more than once),
and CPU time as a percentage of one logical core.

## Original Vesktop client

Vesktop is a custom Discord desktop app

**Main features**:

- Vencord preinstalled
- Much more lightweight and faster than the official Discord app
- Linux Screenshare with sound & wayland
- Much better privacy, since Discord has no access to your system

**Not yet supported**:

- Global Keybinds
- see the [Roadmap](https://github.com/Vencord/Vesktop/issues/324)

![](https://github.com/Vencord/Vesktop/assets/45497981/8608a899-96a9-4027-9725-2cb02ba189fd)
![](https://github.com/Vencord/Vesktop/assets/45497981/8701e5de-52c4-4346-a990-719cb971642e)

## Installing

Visit https://vesktop.dev/install

## Building from Source

You need to have the following dependencies installed:

- [Git](https://git-scm.com/downloads)
- [Node.js](https://nodejs.org/en/download)
- pnpm: `npm install --global pnpm`

Packaging will create builds in the dist/ folder

```sh
git clone https://github.com/Vencord/Vesktop
cd Vesktop

# Install Dependencies
pnpm i

# Either run it without packaging
pnpm start

# Or package (will build packages for your OS)
pnpm package

# Or only build the Linux Pacman package
pnpm package --linux pacman

# Or package to a directory only
pnpm package:dir
```

## Building LibVesktop from Source

This is a small C++ helper library Vesktop uses on Linux to emit D-Bus events. By default, prebuilt binaries for x64 and arm64 are used.

If you want to build it from source:

1. Install build dependencies:
    - Debian/Ubuntu: `apt install build-essential python3 curl pkg-config libglib2.0-dev`
    - Fedora: `dnf install @c-development @development-tools python3 curl pkgconf-pkg-config glib2-devel`
2. Run `pnpm buildLibVesktop`
3. From now on, building Vesktop will use your own build
