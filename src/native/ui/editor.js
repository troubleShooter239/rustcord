/*
 * Rustcord, a Tauri desktop client for Discord
 * Copyright (c) 2026 Rustcord contributors
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

const { invoke } = window.__TAURI_INTERNALS__;
const form = document.querySelector("form");
const css = document.querySelector("textarea");
const status = document.querySelector("output");
const save = document.querySelector("button");

async function load() {
    try {
        css.value = await invoke("editor_load");
        css.disabled = false;
        save.disabled = false;
        css.focus();
    } catch (error) {
        status.textContent = String(error);
    }
}

form.addEventListener("submit", async event => {
    event.preventDefault();
    save.disabled = true;
    try {
        await invoke("editor_save", { css: css.value });
        status.textContent = "Saved";
    } catch (error) {
        status.textContent = String(error);
    } finally {
        save.disabled = false;
    }
});

form.addEventListener("keydown", event => {
    if ((event.ctrlKey || event.metaKey) && event.key === "s") {
        event.preventDefault();
        if (!save.disabled) form.requestSubmit();
    }
});

load();
