// SPDX-License-Identifier: LGPL-3.0-only

import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { ROOT_DIR, fail, readJson } from './core.mjs'

export function verifyPublications(environment = process.env) {
  const required = [
    ['ciphernode image', environment.CIPHERNODE_RESULT],
    ['e3-support image', environment.SUPPORT_RESULT],
    ['npm packages', environment.NPM_RESULT],
  ]

  for (const [name, result] of required) {
    if (result !== 'success') {
      fail(`${name} publication did not succeed: ${result || 'missing result'}`)
    }
  }

  const expectedDappnodeResult = environment.IS_PRERELEASE === 'true' ? 'skipped' : 'success'
  if (environment.DAPPNODE_RESULT !== expectedDappnodeResult) {
    fail(`DAppNode result is ${environment.DAPPNODE_RESULT}, expected ${expectedDappnodeResult}`)
  }

  console.log('Every required release publication succeeded.')
}

function jobSource(workflow, jobName) {
  const start = workflow.indexOf(`\n  ${jobName}:`)
  if (start < 0) {
    fail(`release workflow does not contain ${jobName}`)
  }
  const next = workflow.slice(start + 1).search(/\n  [A-Za-z0-9_-]+:/)
  return next < 0 ? workflow.slice(start) : workflow.slice(start, start + 1 + next)
}

function requireText(source, text, message) {
  if (!source.includes(text)) {
    fail(message)
  }
}

export function checkReleaseSafeguards(rootDir = ROOT_DIR) {
  const ci = readFileSync(join(rootDir, '.github/workflows/ci.yml'), 'utf8')
  const release = readFileSync(join(rootDir, '.github/workflows/releases.yml'), 'utf8')
  const bump = readFileSync(join(rootDir, 'scripts/bump-versions.ts'), 'utf8')
  const packageJson = readJson(rootDir, 'package.json')

  requireText(ci, 'workflow_call:', 'the release workflow cannot call CI')
  requireText(
    ci,
    `FORCE="\${{ github.event_name == 'workflow_dispatch' || inputs.release_candidate }}"`,
    'release candidate CI does not force every path-filtered job',
  )
  requireText(release, 'uses: ./.github/workflows/ci.yml', 'the release does not test its exact tagged commit')
  requireText(release, 'release_candidate: true', 'the release does not run complete CI')
  requireText(release, 'node scripts/release.mjs prepare', 'the release tag is not bound to main history')
  requireText(release, 'group: release-promotion-${{ github.repository }}', 'release promotions can overlap')

  for (const unsafeText of ['continue-on-error: true', 'cachix/cachix-action', 'cargo workspaces publish']) {
    if (release.includes(unsafeText)) {
      fail(`release workflow contains unsafe operation: ${unsafeText}`)
    }
  }

  for (const publisher of ['build-ciphernode-image-release', 'build-e3-support-release', 'publish-npm-packages']) {
    requireText(jobSource(release, publisher), 'release-candidate-gate', `${publisher} can publish before candidate qualification`)
  }

  requireText(release, 'Required circuit-artifacts branch is missing', 'missing circuit artifacts do not fail closed')
  requireText(release, 'if-no-files-found: error', 'missing release archives do not fail artifact upload')
  requireText(release, 'node scripts/release.mjs publish-npm', 'npm publication cannot resume safely')

  const createRelease = jobSource(release, 'create-github-release')
  requireText(createRelease, 'publication-gate', 'GitHub release creation does not require all publications')
  if (createRelease.includes('always()')) {
    fail('GitHub release creation can bypass a failed dependency')
  }
  if (createRelease.indexOf('- name: Create GitHub Release') < createRelease.indexOf('- name: Update stable tag')) {
    fail('the stable release is created before its aliases and stable tag are promoted')
  }

  if (/git tag (?:-a|--annotate)|git push .*refs\/tags/.test(bump)) {
    fail('the version bump script still creates or pushes a release tag')
  }
  requireText(bump, 'this.checkReleaseBranch()', 'the version bump can commit directly on a protected branch')

  if (packageJson.scripts['release:tag'] !== 'node scripts/release.mjs tag') {
    fail('release:tag does not use the protected release command')
  }

  console.log('Release promotion safeguards are fail closed.')
}
