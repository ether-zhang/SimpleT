import { cp, mkdir, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import "./sync-resources.mjs";

if (process.platform !== "darwin") throw new Error("The AppKit prototype can only be built on macOS");
const root = new URL("./", import.meta.url);
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: fileURLToPath(root), stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
};
run("cargo", ["build", "--release", "--locked"]);
const bundle = new URL("target/release/bundle/SimpleT Native.app/Contents/", root);
await mkdir(new URL("MacOS/", bundle), { recursive: true });
await mkdir(new URL("Resources/", bundle), { recursive: true });
await cp(new URL("target/release/simplet-native", root), new URL("MacOS/simplet-native", bundle));
await cp(new URL("../src-tauri/icons/icon.icns", root), new URL("Resources/icon.icns", bundle));
await writeFile(new URL("Info.plist", bundle), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>simplet-native</string>
<key>CFBundleIdentifier</key><string>com.etherzhang.simplet.native-preview</string>
<key>CFBundleName</key><string>SimpleT Native</string>
<key>CFBundleDisplayName</key><string>SimpleT Native</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.2.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>\n`);
run("/usr/bin/codesign", ["--force", "--sign", "-", fileURLToPath(new URL("../", bundle))]);
console.log(fileURLToPath(new URL("../", bundle)));
