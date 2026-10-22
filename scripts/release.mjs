import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { appendFile, copyFile, mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { checkVersions, projectRoot, validateVersion } from "./version.mjs";

const macs = [
  { id: "macos-aarch64", os: "macos-latest", target: "aarch64-apple-darwin", bundles: "dmg" },
];
const windows = { id: "windows-x64", os: "windows-latest", target: "x86_64-pc-windows-msvc", bundles: "nsis,msi" };

export function parseTag(tag) {
  const match = /^(native-)?v(\d+\.\d+(?:\.\d+)?)$/.exec(tag);
  if (!match || match[0] !== tag) throw new Error("Release tags must be vX.Y[.Z] or native-vX.Y[.Z]");
  const version = match[2].split(".").length === 2 ? `${match[2]}.0` : match[2];
  return { version: validateVersion(version), variant: match[1] ? "native" : "tauri" };
}

export function buildMatrix(variant) {
  if (!["tauri", "native"].includes(variant)) throw new Error("Unknown release variant");
  return { include: (variant === "native" ? macs : [windows, ...macs]).map((entry) => ({
    ...entry, workspace: variant === "native" ? "native-macos" : "src-tauri",
  })) };
}

export function assetNames(version, variant) {
  validateVersion(version);
  return buildMatrix(variant).include.flatMap(({ id }) => {
    const prefix = `${variant === "native" ? "SimpleT_Native" : "SimpleT"}_${version}_${id.replaceAll("-", "_")}`;
    return id === "windows-x64" ? [`${prefix}_setup.exe`, `${prefix}.msi`] : [`${prefix}.dmg`];
  });
}

export async function collectAssets(version, variant, target, root = projectRoot) {
  const matrix = buildMatrix(variant).include;
  const entry = matrix.find((item) => item.target === target);
  if (!entry) throw new Error(`Unsupported target: ${target}`);
  const names = assetNames(version, variant).filter((name) => name.includes(entry.id.replaceAll("-", "_")));
  const output = resolve(root, "release-assets");
  await mkdir(output, { recursive: true });
  for (const name of names) {
    const extension = name.slice(name.lastIndexOf("."));
    const directory = variant === "native" ? "native-macos/target/package"
      : `src-tauri/target/${target}/release/bundle/${extension === ".exe" ? "nsis" : extension.slice(1)}`;
    const candidates = (await readdir(resolve(root, directory))).filter((file) => file.endsWith(extension) && file.includes(`_${version}_`));
    if (candidates.length !== 1) throw new Error(`Expected one ${extension} installer for ${version} in ${directory}; found ${candidates.length}`);
    const source = resolve(root, directory, candidates[0]);
    if ((await stat(source)).size === 0) throw new Error(`Empty installer: ${source}`);
    await copyFile(source, resolve(output, name));
  }
  return names;
}

export async function verifyAssets(version, variant, directory) {
  const expected = assetNames(version, variant);
  const actual = (await readdir(directory)).filter((name) => name !== "SHA256SUMS").sort();
  if (JSON.stringify(actual) !== JSON.stringify([...expected].sort())) {
    throw new Error(`Release assets do not match. Expected: ${expected.join(", ")}; found: ${actual.join(", ")}`);
  }
  const files = [];
  for (const name of expected) {
    const path = resolve(directory, name);
    const info = await stat(path);
    if (!info.isFile() || info.size === 0) throw new Error(`Missing or empty installer: ${name}`);
    const digest = createHash("sha256").update(await readFile(path)).digest("hex");
    files.push({ name, path, digest });
  }
  return files;
}

export function releaseNotes(subjects, variant) {
  const changes = (type) => subjects.flatMap((subject) => {
    const match = new RegExp(`^${type}(?:\\([^)]+\\))?!?: (.+)$`).exec(subject);
    return match ? [`- ${match[1]}`] : [];
  });
  const features = changes("feat");
  const fixes = [...changes("fix"), ...changes("perf")];
  const app = variant === "native" ? '"/Applications/SimpleT Native.app"' : "/Applications/SimpleT.app";
  return [
    ...(variant === "native" ? ["Native macOS preview. Configuration and history are stored separately from SimpleT.", ""] : []),
    "## New Features", ...(features.length ? features : ["- No new features in this release."]), "",
    "## Bug Fixes", ...(fixes.length ? fixes : ["- No bug fixes in this release."]), "",
    "## macOS Signing Notice",
    "macOS builds use an ad hoc signature and are not signed with an Apple Developer ID certificate or notarized by Apple.",
    "Download only from this project's GitHub Releases. If macOS blocks the app, go to **System Settings → Privacy & Security → Open Anyway** after confirming the source is trusted.",
    "If it is still blocked, run:", "", "```bash", `xattr -dr com.apple.quarantine ${app}`, "```", "",
  ].join("\n");
}

function git(args, root = projectRoot) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

export async function finalize(tag, variant, root = projectRoot) {
  const version = await checkVersions(tag ? parseTag(tag).version : undefined, root);
  const files = await verifyAssets(version, variant, resolve(root, "release-assets"));
  await writeFile(resolve(root, "release-assets/SHA256SUMS"), files.map(({ name, digest }) => `${digest}  ${name}\n`).join(""));
  if (tag) {
    const sha = git(["rev-parse", "HEAD"], root);
    if (git(["rev-parse", `${tag}^{commit}`], root) !== sha || sha !== process.env.RELEASE_SHA) {
      throw new Error("The release tag no longer points to the commit that was built");
    }
    let previous;
    try {
      previous = git(["describe", "--tags", "--abbrev=0", "--match", variant === "native" ? "native-v[0-9]*" : "v[0-9]*", `${sha}^`], root);
    } catch { /* The first release has no previous matching tag. */ }
    const subjects = git(["log", "--format=%s", previous ? `${previous}..${sha}` : sha], root).split("\n");
    await writeFile(resolve(root, "release-notes.md"), releaseNotes(subjects, variant));
  }
  if (process.env.GITHUB_STEP_SUMMARY) {
    await appendFile(process.env.GITHUB_STEP_SUMMARY, `## ${tag || "Build verification (no Release created)"}\n\n${files.map(({ name }) => `- ${name}`).join("\n")}\n\nAll installers and SHA256SUMS are available in the workflow artifacts.\n`);
  }
  return files;
}

// Only the final job receives a write token. Build jobs never modify a Release.
export async function publishDraft({ github, owner, repo, tag, sha, directory, notes }) {
  const { version, variant } = parseTag(tag);
  const files = await verifyAssets(version, variant, directory);
  files.push({ name: "SHA256SUMS", path: resolve(directory, "SHA256SUMS") });
  const data = await Promise.all(files.map(async (file) => ({ ...file, data: await readFile(file.path) })));
  let release;
  try {
    ({ data: release } = await github.rest.repos.getReleaseByTag({ owner, repo, tag }));
  } catch (error) {
    if (error.status !== 404) throw error;
    // The tag endpoint only finds published releases. Drafts are listed for
    // callers with push access, including this job's contents:write token.
    release = (await github.paginate(github.rest.repos.listReleases, { owner, repo, per_page: 100 }))
      .find((item) => item.tag_name === tag);
  }
  if (release && !release.draft) throw new Error(`Release ${tag} is already published; published assets will not be replaced`);
  if (!release) {
    ({ data: release } = await github.rest.repos.createRelease({
      owner, repo, tag_name: tag, target_commitish: sha,
      name: variant === "native" ? `SimpleT Native ${version} Preview` : `SimpleT ${version}`,
      body: notes, draft: true, prerelease: variant === "native",
    }));
  } else if (Boolean(release.prerelease) !== (variant === "native")) {
    const { data: current } = await github.rest.repos.getRelease({ owner, repo, release_id: release.id });
    if (!current.draft) throw new Error("The Release was published while uploading; stopping");
    await github.rest.repos.updateRelease({ owner, repo, release_id: release.id, prerelease: variant === "native" });
  }
  const existing = await github.paginate(github.rest.repos.listReleaseAssets, { owner, repo, release_id: release.id, per_page: 100 });
  for (const file of data) {
    // Re-running a failed upload repairs only this draft; keep edited notes.
    const { data: current } = await github.rest.repos.getRelease({ owner, repo, release_id: release.id });
    if (!current.draft) throw new Error("The Release was published while uploading; stopping");
    const asset = existing.find((item) => item.name === file.name);
    if (asset) await github.rest.repos.deleteReleaseAsset({ owner, repo, asset_id: asset.id });
    await github.rest.repos.uploadReleaseAsset({
      owner, repo, release_id: release.id, name: file.name,
      headers: { "content-type": "application/octet-stream" }, data: file.data,
    });
  }
  return release.html_url;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const tag = process.env.RELEASE_TAG || "";
    const variant = tag ? parseTag(tag).variant : process.env.RELEASE_VARIANT || "tauri";
    const command = process.argv[2];
    if (command === "plan") {
      const version = await checkVersions(tag ? parseTag(tag).version : undefined);
      const outputs = { tag, variant, version, sha: git(["rev-parse", "HEAD"]), matrix: JSON.stringify(buildMatrix(variant)) };
      if (!process.env.GITHUB_OUTPUT) throw new Error("GITHUB_OUTPUT is required for the release plan");
      await appendFile(process.env.GITHUB_OUTPUT, Object.entries(outputs).map(([key, value]) => `${key}=${value}\n`).join(""));
    } else if (command === "collect") {
      console.log((await collectAssets(await checkVersions(process.env.RELEASE_VERSION), variant, process.env.RELEASE_TARGET)).join("\n"));
    } else if (command === "finalize") {
      await finalize(tag, variant);
    } else {
      throw new Error("Usage: node scripts/release.mjs <plan | collect | finalize>");
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
