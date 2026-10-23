const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const { copyFile, mkdir, mkdtemp, readFile, readdir, rm, writeFile } = require("node:fs/promises");
const { tmpdir } = require("node:os");
const { dirname, resolve } = require("node:path");
const test = require("node:test");

const versions = import("../scripts/version.mjs");
const releases = import("../scripts/release.mjs");

async function fixture(t, includeVersions = false) {
  const root = await mkdtemp(resolve(tmpdir(), "simplet-release-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  if (includeVersions) {
    const { projectRoot, versionFiles } = await versions;
    for (const file of versionFiles) {
      await mkdir(dirname(resolve(root, file)), { recursive: true });
      await copyFile(resolve(projectRoot, file), resolve(root, file));
    }
  }
  return root;
}

async function installers(directory, variant = "tauri", version = "1.3.0") {
  const { assetNames } = await releases;
  await mkdir(directory, { recursive: true });
  const names = assetNames(version, variant);
  for (const name of names) await writeFile(resolve(directory, name), `installer: ${name}`);
  await writeFile(resolve(directory, "SHA256SUMS"), "fixture checksums");
  return names;
}

function githubStub({ existing, error, publishedDuringUpload = false } = {}) {
  const calls = [];
  const draft = { id: 12, draft: true, html_url: "https://github.com/example/simplet/releases/12" };
  const github = { rest: { repos: {} } };
  for (const method of ["getReleaseByTag", "createRelease", "getRelease", "listReleases", "listReleaseAssets", "updateRelease", "deleteReleaseAsset", "uploadReleaseAsset"]) {
    github.rest.repos[method] = async (args) => {
      calls.push({ method, args });
      if (method === "getReleaseByTag") {
        if (error) throw error;
        if (!existing || existing.draft) throw Object.assign(new Error("Not found"), { status: 404 });
        return { data: existing };
      }
      if (method === "createRelease") return { data: draft };
      if (method === "getRelease") return { data: { ...draft, draft: !publishedDuringUpload } };
      return { data: {} };
    };
  }
  github.paginate = async (method, args) => {
    if (method === github.rest.repos.listReleases) {
      calls.push({ method: "listReleases", args });
      return existing ? [{ tag_name: "v1.3.0", ...existing }] : [];
    }
    return [{ id: 90, name: "unrelated.txt" }, { id: 91, name: "SHA256SUMS" }];
  };
  return { github, calls };
}

test("one version command updates all manifests and owned lockfile entries", async (t) => {
  const root = await fixture(t, true);
  const { checkVersions, setVersion, versionFiles } = await versions;
  const before = await readFile(resolve(root, "src-tauri/Cargo.lock"), "utf8");
  await setVersion("1.3.0", root);
  assert.equal(await checkVersions("1.3.0", root), "1.3.0");
  for (const file of versionFiles.filter((name) => name.endsWith("Cargo.toml"))) {
    assert.match(await readFile(resolve(root, file), "utf8"), /\[package\][\s\S]*?version = "1\.3\.0"/);
  }
  const npmLock = JSON.parse(await readFile(resolve(root, "package-lock.json"), "utf8"));
  assert.equal(npmLock.version, "1.3.0");
  assert.equal(npmLock.packages[""].version, "1.3.0");
  const after = await readFile(resolve(root, "src-tauri/Cargo.lock"), "utf8");
  const dependencies = (text) => text.replace(/(name = "simplet(?:-core)?"\r?\nversion = ")[^"]+"/g, '$1VERSION"');
  assert.equal(dependencies(before), dependencies(after), "third-party dependency versions must not change");
});

test("version drift fails with the exact file and expected version", async (t) => {
  const root = await fixture(t, true);
  const { checkVersions, setVersion } = await versions;
  await setVersion("1.3.0", root);
  const file = resolve(root, "native-macos/Cargo.lock");
  await writeFile(file, (await readFile(file, "utf8")).replace('name = "simplet-native"\nversion = "1.3.0"', 'name = "simplet-native"\nversion = "1.2.0"').replace('name = "simplet-native"\r\nversion = "1.3.0"', 'name = "simplet-native"\r\nversion = "1.2.0"'));
  await assert.rejects(checkVersions("1.3.0", root), /native-macos\/Cargo.lock.*simplet-native.*1\.2\.0.*1\.3\.0/);
});

test("invalid versions and malformed manifests fail before changing any files", async (t) => {
  const root = await fixture(t, true);
  const { setVersion, validateVersion } = await versions;
  for (const value of [undefined, "v1.3.0", "1.3", "01.3.0", "1.3.0\n", "1.3.0-rc.1"]) {
    assert.throws(() => validateVersion(value), /three-part version/);
  }
  const manifest = resolve(root, "native-macos/Cargo.toml");
  await writeFile(manifest, '[package]\nname = "wrong-name"\nversion = "1.2.0"\n');
  const before = await readFile(resolve(root, "package.json"), "utf8");
  await assert.rejects(setVersion("1.3.0", root), /Missing package simplet-native/);
  assert.equal(await readFile(resolve(root, "package.json"), "utf8"), before);
});

test("stable and native tags select separate release variants and architecture sets", async () => {
  const { parseTag, buildMatrix } = await releases;
  assert.deepEqual(parseTag("v1.3.0"), { version: "1.3.0", variant: "tauri" });
  assert.deepEqual(parseTag("native-v1.3.0"), { version: "1.3.0", variant: "native" });
  assert.deepEqual(parseTag("v2.0"), { version: "2.0.0", variant: "tauri" });
  assert.deepEqual(parseTag("native-v2.0"), { version: "2.0.0", variant: "native" });
  for (const tag of ["master", "v2", "v2.0.0.0", "v2.0\n", "v02.0", "v1.3.0-rc.1", "v1.3.0\n", "v1.3.0; echo bad", "native-v01.3.0"]) {
    assert.throws(() => parseTag(tag));
  }
  const stable = buildMatrix("tauri").include;
  assert.equal(stable.length, 2);
  const windows = stable.find(({ target }) => target === "x86_64-pc-windows-msvc");
  assert.equal(windows.args, "--no-bundle");
  assert.equal(windows.plainBinary, true);
  const mac = stable.find(({ target }) => target === "aarch64-apple-darwin");
  assert.equal(mac.args, "--bundles dmg");
  assert.equal(mac.plainBinary, false);
  assert.ok(!stable.some(({ target }) => target === "x86_64-apple-darwin"));
  const native = buildMatrix("native").include;
  assert.deepEqual(native.map(({ target }) => target), ["aarch64-apple-darwin"]);
  assert.ok(native.every(({ workspace, os }) => workspace === "native-macos" && os.startsWith("macos-")));
  assert.throws(() => buildMatrix("unknown"));
});

test("Windows collection uses the standalone program instead of cached installers", async (t) => {
  const root = await fixture(t);
  const { collectAssets } = await releases;
  const release = resolve(root, "src-tauri/target/x86_64-pc-windows-msvc/release");
  for (const [bundle, extension] of [["nsis", "exe"], ["msi", "msi"]]) {
    const directory = resolve(release, `bundle/${bundle}`);
    await mkdir(directory, { recursive: true });
    await writeFile(resolve(directory, `SimpleT_1.3.0_x64.${extension}`), "installer, not the program");
    await writeFile(resolve(directory, `SimpleT_1.2.0_x64.${extension}`), "stale installer");
  }
  await writeFile(resolve(release, "simplet.exe"), "standalone program");
  await writeFile(resolve(release, "SimpleT_1.2.0_windows_x64.exe"), "stale renamed copy");
  const names = await collectAssets("1.3.0", "tauri", "x86_64-pc-windows-msvc", root);
  assert.deepEqual(names, ["SimpleT_1.3.0_windows_x64.exe"]);
  assert.equal(await readFile(resolve(root, "release-assets", names[0]), "utf8"), "standalone program");
  await rm(resolve(release, "simplet.exe"));
  await assert.rejects(collectAssets("1.3.0", "tauri", "x86_64-pc-windows-msvc", root), /ENOENT/);
  await assert.rejects(collectAssets("1.3.0", "native", "x86_64-pc-windows-msvc", root), /Unsupported target/);
});

test("native collection keeps the preview label and rejects Intel Mac builds", async (t) => {
  const root = await fixture(t);
  const directory = resolve(root, "native-macos/target/package");
  await mkdir(directory, { recursive: true });
  await writeFile(resolve(directory, "SimpleT_Native_1.3.0_aarch64.dmg"), "native installer");
  const { collectAssets } = await releases;
  assert.deepEqual(await collectAssets("1.3.0", "native", "aarch64-apple-darwin", root), ["SimpleT_Native_1.3.0_macos_aarch64.dmg"]);
  await assert.rejects(collectAssets("1.3.0", "native", "x86_64-apple-darwin", root), /Unsupported target/);
});

test("missing, empty and obsolete release files block release assembly", async (t) => {
  const directory = await fixture(t);
  const { verifyAssets } = await releases;
  const names = await installers(directory);
  const macInstaller = names.at(-1);
  await rm(resolve(directory, macInstaller));
  await assert.rejects(verifyAssets("1.3.0", "tauri", directory), /assets do not match/);
  await writeFile(resolve(directory, macInstaller), "");
  await assert.rejects(verifyAssets("1.3.0", "tauri", directory), /empty release file/);
  await writeFile(resolve(directory, macInstaller), "Mac installer");
  await writeFile(resolve(directory, "SimpleT_1.3.0_macos_x64.dmg"), "old Intel build");
  await assert.rejects(verifyAssets("1.3.0", "tauri", directory), /assets do not match/);
  await rm(resolve(directory, "SimpleT_1.3.0_macos_x64.dmg"));
  await writeFile(resolve(directory, "SimpleT_1.3.0_windows_x64.msi"), "old Windows installer");
  await assert.rejects(verifyAssets("1.3.0", "tauri", directory), /assets do not match/);
});

test("build-only finalization checks all installers and produces verifiable SHA-256 sums", async (t) => {
  const root = await fixture(t, true);
  const { setVersion } = await versions;
  const { finalize } = await releases;
  await setVersion("1.3.0", root);
  const directory = resolve(root, "release-assets");
  const names = await installers(directory);
  await finalize("", "tauri", root);
  const sums = await readFile(resolve(directory, "SHA256SUMS"), "utf8");
  for (const name of names) {
    const digest = createHash("sha256").update(await readFile(resolve(directory, name))).digest("hex");
    assert.ok(sums.includes(`${digest}  ${name}\n`));
  }
  assert.ok(!(await readdir(root)).includes("release-notes.md"), "build-only mode does not prepare a release");
});

test("release notes retain three English sections and the correct macOS app path", async () => {
  const { releaseNotes } = await releases;
  const subjects = ["feat(history): persist translations", "fix(macos): restore focus", "perf: reduce memory", "chore: update lockfile"];
  for (const variant of ["tauri", "native"]) {
    const notes = releaseNotes(subjects, variant);
    assert.deepEqual(notes.match(/^## .+$/gm), ["## New Features", "## Bug Fixes", "## macOS Signing Notice"]);
    assert.match(notes, /- persist translations/);
    assert.match(notes, /- restore focus/);
    assert.match(notes, /- reduce memory/);
    assert.doesNotMatch(notes, /update lockfile/);
    assert.ok(notes.includes(variant === "native" ? 'xattr -dr com.apple.quarantine "/Applications/SimpleT Native.app"' : "xattr -dr com.apple.quarantine /Applications/SimpleT.app"));
  }
});

test("draft publication pins the built commit and uploads only the complete asset set", async (t) => {
  const directory = await fixture(t);
  await installers(directory);
  const { publishDraft } = await releases;
  const { github, calls } = githubStub();
  const url = await publishDraft({ github, owner: "example", repo: "simplet", tag: "v1.3.0", sha: "built-commit", directory, notes: "Release notes" });
  assert.match(url, /releases\/12$/);
  const creation = calls.find(({ method }) => method === "createRelease").args;
  assert.equal(creation.draft, true);
  assert.equal(creation.prerelease, false);
  assert.equal(creation.target_commitish, "built-commit");
  assert.equal(calls.filter(({ method }) => method === "uploadReleaseAsset").length, 3);
});

test("a short release tag is preserved while installer and app versions use three parts", async (t) => {
  const directory = await fixture(t);
  const names = await installers(directory, "tauri", "2.0.0");
  const { publishDraft } = await releases;
  const { github, calls } = githubStub();
  await publishDraft({ github, owner: "example", repo: "simplet", tag: "v2.0", sha: "built-commit", directory, notes: "Version 2.0" });
  const creation = calls.find(({ method }) => method === "createRelease").args;
  assert.equal(creation.tag_name, "v2.0");
  assert.equal(creation.name, "SimpleT 2.0.0");
  assert.deepEqual(calls.filter(({ method }) => method === "uploadReleaseAsset").map(({ args }) => args.name), [...names, "SHA256SUMS"]);
  assert.deepEqual(names, ["SimpleT_2.0.0_windows_x64.exe", "SimpleT_2.0.0_macos_aarch64.dmg"]);
});

test("native drafts are always marked as previews", async (t) => {
  const directory = await fixture(t);
  await installers(directory, "native");
  const { publishDraft } = await releases;
  const { github, calls } = githubStub();
  await publishDraft({ github, owner: "example", repo: "simplet", tag: "native-v1.3.0", sha: "built-commit", directory, notes: "Preview" });
  const creation = calls.find(({ method }) => method === "createRelease").args;
  assert.equal(creation.prerelease, true);
  assert.equal(creation.name, "SimpleT Native 1.3.0 Preview");
});

test("rerunning draft uploads preserves edited notes and unrelated assets", async (t) => {
  const directory = await fixture(t);
  await installers(directory);
  const { publishDraft } = await releases;
  const { github, calls } = githubStub({ existing: { id: 12, draft: true, body: "Edited by the user" } });
  await publishDraft({ github, owner: "example", repo: "simplet", tag: "v1.3.0", sha: "built-commit", directory, notes: "Generated notes" });
  assert.ok(!calls.some(({ method }) => method === "createRelease"));
  assert.ok(calls.some(({ method }) => method === "listReleases"), "drafts must be found even when the tag endpoint returns 404");
  assert.deepEqual(calls.filter(({ method }) => method === "deleteReleaseAsset").map(({ args }) => args.asset_id), [91]);
});

test("an existing native draft is marked as a prerelease without replacing its notes", async (t) => {
  const directory = await fixture(t);
  await installers(directory, "native");
  const { publishDraft } = await releases;
  const { github, calls } = githubStub({ existing: { id: 12, tag_name: "native-v1.3.0", draft: true, prerelease: false, body: "Edited preview notes" } });
  await publishDraft({ github, owner: "example", repo: "simplet", tag: "native-v1.3.0", sha: "built-commit", directory, notes: "Generated notes" });
  assert.ok(!calls.some(({ method }) => method === "createRelease"));
  const update = calls.find(({ method }) => method === "updateRelease").args;
  assert.equal(update.prerelease, true);
  assert.ok(!Object.hasOwn(update, "body"));
});

test("published releases and API permission failures never trigger release writes", async (t) => {
  const directory = await fixture(t);
  await installers(directory);
  const { publishDraft } = await releases;
  for (const options of [{ existing: { id: 12, draft: false } }, { error: Object.assign(new Error("Forbidden"), { status: 403 }) }]) {
    const { github, calls } = githubStub(options);
    await assert.rejects(publishDraft({ github, owner: "example", repo: "simplet", tag: "v1.3.0", sha: "built-commit", directory, notes: "Notes" }));
    assert.ok(!calls.some(({ method }) => ["createRelease", "deleteReleaseAsset", "uploadReleaseAsset"].includes(method)));
  }
});

test("publishing a draft during an upload stops further asset changes", async (t) => {
  const directory = await fixture(t);
  await installers(directory);
  const { publishDraft } = await releases;
  const { github, calls } = githubStub({ existing: { id: 12, draft: true }, publishedDuringUpload: true });
  await assert.rejects(publishDraft({ github, owner: "example", repo: "simplet", tag: "v1.3.0", sha: "built-commit", directory, notes: "Notes" }), /published while uploading/);
  assert.ok(!calls.some(({ method }) => ["deleteReleaseAsset", "uploadReleaseAsset"].includes(method)));
});
