import { readFile, mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

// Only evaluate the existing literal dictionaries, never the frontend runtime.
const root = new URL("../", import.meta.url);
const source = await readFile(new URL("src/main.js", root), "utf8");
const start = source.indexOf("const LANGUAGES = [");
const end = source.indexOf("let currentLang =");
if (start < 0 || end < start) throw new Error("Frontend resource boundaries changed");
const resources = vm.runInNewContext(`${source.slice(start, end)}; ({ languages: LANGUAGES, locales: I18N })`, {}, { timeout: 1000 });
await mkdir(new URL("resources/", import.meta.url), { recursive: true });
await writeFile(new URL("resources/strings.json", import.meta.url), JSON.stringify(resources, null, 2) + "\n");
console.log(`Synced ${resources.languages.length} languages and ${Object.keys(resources.locales).length} UI locales to ${fileURLToPath(new URL("resources/strings.json", import.meta.url))}`);
