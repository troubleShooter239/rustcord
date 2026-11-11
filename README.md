# Rustcord

Rustcord is a Tauri 2 desktop client for the Discord web app. It uses WebView2 on Windows,
WebKitGTK on Linux, and WKWebView on macOS. It starts with one webview and opens the QuickCSS
editor on demand. On Windows it requests a lower WebView2 memory target when minimized or hidden
in the tray.

## Build and run

Use Node.js 22 or newer, Rust 1.99.1 or newer and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. Linux builds
also need `libssl-dev` or the distribution's equivalent. macOS requires version 14 or newer for
isolated WKWebView profiles.
The preparation script uses Node.js to verify and cache the Vencord build artifacts before compilation.

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

## Performance roadmap

1. **Measure signed-in workloads.** Repeat cold and warm launch, channel idle and scrolling, minimized
   idle, voice, and screen sharing with the same settings and plugin configuration. Record startup
   latency, CPU time, and peak and steady-state memory across all client processes. On Windows,
   compare opaque and translucent modes under the same workload.
2. **Profile foreground CPU first.** The earlier login-screen measurement showed lower memory with
   Rustcord and higher foreground CPU. Use WPR with the WebView2 profile to attribute CPU to the
   host, renderer, or GPU. Use the web inspector to trace animation, layout, timers, and plugin
   callbacks. Compare identical workloads with individual plugins toggled to isolate their cost.
   [WebView2 profiling guide](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
3. **Reduce work in the hottest paths.** If settings or IPC show up in the trace, coalesce bursts of
   settings saves and measure writes and copies. Flush pending settings before exit and test reload
   and restart. If the renderer dominates, fix the measured animation, timer, or DOM work and
   compare frame pacing at the same settings.
4. **Measure memory retention and background behavior.** Repeatedly open and close QuickCSS,
   switch channels and themes, and minimize and restore. Track steady-state memory after each
   cycle. Tune memory policy per engine and verify notification delivery, reconnect, voice, and
   screen sharing during background use.
5. **Gate optimizations on compatibility.** Repeat the same scenarios on WebView2, WebKitGTK, and
   WKWebView. Check plugin loading, settings persistence, QuickCSS, local themes, and native plugin
   calls on each platform. Keep changes that produce a repeatable improvement while preserving
   those behaviors.

Rustcord originated as a port of [Vesktop](https://github.com/Vencord/Vesktop).
