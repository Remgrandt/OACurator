// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { existsSync, readFileSync } from "node:fs";

const lockPath = "scripts/libvips-runtime-lock.json";
const windowsInstallerPath = "scripts/install-libvips-windows.ps1";
const macosInstallerPath = "scripts/install-libvips-macos.sh";

const sha256Pattern = /^[a-f0-9]{64}$/;

function readRequiredText(path) {
  if (!existsSync(path)) {
    throw new Error(`Missing required file: ${path}`);
  }

  return readFileSync(path, "utf8");
}

function requireString(value, label) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${label} must be a non-empty string.`);
  }

  return value;
}

function requireSha256(value, label) {
  const normalized = requireString(value, label).toLowerCase();
  if (!sha256Pattern.test(normalized)) {
    throw new Error(`${label} must be a 64-character lowercase SHA-256 hex digest.`);
  }

  return normalized;
}

function requireContains(text, needle, label) {
  if (!text.includes(needle)) {
    throw new Error(`${label} must include ${needle}.`);
  }
}

const lock = JSON.parse(readRequiredText(lockPath));

if (lock.notice !== "Copyright (c) 2026 Remgrandt Works. All rights reserved.") {
  throw new Error("Runtime lock must carry the Remgrandt Works copyright notice.");
}

if (lock.formatVersion !== 1) {
  throw new Error("Runtime lock formatVersion must be 1.");
}

const windows = lock.windows;
requireString(windows?.libvipsVersion, "windows.libvipsVersion");
const windowsAsset = requireString(windows?.assetName, "windows.assetName");
const windowsUrl = requireString(windows?.url, "windows.url");
requireSha256(windows?.sha256, "windows.sha256");

if (!windowsAsset.includes(windows.libvipsVersion)) {
  throw new Error("windows.assetName must include windows.libvipsVersion.");
}

if (!windowsUrl.endsWith(`/${windowsAsset}`)) {
  throw new Error("windows.url must end with windows.assetName.");
}

const macos = lock.macos;
requireString(macos?.sharpLibvipsVersion, "macos.sharpLibvipsVersion");

for (const [arch, expectedPackage] of [
  ["arm64", "@img/sharp-libvips-darwin-arm64"],
  ["x86_64", "@img/sharp-libvips-darwin-x64"],
]) {
  const entry = macos?.packages?.[arch];
  const packageName = requireString(entry?.name, `macos.packages.${arch}.name`);
  if (packageName !== expectedPackage) {
    throw new Error(`macos.packages.${arch}.name must be ${expectedPackage}.`);
  }

  requireSha256(entry?.sha256, `macos.packages.${arch}.sha256`);
}

const windowsInstaller = readRequiredText(windowsInstallerPath);
requireContains(windowsInstaller, "libvips-runtime-lock.json", windowsInstallerPath);
requireContains(windowsInstaller, "Get-FileHash", windowsInstallerPath);
requireContains(windowsInstaller, "SHA256", windowsInstallerPath);

const macosInstaller = readRequiredText(macosInstallerPath);
requireContains(macosInstaller, "libvips-runtime-lock.json", macosInstallerPath);
requireContains(macosInstaller, "shasum -a 256", macosInstallerPath);

console.log("libvips runtime downloads are pinned by repository SHA-256 checks.");
