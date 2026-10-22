import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const projectRoot = fileURLToPath(new URL("../", import.meta.url));
const crates = {
  "src-tauri": ["simplet", "simplet-core"],
  "crates/simplet-core": ["simplet-core"],
  "native-macos": ["simplet-native", "simplet-core"],
};
export const versionFiles = [
  "package.json", "package-lock.json", "src-tauri/tauri.conf.json",
  ...Object.keys(crates).flatMap((directory) => [`${directory}/Cargo.toml`, `${directory}/Cargo.lock`]),
];

export function validateVersion(version) {
  if (typeof version !== "string" || version.trim() !== version
      || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error("Use a three-part version, for example 1.3.0");
  }
  return version;
}

function cargoVersions(source, names, lockfile, update) {
  const found = new Map();
  const sections = lockfile ? /\[\[package\]\][\s\S]*?(?=\n\[\[package\]\]|$)/g
    : /\[package\][\s\S]*?(?=\n\[|$)/g;
  const text = source.replace(sections, (section) => {
    const name = section.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    if (!names.includes(name)) return section;
    const version = section.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    if (!version || found.has(name)) throw new Error(`Missing or duplicate version for ${name}`);
    found.set(name, version);
    return update === undefined ? section : section.replace(/^(version\s*=\s*)"[^"]+"/m, `$1"${update}"`);
  });
  for (const name of names) {
    if (!found.has(name)) throw new Error(`Missing package ${name}`);
  }
  return { text, versions: [...found].map(([name, version]) => ({ name, version })) };
}

async function versionDocuments(root, update) {
  // Read and validate every document before writing any version changes.
  const documents = [];
  for (const file of versionFiles) {
    const source = await readFile(resolve(root, file), "utf8");
    let text;
    let versions;
    if (file.endsWith(".json")) {
      const data = JSON.parse(source);
      versions = [{ name: "version", version: data.version }];
      if (file === "package-lock.json") {
        versions.push({ name: "packages[root]", version: data.packages[""].version });
      }
      if (update !== undefined) {
        data.version = update;
        if (file === "package-lock.json") data.packages[""].version = update;
      }
      text = JSON.stringify(data, null, 2) + "\n";
      if (source.includes("\r\n")) text = text.replaceAll("\n", "\r\n");
    } else {
      const directory = file.slice(0, file.lastIndexOf("/"));
      const lockfile = file.endsWith(".lock");
      ({ text, versions } = cargoVersions(source, lockfile ? crates[directory] : [crates[directory][0]], lockfile, update));
    }
    documents.push({ file, text, versions });
  }
  return documents;
}

export async function checkVersions(expected, root = projectRoot) {
  const documents = await versionDocuments(root);
  const version = validateVersion(expected ?? documents[0].versions[0].version);
  for (const { file, versions } of documents) {
    for (const entry of versions) {
      if (entry.version !== version) {
        throw new Error(`${file} (${entry.name}) is ${entry.version}; expected ${version}. Run npm run release:version -- ${version}`);
      }
    }
  }
  return version;
}

export async function setVersion(version, root = projectRoot) {
  validateVersion(version);
  const documents = await versionDocuments(root, version);
  for (const { file, text } of documents) await writeFile(resolve(root, file), text);
  await checkVersions(version, root);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [command, version] = process.argv.slice(2);
    if (command === "set" && version) {
      await setVersion(version);
      console.log(`Updated all application and lockfile versions to ${version}`);
    } else if (command === "check") {
      console.log(`Versions match: ${await checkVersions(version)}`);
    } else {
      throw new Error("Usage: node scripts/version.mjs <set VERSION | check [VERSION]>");
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
