// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();

function readWorkflow(name) {
  return readFileSync(join(root, '.github', 'workflows', name), 'utf8');
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function assertIncludes(text, needle, description) {
  assert(text.includes(needle), `Expected ${description} to include: ${needle}`);
}

const windowsRelease = readWorkflow('release-windows.yml');
const macosRelease = readWorkflow('release-macos.yml');
const publishStagedRelease = readWorkflow('publish-staged-release.yml');
const packageJson = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));

for (const [name, workflow] of [
  ['Windows Release', windowsRelease],
  ['macOS Release', macosRelease],
]) {
  assertIncludes(workflow, 'workflow_dispatch:', `${name} workflow`);
  assertIncludes(workflow, 'draft:', `${name} workflow`);
  assertIncludes(workflow, 'default: true', `${name} workflow draft input`);
  assertIncludes(workflow, 'gh release', `${name} workflow`);
}

for (const scriptName of ['release:macos', 'release:macos:arm64', 'release:macos:x64']) {
  assertIncludes(
    packageJson.scripts[scriptName],
    '--bundles app,dmg',
    `${scriptName} package script`,
  );
}

assertIncludes(
  publishStagedRelease,
  'name: Publish Staged Release',
  'Publish Staged Release workflow',
);
assertIncludes(
  publishStagedRelease,
  'workflow_dispatch:',
  'Publish Staged Release workflow',
);
assertIncludes(
  publishStagedRelease,
  'gh release edit "$tag" --repo "$GITHUB_REPOSITORY" --draft=false',
  'Publish Staged Release workflow',
);
assertIncludes(
  publishStagedRelease,
  'if not release.get("isDraft"):',
  'Publish Staged Release draft guard',
);
assertIncludes(
  publishStagedRelease,
  'is not a draft.',
  'Publish Staged Release draft guard message',
);
assertIncludes(
  publishStagedRelease,
  'required_platforms',
  'Publish Staged Release updater manifest validation',
);

for (const forbidden of [
  'gh release create',
  'gh release upload',
  'npm run',
  'tauri',
  'cargo ',
  'actions/checkout',
  'actions/setup-node',
  'dtolnay/rust-toolchain',
  'azure/login',
  'artifact-signing-action',
  'notarytool',
]) {
  assert(
    !publishStagedRelease.includes(forbidden),
    `Publish Staged Release workflow must not rebuild or upload assets directly; found ${forbidden}`,
  );
}

console.log('Release workflow staging checks passed.');
