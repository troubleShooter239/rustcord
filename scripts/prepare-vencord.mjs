import { createHash, randomUUID } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";

// Update the source revision and all hashes together after testing the bridge.
const version = "d07030c0968e31e41caa0c35fda0ef8ae63a079e";
const release = "https://raw.githubusercontent.com/Vencord/builds/7b52187d06792777e41ab61e6512f93d52579997/";
const directory = new URL("../.vencord/", import.meta.url);
const assets = {
    "vencordDesktopRenderer.js": "391b3709f25ac7642dc5c32e1638c2e55a79d2806d84cb6ddad095686df54bc7",
    "vencordDesktopRenderer.css": "f2dc03726e81af8ef67b017f6e441432580c088758a4dcd8c78b07686f650254",
    "vencordDesktopRenderer.js.LEGAL.txt": "45eff05ed5011021938f095940a157ff5fc21ce82be518eae3c835a7f017bcc7"
};

await mkdir(directory, { recursive: true });
for (const [name, expected] of Object.entries(assets)) {
    const destination = new URL(name, directory);
    const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
    const cached = await readFile(destination).catch(error => {
        if (error.code !== "ENOENT") throw error;
        return null;
    });
    if (cached && sha256(cached) === expected) continue;

    const response = await fetch(release + name, { signal: AbortSignal.timeout(30_000) });
    if (!response.ok) throw new Error(`Could not download ${name}: HTTP ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (sha256(bytes) !== expected) {
        throw new Error(`Digest mismatch for ${name} from Vencord ${version}.`);
    }
    const staging = new URL(`${name}.${randomUUID()}.tmp`, directory);
    await writeFile(staging, bytes, { flag: "wx" });
    await rename(staging, destination);
}
console.log(`Prepared Vencord ${version}`);
