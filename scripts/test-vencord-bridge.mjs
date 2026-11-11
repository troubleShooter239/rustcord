import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

const source = readFileSync(new URL("../src/native/vencord.js", import.meta.url), "utf8");

function bridge(invoke) {
    const storage = new Map();
    const sessionStorage = {
        getItem: key => storage.get(key) ?? null,
        setItem: (key, value) => storage.set(key, value)
    };
    const window = {
        __TAURI_INTERNALS__: { invoke: (_, { request }) => invoke(request) },
        sessionStorage
    };
    runInNewContext(source, { window, bootstrap: { settings: {}, session: "test" } });
    return { window, storage };
}

test("a burst writes only its latest snapshot and skips duplicate writes", async () => {
    const writes = [];
    const { window, storage } = bridge(request => writes.push(request.args.settings.value));
    const { settings } = window.VencordNative;
    await Promise.all(Array.from({ length: 100 }, (_, value) => settings.set({ value })));
    await settings.set({ value: 99 });
    assert.deepEqual(writes, [99]);
    assert.equal(settings.get().value, 99);
    assert.equal(JSON.parse(storage.get("RustcordVencordSettings")).settings.value, 99);
});

test("relaunch waits for the active save and the latest queued settings", async () => {
    const started = Promise.withResolvers();
    const release = Promise.withResolvers();
    const calls = [];
    const { window } = bridge(async request => {
        const value = request.args?.settings.value;
        calls.push(value ?? request.method);
        if (value === 1) {
            started.resolve();
            await release.promise;
        }
    });
    const { settings } = window.VencordNative;
    const first = settings.set({ value: 1 });
    await started.promise;
    const second = settings.set({ value: 2 });
    const third = settings.set({ value: 3 });
    const restart = window.VesktopNative.app.relaunch();
    assert.deepEqual(calls, [1]);
    release.resolve();
    await Promise.all([first, second, third, restart]);
    assert.deepEqual(calls, [1, 3, "relaunch"]);
});

test("a failed save can be retried with the same settings", async () => {
    let attempts = 0;
    const { window } = bridge(() => {
        if (++attempts === 1) throw new Error("write failed");
    });
    const { settings } = window.VencordNative;
    await assert.rejects(settings.set({ value: 1 }), /write failed/);
    await settings.set({ value: 1 });
    assert.equal(attempts, 2);
    assert.equal(settings.get().value, 1);
});

test("a failed save does not prevent restarting Rustcord", async () => {
    const { window } = bridge(request => {
        if (request.method === "save_settings") throw new Error("write failed");
        return request.method;
    });
    await assert.rejects(window.VencordNative.settings.set({ value: 1 }), /write failed/);
    assert.equal(await window.VesktopNative.app.relaunch(), "relaunch");
});
