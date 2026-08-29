// SPDX-License-Identifier: LGPL-3.0-only

import { execFileSync, spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const ROOT_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../..')

export function fail(message) {
  throw new Error(message)
}

export function readJson(rootDir, relativePath) {
  return JSON.parse(readFileSync(join(rootDir, relativePath), 'utf8'))
}

export function runGit(args, cwd = ROOT_DIR) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim()
}

export function gitSucceeds(args, cwd = ROOT_DIR) {
  return spawnSync('git', args, { cwd, encoding: 'utf8', stdio: 'pipe' }).status === 0
}

export function resolveCommit(reference, cwd = ROOT_DIR) {
  try {
    return runGit(['rev-parse', '--verify', `${reference}^{commit}`], cwd)
  } catch {
    fail(`cannot resolve ${reference} to a commit`)
  }
}
