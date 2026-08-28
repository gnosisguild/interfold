#!/usr/bin/env tsx
// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { execSync } from 'child_process'
import { createHash } from 'crypto'
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'fs'
import { join, relative, resolve } from 'path'

const BRANCH = 'circuit-artifacts'
const ROOT = resolve(__dirname, '..')
const DIST = join(ROOT, 'dist', 'circuits')
const METADATA_FILES = new Set(['.git', 'SOURCE_HASH', 'SHA256SUMS', 'checksums.json'])

const run = (cmd: string, cwd = ROOT) => execSync(cmd, { encoding: 'utf-8', cwd, stdio: 'pipe' }).trim()
const runV = (cmd: string, cwd = ROOT) => execSync(cmd, { cwd, stdio: 'inherit' })

function copyArtifactsInto(target: string): void {
  for (const preset of readdirSync(DIST)) {
    if (METADATA_FILES.has(preset)) continue
    const presetPath = join(DIST, preset)
    if (!statSync(presetPath).isDirectory()) continue

    for (const committee of readdirSync(presetPath)) {
      const localPair = join(presetPath, committee)
      if (!statSync(localPair).isDirectory()) continue

      const remotePair = join(target, preset, committee)
      if (existsSync(remotePair)) rmSync(remotePair, { recursive: true })
      mkdirSync(join(target, preset), { recursive: true })
      cpSync(localPair, remotePair, { recursive: true })
    }
  }
}

function artifactFiles(dir: string, base = dir): string[] {
  const files: string[] = []
  for (const entry of readdirSync(dir)) {
    if (METADATA_FILES.has(entry)) continue
    const full = join(dir, entry)
    const stat = statSync(full)
    if (stat.isDirectory()) files.push(...artifactFiles(full, base))
    else if (stat.isFile()) files.push(relative(base, full))
  }
  return files.sort()
}

function refreshChecksums(dir: string): void {
  const sums: Record<string, string> = {}
  const lines: string[] = []

  for (const file of artifactFiles(dir)) {
    const hash = createHash('sha256')
      .update(readFileSync(join(dir, file)))
      .digest('hex')
    sums[file] = hash
    lines.push(`${hash}  ${file}`)
  }

  writeFileSync(join(dir, 'SHA256SUMS'), lines.join('\n') + '\n')
  writeFileSync(
    join(dir, 'checksums.json'),
    JSON.stringify({ algorithm: 'sha256', generated: new Date().toISOString(), files: sums }, null, 2) + '\n',
  )
}

function stampFiles(dir: string): string[] {
  const stamps: string[] = []
  for (const file of artifactFiles(dir)) {
    if (file.endsWith('.build-stamp.json')) stamps.push(file)
  }
  return stamps
}

function requiredArtifactMarkers(preset: string, committee: string): string[] {
  return [
    join(preset, committee, 'default/dkg/pk/pk.json'),
    join(preset, committee, 'default/threshold/pk_aggregation/pk_aggregation.json'),
    join(preset, committee, 'default/recursive_aggregation/dkg_aggregator/dkg_aggregator.json'),
    join(preset, committee, 'default/recursive_aggregation/decryption_aggregator/decryption_aggregator.json'),
  ]
}

function validateRetainedStamps(dir: string): void {
  for (const stampFile of stampFiles(dir)) {
    const stampPath = join(dir, stampFile)
    const stamp = JSON.parse(readFileSync(stampPath, 'utf8')) as {
      preset?: string
      committee?: string
      sourceHash?: string
    }
    if (!stamp.preset || !stamp.committee || !stamp.sourceHash) {
      throw new Error(`Invalid circuit build stamp: ${stampFile}`)
    }
    const expected = run(`pnpm tsx scripts/build-circuits.ts hash --preset ${stamp.preset} --committee ${stamp.committee}`)
    if (stamp.sourceHash !== expected) {
      throw new Error(
        `Stale circuit artifacts at ${stamp.preset}/${stamp.committee}: ` +
          `stamp=${stamp.sourceHash}, expected=${expected}. ` +
          `Rebuild that pair before pushing.`,
      )
    }
    const missing = requiredArtifactMarkers(stamp.preset, stamp.committee).filter((marker) => !existsSync(join(dir, marker)))
    if (missing.length > 0) {
      throw new Error(
        `Incomplete circuit artifacts at ${stamp.preset}/${stamp.committee}: ` + `missing ${missing[0]}. Rebuild that pair before pushing.`,
      )
    }
  }
}

async function push() {
  if (!existsSync(DIST)) {
    console.error('❌ No artifacts. Run: pnpm build:circuits')
    process.exit(1)
  }

  const replace = process.argv.includes('--replace')
  const hash = run('pnpm tsx scripts/build-circuits.ts hash')
  const remote = run('git remote get-url origin')
  const tmp = join(ROOT, '.tmp-circuits')

  if (existsSync(tmp)) rmSync(tmp, { recursive: true })

  const branchExists = run(`git ls-remote --heads origin ${BRANCH}`).includes(BRANCH)

  if (branchExists) {
    runV(`git clone --depth 1 --branch ${BRANCH} --single-branch ${remote} ${tmp}`)
    if (replace) {
      for (const f of readdirSync(tmp)) if (f !== '.git') rmSync(join(tmp, f), { recursive: true })
    }
  } else {
    mkdirSync(tmp)
    run('git init', tmp)
    run(`git remote add origin ${remote}`, tmp)
    run(`git checkout -b ${BRANCH}`, tmp)
  }

  copyArtifactsInto(tmp)
  validateRetainedStamps(tmp)
  writeFileSync(join(tmp, 'SOURCE_HASH'), hash)
  refreshChecksums(tmp)

  run('git add -A', tmp)
  try {
    run(`git commit -m "circuits: ${hash}"`, tmp)
  } catch {
    console.log('✅ No changes')
    rmSync(tmp, { recursive: true })
    return
  }
  runV(`git push origin ${BRANCH}`, tmp)
  console.log(`✅ Pushed (${hash})`)

  rmSync(tmp, { recursive: true })
}

async function pull() {
  try {
    run(`git fetch origin ${BRANCH}`)
  } catch (e: any) {
    const isNetworkError =
      e.message?.includes('Could not resolve host') || e.message?.includes('unable to access') || e.message?.includes('Connection refused')
    if (isNetworkError) {
      console.error('❌ Network error fetching branch')
    } else {
      console.error(`❌ Branch '${BRANCH}' not found`)
    }
    process.exit(1)
  }

  if (existsSync(DIST)) rmSync(DIST, { recursive: true })
  mkdirSync(DIST, { recursive: true })

  runV(`git archive origin/${BRANCH} | tar -x -C "${DIST}"`)
  console.log(`✅ Pulled to ${DIST}`)
}

const cmd = process.argv[2]
if (cmd === 'push') push()
else if (cmd === 'pull') pull()
else console.log('Usage: circuit-artifacts.ts [push [--replace]|pull]')
