/*
 * Rustcord, a Tauri desktop client for Discord
 * Copyright (c) 2026 Rustcord contributors
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

// This script runs inside the origin and top-frame check in app.rs.
const { invoke } = window.__TAURI_INTERNALS__;
const call = (method, args) =>
    invoke("vencord", {
        token: bootstrap.token,
        request: { method, args }
    });
const cssListeners = new Set();
const settingsKey = "RustcordVencordSettings";
let settingsSnapshot = bootstrap.settings;
let pendingSettings = Promise.resolve();
let waitingBatch;
let savedSettingsJson;
let sessionCache;

try {
    // Discord removes the global storage reference after startup.
    sessionCache = window.sessionStorage;
    const cached = JSON.parse(sessionCache.getItem(settingsKey));
    if (cached?.session === bootstrap.session) settingsSnapshot = cached.settings;
} catch {
    // Some navigation contexts block sessionStorage. The disk snapshot is still available.
}

function queueSettings(settings) {
    const json = JSON.stringify(settings);
    if (waitingBatch) {
        waitingBatch.json = json;
        return pendingSettings;
    }

    const batch = { json };
    waitingBatch = batch;
    pendingSettings = pendingSettings.catch(() => {}).then(async () => {
        waitingBatch = undefined;
        const snapshot = JSON.parse(batch.json);
        if (batch.json !== savedSettingsJson) {
            await call("save_settings", { settings: snapshot });
            savedSettingsJson = batch.json;
            try {
                sessionCache?.setItem(settingsKey, JSON.stringify({ session: bootstrap.session, settings: snapshot }));
            } catch {
                // The settings have already been written to the profile.
            }
        }
        settingsSnapshot = snapshot;
    });
    return pendingSettings;
}

Object.defineProperty(window, "__rustcordQuickCssChanged", {
    value: css => {
        for (const listener of cssListeners) listener(css);
    }
});

window.VencordNative = {
    settings: {
        get: () => settingsSnapshot,
        set: queueSettings,
        openFolder: () => call("open_settings_folder")
    },
    quickCss: {
        get: () => call("get_css"),
        set: css => call("save_css", { css }),
        addChangeListener: listener => cssListeners.add(listener),
        addThemeChangeListener() {},
        openFile: () => call("open_css_file"),
        openEditor: () => call("open_editor"),
        getEditorTheme: () => "vs-dark"
    },
    themes: {
        getThemesList: () => call("list_themes"),
        getThemeData: name => call("get_theme", { name }),
        deleteTheme: name => call("delete_theme", { name }),
        uploadTheme: () => Promise.reject(new Error("Copy local CSS files into the themes folder.")),
        openFolder: () => call("open_themes_folder"),
        getSystemValues: async () => ({})
    },
    native: {
        getVersions: () => ({ rustcord: bootstrap.version }),
        supportsWindowsMaterial: () => bootstrap.windows,
        openExternal: url => call("open_external", { url }),
        getRendererCss: async () => bootstrap.rendererCss,
        onRendererCssUpdate() {}
    },
    updater: {
        getRepo: async () => ({ ok: true, value: "https://github.com/Vendicated/Vencord" }),
        getUpdates: async () => ({ ok: true, value: [] }),
        update: async () => ({ ok: true, value: false }),
        rebuild: async () => ({ ok: true, value: false })
    },
    csp: {
        isDomainAllowed: async () => false,
        requestAddOverride: async () => "invalid",
        removeOverride: async () => false
    },
    pluginHelpers: {
        ConsoleShortcuts: {
            initDevtoolsOpenEagerLoad: async () => {}
        },
        DirectFileDownload: {
            saveAttachment: url => call("save_attachment", { url })
        },
        OpenInApp: {
            resolveRedirect: url => call("resolve_redirect", { url })
        },
        Translate: {
            makeDeeplTranslateRequest: (pro, apiKey, payload) => call("deepl", { pro, api_key: apiKey, payload }),
            makeKagiTranslateRequest: (token, text, sourceLang, targetLang) =>
                call("kagi", { token, text, source_lang: sourceLang, target_lang: targetLang })
        },
        XSOverlay: {
            sendToOverlay: data => call("send_overlay", { data }),
            closeSocket: () => call("close_overlay")
        }
    }
};

window.VesktopNative = {
    app: {
        getVersion: () => bootstrap.version,
        getRendererCss: async () => "",
        onRendererCssUpdate() {},
        isOutdated: async () => false,
        openUpdater: async () => {},
        relaunch: async () => {
            await pendingSettings.catch(() => {});
            return call("relaunch");
        }
    },
    clipboard: {
        copyImage: bytes => {
            const png = new Blob([bytes], { type: "image/png" });
            return navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
        }
    }
};
