import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import "./sync-resources.mjs";

if (process.platform !== "darwin") throw new Error("The macOS app can only be built on macOS");
const root = new URL("./", import.meta.url);
const packageMode = process.argv.includes("--dmg");
const version = (await readFile(new URL("Cargo.toml", root), "utf8")).match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!version) throw new Error("Native package version is missing");
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: fileURLToPath(root), stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with status ${result.status}`);
};
run("cargo", ["build", "--release", "--locked"]);
// Packaging has a separate output so a running development preview stays intact.
const output = new URL(packageMode ? "target/package/" : "target/release/bundle/", root);
const application = new URL("SimpleT.app/", output);
const bundle = new URL("Contents/", application);
await mkdir(new URL("MacOS/", bundle), { recursive: true });
await mkdir(new URL("Resources/", bundle), { recursive: true });
await cp(new URL("target/release/simplet-native", root), new URL("MacOS/simplet-native", bundle));
await cp(new URL("../src-tauri/icons/icon.icns", root), new URL("Resources/icon.icns", bundle));
await cp(new URL("../src-tauri/third_party/WholeWindowLiquidGlass.LICENSE", root), new URL("Resources/WholeWindowLiquidGlass.LICENSE", bundle));
await writeFile(new URL("Info.plist", bundle), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>simplet-native</string>
<key>CFBundleIdentifier</key><string>com.etherzhang.simplet</string>
<key>CFBundleName</key><string>SimpleT</string>
<key>CFBundleDisplayName</key><string>SimpleT</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>${version}</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>\n`);
run("/usr/bin/codesign", ["--force", "--sign", "-", fileURLToPath(application)]);
run("/usr/bin/codesign", ["--verify", "--deep", "--strict", fileURLToPath(application)]);
console.log(fileURLToPath(application));

if (packageMode) {
  const architecture = spawnSync("/usr/bin/lipo", ["-archs", fileURLToPath(new URL("MacOS/simplet-native", bundle))], { encoding: "utf8" });
  if (architecture.error) throw architecture.error;
  if (architecture.status !== 0) throw new Error("Cannot determine application architecture");
  const architectures = architecture.stdout.trim().split(/\s+/);
  const suffix = architectures.includes("arm64") && architectures.includes("x86_64") ? "universal"
    : architectures[0] === "arm64" ? "aarch64" : architectures[0];
  if (!/^[a-zA-Z0-9_]+$/.test(suffix)) throw new Error("Unexpected application architecture");
  const name = `SimpleT_${version}_${suffix}.dmg`;
  const image = new URL(name, output);
  const staging = await mkdtemp(fileURLToPath(new URL("dmg-", output)));
  try {
    await cp(application, join(staging, "SimpleT.app"), { recursive: true });
    await symlink("/Applications", join(staging, "Applications"));
    run("/usr/bin/hdiutil", ["create", "-volname", "SimpleT", "-srcfolder", staging, "-fs", "HFS+", "-format", "UDZO", "-ov", fileURLToPath(image)]);
    run("/usr/bin/hdiutil", ["verify", fileURLToPath(image)]);
    const checksum = createHash("sha256").update(await readFile(image)).digest("hex");
    await writeFile(new URL(`${name}.sha256`, output), `${checksum}  ${name}\n`);
    console.log(fileURLToPath(image));
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}
