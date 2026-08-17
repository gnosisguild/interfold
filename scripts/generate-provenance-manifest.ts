#!/usr/bin/env tsx
// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

/**
 * Build the release provenance manifest for the RISC Zero compute guest.
 *
 * `Risc0BfvCiphertextVerifier.imageId` is immutable and names exactly one guest image. A proof
 * tells a verifier which guest ran; it says nothing about which source produced that guest. This
 * manifest is the record that closes that gap, so a third party can start from a released tag and
 * arrive at the deployed image ID.
 *
 * Local fields come from the working tree and the build output. Chain fields need an RPC endpoint
 * and the deployed verifier address; without them the manifest still emits, with those fields null
 * and `complete` false.
 *
 *   pnpm provenance:manifest
 *   pnpm provenance:manifest --rpc https://... --verifier 0x... --out manifest.json
 *
 * The manifest is a record, not a check. `pnpm check:image-id` is the gate; this describes what was
 * built and where it was deployed. Verifying a manifest against a rebuild is
 * `./scripts/check-image-id.sh --rebuild`, and the procedure is documented at
 * docs/pages/verifying-the-compute-provider.mdx.
 */

import { execFileSync } from 'child_process'
import { createHash } from 'crypto'
import fs from 'fs'
import path from 'path'

const REPO_ROOT = path.resolve(__dirname, '..')
const SUPPORT = path.join(REPO_ROOT, 'crates', 'support')
const IMAGE_ID_SOL = path.join(SUPPORT, 'contracts', 'ImageID.sol')
const STAMP = path.join(SUPPORT, 'contracts', 'ImageID.stamp.json')
const DOCKERFILE = path.join(SUPPORT, 'Dockerfile')

interface Args {
  rpc?: string
  verifier?: string
  out?: string
}

function parseArgs(argv: string[]): Args {
  const args: Args = {}
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i]
    const value = argv[i + 1]
    if (flag === '--rpc' || flag === '--verifier' || flag === '--out') {
      if (!value || value.startsWith('--')) {
        throw new Error(`${flag} needs a value`)
      }
      args[flag.slice(2) as keyof Args] = value
      i += 1
    } else {
      throw new Error(`unknown argument '${flag}'`)
    }
  }
  if (args.rpc && !args.verifier) throw new Error('--rpc also needs --verifier')
  if (args.verifier && !args.rpc) throw new Error('--verifier also needs --rpc')
  return args
}

function sh(command: string, commandArgs: string[]): string | null {
  try {
    return execFileSync(command, commandArgs, {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
  } catch {
    return null
  }
}

function readIfPresent(file: string): string | null {
  return fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : null
}

function sha256File(file: string): string | null {
  if (!fs.existsSync(file)) return null
  return createHash('sha256').update(fs.readFileSync(file)).digest('hex')
}

function firstMatch(text: string | null, pattern: RegExp): string | null {
  if (!text) return null
  const match = text.match(pattern)
  return match ? match[1] : null
}

/** Collects every Interfold git pin the guest workspace reads, so a split pin is visible. */
function pinnedRevisions(): Record<string, string[]> {
  const manifests = [path.join(SUPPORT, 'Cargo.toml'), path.join(SUPPORT, 'methods', 'guest', 'Cargo.toml')]
  const pins: Record<string, string[]> = {}
  for (const manifest of manifests) {
    const text = readIfPresent(manifest)
    if (!text) continue
    const found = [...text.matchAll(/rev = "([0-9a-f]{40})"/g)].map((m) => m[1])
    pins[path.relative(REPO_ROOT, manifest)] = [...new Set(found)]
  }
  return pins
}

/**
 * Resolves the RISC Zero guest builder image.
 *
 * The builder is selected by a mutable tag, and `RISC0_DOCKER_CONTAINER_TAG` overrides it, so the
 * tag alone does not identify a build. Record the resolved digest whenever Docker can supply it.
 */
function builderImage(risc0Version: string | null) {
  const tag = process.env.RISC0_DOCKER_CONTAINER_TAG ?? (risc0Version ? `risczero/risc0-guest-builder:v${risc0Version}` : null)
  if (!tag) return { tag: null, digest: null }
  const digest = sh('docker', ['image', 'inspect', '--format', '{{index .RepoDigests 0}}', tag])
  return { tag, digest }
}

/** Reads the guest ELF path out of the generated Elf.sol, which is not committed. */
function guestElf() {
  const elfSol = readIfPresent(path.join(SUPPORT, 'tests', 'Elf.sol'))
  const elfPath = firstMatch(elfSol, /"([^"]+program\.bin)"/)
  if (!elfPath) {
    return { path: null, sha256: null, note: 'Elf.sol absent; build the guest first' }
  }
  const sha256 = sha256File(elfPath)
  return {
    path: elfPath,
    sha256,
    note: sha256 ? null : 'Elf.sol names a path that does not exist on this machine',
  }
}

async function rpcCall(rpc: string, method: string, params: unknown[]): Promise<string | null> {
  try {
    const response = await fetch(rpc, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    })
    const body = (await response.json()) as { result?: string; error?: unknown }
    if (body.error || typeof body.result !== 'string') return null
    return body.result
  } catch {
    return null
  }
}

/** `keccak256("imageId()")[0..4]` and `keccak256("risc0Verifier()")[0..4]`. */
const SELECTOR_IMAGE_ID = '0xef3f7dd5'
const SELECTOR_RISC0_VERIFIER = '0x5c9770c5'

async function chainFacts(rpc: string, verifier: string) {
  const code = await rpcCall(rpc, 'eth_getCode', [verifier, 'latest'])
  // SHA-256, not keccak256: Node's crypto has no keccak256, and any fixed digest serves the
  // purpose here as long as the manifest names which one it is.
  const runtimeCodeSha256 =
    code && code !== '0x'
      ? `0x${createHash('sha256')
          .update(Buffer.from(code.slice(2), 'hex'))
          .digest('hex')}`
      : null

  const onchainImageId = await rpcCall(rpc, 'eth_call', [{ to: verifier, data: SELECTOR_IMAGE_ID }, 'latest'])
  const underlying = await rpcCall(rpc, 'eth_call', [{ to: verifier, data: SELECTOR_RISC0_VERIFIER }, 'latest'])
  const chainId = await rpcCall(rpc, 'eth_chainId', [])

  return {
    chainId: chainId ? Number.parseInt(chainId, 16) : null,
    ciphertextVerifier: verifier,
    ciphertextVerifierRuntimeCodeSha256: runtimeCodeSha256,
    ciphertextVerifierCodePresent: Boolean(code && code !== '0x'),
    underlyingRisc0Verifier: underlying ? `0x${underlying.slice(-40)}` : null,
    onchainImageId,
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2))

  const dockerfile = readIfPresent(DOCKERFILE)
  const risc0Version = firstMatch(dockerfile, /^ARG RISC0_VERSION=(.*)$/m)
  const risc0Toolchain = firstMatch(dockerfile, /^ARG RISC0_TOOLCHAIN=(.*)$/m)
  const stamp = readIfPresent(STAMP)
  const committedImageId = firstMatch(readIfPresent(IMAGE_ID_SOL), /(0x[0-9a-fA-F]{64})/)

  const elf = guestElf()
  const chain = args.rpc && args.verifier ? await chainFacts(args.rpc, args.verifier) : null

  const manifest = {
    schema: 'interfold.compute-provider-provenance/1',
    generatedFrom: {
      sourceCommit: sh('git', ['rev-parse', 'HEAD']),
      sourceDescribe: sh('git', ['describe', '--tags', '--always', '--dirty']),
      treeClean: sh('git', ['status', '--porcelain']) === '',
    },
    build: {
      risc0Version,
      risc0GuestToolchain: risc0Toolchain,
      hostToolchain: firstMatch(readIfPresent(path.join(REPO_ROOT, 'rust-toolchain.toml')), /channel = "([^"]+)"/),
      builderImage: builderImage(risc0Version),
      pinnedRevisions: pinnedRevisions(),
      lockfiles: {
        'crates/support/Cargo.lock': sha256File(path.join(SUPPORT, 'Cargo.lock')),
        'crates/support/methods/guest/Cargo.lock': sha256File(path.join(SUPPORT, 'methods', 'guest', 'Cargo.lock')),
      },
    },
    guest: {
      elfPath: elf.path,
      elfSha256: elf.sha256,
      elfNote: elf.note,
      // The SHA-256 of the ELF is a binary integrity check. It is NOT the image ID, which is
      // computed from the loaded memory image. Both are recorded; neither substitutes for the other.
      imageId: committedImageId,
      imageIdVerified: stamp ? !/"imageIdVerified"\s*:\s*false/.test(stamp) : null,
      guestInputsDigest: firstMatch(stamp, /"guestInputsDigest"\s*:\s*"([0-9a-f]{64})"/),
    },
    deployment: chain,
  }

  const unresolved: string[] = []
  if (!manifest.guest.elfSha256) unresolved.push('guest.elfSha256')
  if (!manifest.build.builderImage.digest) unresolved.push('build.builderImage.digest')
  if (!manifest.guest.imageIdVerified) unresolved.push('guest.imageIdVerified')
  if (!chain) unresolved.push('deployment (pass --rpc and --verifier)')

  const output = { ...manifest, complete: unresolved.length === 0, unresolved }
  const json = `${JSON.stringify(output, null, 2)}\n`

  if (args.out) {
    fs.writeFileSync(path.resolve(REPO_ROOT, args.out), json)
    console.log(`provenance manifest written to ${args.out}`)
  } else {
    process.stdout.write(json)
  }

  if (unresolved.length > 0) {
    console.error(
      `\n⚠️  incomplete manifest. Unresolved: ${unresolved.join(', ')}\n` +
        `   A release manifest must be complete. See docs/pages/verifying-the-compute-provider.mdx.`,
    )
  }
}

main().catch((error: unknown) => {
  console.error(`generate-provenance-manifest: ${(error as Error).message}`)
  process.exit(1)
})
