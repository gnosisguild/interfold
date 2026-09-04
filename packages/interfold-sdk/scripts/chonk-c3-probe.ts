// SPDX-License-Identifier: LGPL-3.0-only

import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { gunzipSync } from 'node:zlib'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { AztecClientBackend, Barretenberg, deflattenFields, UltraHonkBackend } from '@aztec/bb.js'
import { Noir } from '@noir-lang/noir_js'

type CompiledCircuit = {
  bytecode: string
}

type ProofFixture = {
  proof: string[]
  publicInputs: string[]
}

type LeafFixtureInput = {
  slotIndices: number[]
  c3a: ProofFixture[]
  c3b: ProofFixture[]
}

type TubeFixture = {
  proofFields: string[]
  publicInputs: string[]
  verificationKey: string[]
  keyHash: string
}

type TubeFixtureOutput = {
  slotIndices: number[]
  c3a: TubeFixture[]
  c3b: TubeFixture[]
}

type LeafChonkCircuits = {
  app: CompiledCircuit
  init: CompiledCircuit
  inner: CompiledCircuit
  tail: CompiledCircuit
  hiding: CompiledCircuit
}

type LeafChonkNoirs = {
  app: Noir
  init: Noir
  inner: Noir
  tail: Noir
  hiding: Noir
}

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '../../..')
const probeRoot = resolve(repoRoot, 'circuits/benchmarks/chonk_c3_probe')
const leafDir = resolve(repoRoot, 'circuits/bin/dkg/share_encryption')
const dkgTargetDir = resolve(repoRoot, 'circuits/bin/dkg/target')

type Committee = 'minimum' | 'micro' | 'small'

const committee = process.env.CHONK_C3_COMMITTEE ?? 'minimum'
const committeeParties: Record<Committee, number> = {
  minimum: 3,
  micro: 9,
  small: 19,
}
if (!(committee in committeeParties)) {
  throw new Error(`Unsupported CHONK_C3_COMMITTEE=${committee}; expected minimum, micro, or small`)
}

const C3_MODULI = 2
const C3_SLOTS = committeeParties[committee as Committee] * C3_MODULI
const C3_SLOTS_PER_PARTY = C3_MODULI
const C3_LEAF_COUNT = C3_SLOTS - C3_SLOTS_PER_PARTY
const C3_BATCH_SIZE = C3_LEAF_COUNT / 2
const C3_CHUNK_COUNT = C3_LEAF_COUNT / C3_BATCH_SIZE
const C3_LEAF_STACK_SIZE = 9
const C3_LEAF_STACK_COUNT = C3_LEAF_COUNT / C3_LEAF_STACK_SIZE
if (!Number.isInteger(C3_BATCH_SIZE) || !Number.isInteger(C3_CHUNK_COUNT)) {
  throw new Error(`C3 leaf count must split evenly into Chonk batches: ${C3_LEAF_COUNT}`)
}
if (committee === 'small' && !Number.isInteger(C3_LEAF_STACK_COUNT)) {
  throw new Error(`C3 leaf count must split evenly into leaf Chonk stacks: ${C3_LEAF_COUNT}`)
}
const C3_ACCUMULATOR_LEN = 3 * C3_SLOTS
const C3_FOLD_PREFIX_LEN = 6
const C3_FOLD_PUBLIC_LEN = C3_FOLD_PREFIX_LEN + C3_ACCUMULATOR_LEN
const CHONK_VK_LENGTH_IN_FIELDS = 115
const CHONK_PROOF_LENGTH = 1270
const ROLLUP_HONK_PROOF_LENGTH = 480
const SLOT_INDICES = Array.from({ length: C3_LEAF_COUNT }, (_, index) => C3_SLOTS_PER_PARTY + index)
const chonkMode = process.env.CHONK_C3_MODE ?? 'batch'
if (chonkMode !== 'batch' && chonkMode !== 'leaf') {
  throw new Error(`Unsupported CHONK_C3_MODE=${chonkMode}; expected batch or leaf`)
}
if (chonkMode === 'leaf' && committee !== 'small') {
  throw new Error('CHONK_C3_MODE=leaf currently requires the Small committee artifacts')
}

function run(command: string, args: string[], cwd: string, inherit = false): void {
  execFileSync(command, args, { cwd, stdio: inherit ? 'inherit' : 'ignore' })
}

function loadCircuit(name: string): CompiledCircuit {
  const path = resolve(probeRoot, name, 'target', `chonk_${name}.json`)
  return JSON.parse(readFileSync(path, 'utf8')) as CompiledCircuit
}

function loadBytes(path: string): Uint8Array {
  return new Uint8Array(readFileSync(path))
}

function fieldToHex(field: Uint8Array): string {
  return `0x${Buffer.from(field).toString('hex').padStart(64, '0')}`
}

function fieldsFromBytes(bytes: Uint8Array): string[] {
  if (bytes.length % 32 !== 0) throw new Error(`Expected field-aligned bytes, got ${bytes.length} bytes`)
  const fields: string[] = []
  for (let i = 0; i < bytes.length; i += 32) fields.push(fieldToHex(bytes.slice(i, i + 32)))
  return fields
}

function numberToField(value: number): string {
  return `0x${value.toString(16).padStart(64, '0')}`
}

function canonicalField(value: string): string {
  return `0x${BigInt(value).toString(16).padStart(64, '0')}`
}

function flattenReturnValue(value: unknown): string[] {
  if (Array.isArray(value)) return value.flatMap(flattenReturnValue)
  if (typeof value === 'object' && value !== null) {
    return Object.values(value as Record<string, unknown>).flatMap(flattenReturnValue)
  }
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'bigint') return [String(value)]
  throw new Error(`Unsupported Noir return value: ${String(value)}`)
}

function parseTomlArray(source: string, section: string): string[] {
  const match = source.match(new RegExp(`\\[${section}\\]\\s*\\r?\\ncoefficients\\s*=\\s*\\[([^\\]]*)\\]`, 's'))
  if (!match) throw new Error(`Missing coefficients for [${section}] in Prover.toml`)

  return match[1]
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)
    .map((value) => (value.startsWith('"') && value.endsWith('"') ? value.slice(1, -1) : value))
}

function rewriteSlot(source: string, partyIdx: number, modIdx: number, messageCommitment: string): string {
  return source
    .replace(/^expected_message_commitment\s*=\s*.*$/m, `expected_message_commitment = "${messageCommitment}"`)
    .replace(/^mod_idx\s*=\s*.*$/m, `mod_idx = ${modIdx}`)
    .replace(/^party_idx\s*=\s*.*$/m, `party_idx = ${partyIdx}`)
}

function ensureLeafArtifact(): void {
  run('nargo', ['compile'], leafDir)
  run('bb', ['write_vk', '-b', 'share_encryption.json', '-o', '.', '-t', 'noir-recursive'], dkgTargetDir)
  copyFileSync(resolve(dkgTargetDir, 'vk'), resolve(dkgTargetDir, 'share_encryption.vk_noir'))
  copyFileSync(resolve(dkgTargetDir, 'vk_hash'), resolve(dkgTargetDir, 'share_encryption.vk_noir_hash'))
}

function ensureProbeArtifacts(): void {
  const circuits = [
    'c3_commitment',
    'c3_batch_app',
    'c3_init_kernel',
    'c3_tail_kernel',
    'c3_hiding_kernel',
    'c3_leaf_app',
    'c3_leaf_init_kernel',
    'c3_leaf_inner_kernel',
    'c3_leaf_tail_kernel',
    'c3_leaf_hiding_kernel',
    'c3_tube',
    'c3ab_fold',
  ]
  for (const circuit of circuits) run('nargo', ['compile'], resolve(probeRoot, circuit))

  for (const circuit of ['c3_batch_app', 'c3_init_kernel', 'c3_tail_kernel']) {
    const dir = resolve(probeRoot, circuit)
    run('bb', ['write_vk', '--scheme', 'chonk', '-b', `target/chonk_${circuit}.json`, '-o', 'target'], dir)
  }

  for (const circuit of ['c3_leaf_app', 'c3_leaf_init_kernel', 'c3_leaf_inner_kernel', 'c3_leaf_tail_kernel']) {
    const dir = resolve(probeRoot, circuit)
    run('bb', ['write_vk', '--scheme', 'chonk', '-b', `target/chonk_${circuit}.json`, '-o', 'target'], dir)
  }

  const hidingDir = resolve(probeRoot, 'c3_hiding_kernel')
  run('bb', ['write_vk', '--scheme', 'chonk', '--use_zk_flavor', '-b', 'target/chonk_c3_hiding_kernel.json', '-o', 'target'], hidingDir)
  const leafHidingDir = resolve(probeRoot, 'c3_leaf_hiding_kernel')
  run(
    'bb',
    ['write_vk', '--scheme', 'chonk', '--use_zk_flavor', '-b', 'target/chonk_c3_leaf_hiding_kernel.json', '-o', 'target'],
    leafHidingDir,
  )

  if (process.env.CHONK_C3_OUTPUT) {
    const c3abDir = resolve(probeRoot, 'c3ab_fold')
    run('bb', ['write_vk', '-b', 'target/chonk_c3ab_fold.json', '-o', 'target', '-t', 'noir-recursive-no-zk'], c3abDir)
  }
}

async function generateLeafProofs(commitmentCircuit: CompiledCircuit): Promise<ProofFixture[]> {
  const commitmentNoir = new Noir(commitmentCircuit as any)
  const fixtures: ProofFixture[] = []

  for (const slot of SLOT_INDICES) {
    const partyIdx = Math.floor(slot / 2)
    const modIdx = slot % 2

    run(
      'cargo',
      [
        'run',
        '--quiet',
        '-p',
        'e3-zk-helpers',
        '--bin',
        'zk_cli',
        '--',
        '--circuit',
        'share-encryption',
        '--preset',
        'insecure',
        '--committee',
        committee,
        '--output',
        leafDir,
        '--toml',
        '--no-configs',
        '--inputs',
        'secret-key',
      ],
      repoRoot,
      true,
    )

    const proverTomlPath = resolve(leafDir, 'Prover.toml')
    const proverToml = readFileSync(proverTomlPath, 'utf8')
    const message = parseTomlArray(proverToml, 'message')
    const commitmentExecution = await commitmentNoir.execute({
      party_idx: partyIdx,
      mod_idx: modIdx,
      message: { coefficients: message },
    })
    const messageCommitment = flattenReturnValue(commitmentExecution.returnValue)[0]
    if (!messageCommitment) throw new Error(`Commitment helper returned no value for slot ${slot}`)

    writeFileSync(proverTomlPath, rewriteSlot(proverToml, partyIdx, modIdx, messageCommitment))
    run('nargo', ['execute'], leafDir, true)
    run(
      'bb',
      [
        'prove',
        '-b',
        'share_encryption.json',
        '-w',
        'share_encryption.gz',
        '-k',
        'share_encryption.vk_noir',
        '-t',
        'noir-recursive',
        '-o',
        '.',
      ],
      dkgTargetDir,
      true,
    )
    run(
      'bb',
      ['verify', '-k', 'share_encryption.vk_noir', '-p', 'proof', '-i', 'public_inputs', '-t', 'noir-recursive'],
      dkgTargetDir,
      true,
    )

    fixtures.push({
      proof: deflattenFields(loadBytes(resolve(dkgTargetDir, 'proof'))),
      publicInputs: fieldsFromBytes(loadBytes(resolve(dkgTargetDir, 'public_inputs'))),
    })
  }

  return fixtures
}

function loadLeafFixtureInput(): LeafFixtureInput | undefined {
  const path = process.env.CHONK_C3_LEAF_FIXTURES
  if (!path) return undefined

  const input = JSON.parse(readFileSync(path, 'utf8')) as LeafFixtureInput
  if (input.slotIndices.length !== C3_LEAF_COUNT) {
    throw new Error(`Expected ${C3_LEAF_COUNT} C3 slot indices, got ${input.slotIndices.length}`)
  }
  if (
    input.slotIndices.some((slot) => !Number.isInteger(slot) || slot < 0 || slot >= C3_SLOTS) ||
    new Set(input.slotIndices).size !== input.slotIndices.length
  ) {
    throw new Error(`Invalid or duplicate C3 slot indices for ${committee} committee`)
  }
  for (const [label, fixtures] of [
    ['c3a', input.c3a],
    ['c3b', input.c3b],
  ] as const) {
    if (fixtures.length !== C3_LEAF_COUNT) throw new Error(`Expected ${C3_LEAF_COUNT} ${label} leaf proofs`)
    for (const fixture of fixtures) {
      if (fixture.proof.length === 0 || fixture.publicInputs.length !== 5) {
        throw new Error(`Invalid ${label} ShareEncryption fixture`)
      }
    }
  }
  return input
}

async function poseidonHash(api: Barretenberg, fields: string[]): Promise<string> {
  const result = await api.poseidon2Hash({
    inputs: fields.map((field) => {
      const hex = field.startsWith('0x') ? field.slice(2) : field
      const padded = hex.padStart(64, '0')
      const bytes = new Uint8Array(32)
      for (let i = 0; i < bytes.length; i++) bytes[i] = Number.parseInt(padded.slice(i * 2, i * 2 + 2), 16)
      return bytes
    }),
  })
  return fieldToHex(result.hash)
}

async function proveChonkTube(
  api: Barretenberg,
  chonkApi: Barretenberg,
  chonkCircuits: CompiledCircuit[],
  chonkVks: Uint8Array[],
  circuitNames: string[],
  tubeCircuit: CompiledCircuit,
  leafVkFields: string[],
  leafKeyHash: string,
  c3FoldKernelKeyHash: string,
  leaves: ProofFixture[],
  slotIndices: number[],
  appNoir: Noir,
  initNoir: Noir,
  tailNoir: Noir,
  hidingNoir: Noir,
  tubeNoir: Noir,
): Promise<{ fixture: TubeFixture; chonkSeconds: number; tubeSeconds: number }> {
  const appVkFields = fieldsFromBytes(loadBytes(resolve(probeRoot, 'c3_batch_app/target/vk')))
  const initVkFields = fieldsFromBytes(loadBytes(resolve(probeRoot, 'c3_init_kernel/target/vk')))
  const tailVkFields = fieldsFromBytes(loadBytes(resolve(probeRoot, 'c3_tail_kernel/target/vk')))
  const appVk = { key: appVkFields, hash: await poseidonHash(api, appVkFields) }
  const initVk = { key: initVkFields, hash: await poseidonHash(api, initVkFields) }
  const tailVk = { key: tailVkFields, hash: await poseidonHash(api, tailVkFields) }

  const { witness: appWitness, returnValue: appReturnValue } = await appNoir.execute({
    verification_key: leafVkFields,
    key_hash: leafKeyHash,
    proofs: leaves.map((fixture) => fixture.proof),
    public_inputs: leaves.map((fixture) => fixture.publicInputs),
    slot_indices: slotIndices,
  })
  const appAccumulator = flattenReturnValue(appReturnValue)
  if (appAccumulator.length !== C3_ACCUMULATOR_LEN) {
    throw new Error(`Expected ${C3_ACCUMULATOR_LEN} app accumulator fields, got ${appAccumulator.length}`)
  }

  const { witness: initWitness, returnValue: initReturnValue } = await initNoir.execute({
    app_inputs: appAccumulator,
    app_vk: appVk,
  })
  const { witness: tailWitness, returnValue: tailReturnValue } = await tailNoir.execute({
    prev_kernel_inputs: flattenReturnValue(initReturnValue),
    kernel_vk: initVk,
  })
  const { witness: hidingWitness, returnValue: hidingReturnValue } = await hidingNoir.execute({
    prev_kernel_inputs: flattenReturnValue(tailReturnValue),
    kernel_vk: tailVk,
  })

  const hidingOutput = flattenReturnValue(hidingReturnValue)
  if (hidingOutput.length !== C3_ACCUMULATOR_LEN) {
    throw new Error(`Expected ${C3_ACCUMULATOR_LEN} hiding-kernel fields, got ${hidingOutput.length}`)
  }

  const chonkBackend = new AztecClientBackend(chonkCircuits.map(uncompressedBytecode), chonkApi, circuitNames)
  const chonkStarted = performance.now()
  const chonkResult = await chonkBackend.prove(
    [appWitness, initWitness, tailWitness, hidingWitness].map((witness) => gunzipSync(witness)),
    chonkVks,
  )
  const chonkSeconds = (performance.now() - chonkStarted) / 1000
  if (!(await chonkBackend.verify(chonkResult.proof, chonkResult.vk))) throw new Error('Chonk proof did not verify')

  const chonkProofFields = chonkResult.proofFields.map(fieldToHex)
  if (chonkProofFields.length !== C3_ACCUMULATOR_LEN + CHONK_PROOF_LENGTH) {
    throw new Error(`Unexpected Chonk field count: ${chonkProofFields.length}`)
  }
  const chonkPublicInputs = chonkProofFields.slice(0, C3_ACCUMULATOR_LEN)
  const chonkProof = chonkProofFields.slice(C3_ACCUMULATOR_LEN)
  const chonkVkFields = fieldsFromBytes(chonkResult.vk)
  if (chonkVkFields.length !== CHONK_VK_LENGTH_IN_FIELDS) {
    throw new Error(`Expected ${CHONK_VK_LENGTH_IN_FIELDS} Chonk VK fields, got ${chonkVkFields.length}`)
  }
  const chonkKeyHash = await poseidonHash(api, chonkVkFields)
  if (hidingOutput.some((value, index) => canonicalField(value) !== canonicalField(chonkPublicInputs[index]))) {
    throw new Error('Hiding-kernel and Chonk accumulator values differ')
  }

  const c3PublicInputs = [
    leafKeyHash,
    '',
    numberToField(0),
    numberToField(slotIndices[slotIndices.length - 1]),
    c3FoldKernelKeyHash,
    '',
    ...chonkPublicInputs,
  ]

  const tubeBackend = new UltraHonkBackend(tubeCircuit.bytecode, api)
  const tubeVk = await tubeBackend.getVerificationKey({ verifierTarget: 'noir-rollup-no-zk' })
  const tubeVkFields = fieldsFromBytes(tubeVk)
  const tubeKeyHash = await poseidonHash(api, tubeVkFields)
  c3PublicInputs[1] = tubeKeyHash
  c3PublicInputs[5] = tubeKeyHash
  if (c3PublicInputs.length !== C3_FOLD_PUBLIC_LEN) {
    throw new Error(`Expected ${C3_FOLD_PUBLIC_LEN} tube public inputs, got ${c3PublicInputs.length}`)
  }

  const { witness: tubeWitness } = await tubeNoir.execute({
    verification_key: chonkVkFields,
    proof: chonkProof,
    chonk_public_inputs: chonkPublicInputs,
    key_hash: chonkKeyHash,
    c3_public_inputs: c3PublicInputs,
  })
  const tubeStarted = performance.now()
  const tubeProof = await tubeBackend.generateProof(tubeWitness, { verifierTarget: 'noir-rollup-no-zk' })
  const tubeSeconds = (performance.now() - tubeStarted) / 1000
  if (!(await tubeBackend.verifyProof(tubeProof, { verifierTarget: 'noir-rollup-no-zk' }))) {
    throw new Error('C3 tube proof did not verify')
  }
  if (fieldsFromBytes(tubeProof.proof).length !== ROLLUP_HONK_PROOF_LENGTH) {
    throw new Error(`Expected ${ROLLUP_HONK_PROOF_LENGTH} rollup proof fields, got ${fieldsFromBytes(tubeProof.proof).length}`)
  }
  if (tubeProof.publicInputs.length !== C3_FOLD_PUBLIC_LEN) {
    throw new Error(`Expected ${C3_FOLD_PUBLIC_LEN} tube proof public inputs, got ${tubeProof.publicInputs.length}`)
  }

  return {
    fixture: {
      proofFields: fieldsFromBytes(tubeProof.proof),
      publicInputs: tubeProof.publicInputs,
      verificationKey: tubeVkFields,
      keyHash: tubeKeyHash,
    },
    chonkSeconds,
    tubeSeconds,
  }
}

async function proveChonkLeafTube(
  api: Barretenberg,
  chonkApi: Barretenberg,
  circuits: LeafChonkCircuits,
  tubeCircuit: CompiledCircuit,
  leafVkFields: string[],
  leafKeyHash: string,
  c3FoldKernelKeyHash: string,
  leaves: ProofFixture[],
  slotIndices: number[],
  noirs: LeafChonkNoirs,
  tubeNoir: Noir,
): Promise<{ fixture: TubeFixture; chonkSeconds: number; tubeSeconds: number }> {
  if (leaves.length !== C3_LEAF_STACK_SIZE || slotIndices.length !== C3_LEAF_STACK_SIZE) {
    throw new Error(`Leaf Chonk stack expects ${C3_LEAF_STACK_SIZE} leaves and slots`)
  }

  const appVkFields = fieldsFromBytes(loadBytes(resolve(probeRoot, 'c3_leaf_app/target/vk')))
  const initVkFields = fieldsFromBytes(loadBytes(resolve(probeRoot, 'c3_leaf_init_kernel/target/vk')))
  const innerVkFields = fieldsFromBytes(loadBytes(resolve(probeRoot, 'c3_leaf_inner_kernel/target/vk')))
  const tailVkFields = fieldsFromBytes(loadBytes(resolve(probeRoot, 'c3_leaf_tail_kernel/target/vk')))
  const appVk = { key: appVkFields, hash: await poseidonHash(api, appVkFields) }
  const initVk = { key: initVkFields, hash: await poseidonHash(api, initVkFields) }
  const innerVk = { key: innerVkFields, hash: await poseidonHash(api, innerVkFields) }
  const tailVk = { key: tailVkFields, hash: await poseidonHash(api, tailVkFields) }

  const appVkBytes = loadBytes(resolve(probeRoot, 'c3_leaf_app/target/vk'))
  const initVkBytes = loadBytes(resolve(probeRoot, 'c3_leaf_init_kernel/target/vk'))
  const innerVkBytes = loadBytes(resolve(probeRoot, 'c3_leaf_inner_kernel/target/vk'))
  const tailVkBytes = loadBytes(resolve(probeRoot, 'c3_leaf_tail_kernel/target/vk'))
  const hidingVkBytes = loadBytes(resolve(probeRoot, 'c3_leaf_hiding_kernel/target/vk'))

  const stackCircuits: CompiledCircuit[] = []
  const stackVks: Uint8Array[] = []
  const circuitNames: string[] = []
  const witnesses: Uint8Array[] = []
  const addStackStep = (name: string, circuit: CompiledCircuit, vk: Uint8Array, witness: Uint8Array): void => {
    circuitNames.push(name)
    stackCircuits.push(circuit)
    stackVks.push(vk)
    witnesses.push(gunzipSync(witness))
  }

  let previousKernelReturnValue: any
  for (let i = 0; i < C3_LEAF_STACK_SIZE; i++) {
    const appExecution = await noirs.app.execute({
      verification_key: leafVkFields,
      key_hash: leafKeyHash,
      proof: leaves[i].proof,
      public_inputs: leaves[i].publicInputs,
      slot_index: slotIndices[i],
    })
    const appOutput = flattenReturnValue(appExecution.returnValue)
    if (appOutput.length !== 4) throw new Error(`Expected four C3 leaf return fields, got ${appOutput.length}`)
    addStackStep('chonk_c3_leaf_app', circuits.app, appVkBytes, appExecution.witness)

    if (i === 0) {
      const initExecution = await noirs.init.execute({
        app_inputs: appExecution.returnValue,
        app_vk: appVk,
      })
      addStackStep('chonk_c3_leaf_init_kernel', circuits.init, initVkBytes, initExecution.witness)
      previousKernelReturnValue = initExecution.returnValue
    } else {
      const innerExecution = await noirs.inner.execute({
        prev_kernel_inputs: previousKernelReturnValue,
        kernel_vk: i === 1 ? initVk : innerVk,
        app_inputs: appExecution.returnValue,
        app_vk: appVk,
      })
      addStackStep('chonk_c3_leaf_inner_kernel', circuits.inner, innerVkBytes, innerExecution.witness)
      previousKernelReturnValue = innerExecution.returnValue
    }
  }

  const tailExecution = await noirs.tail.execute({
    prev_kernel_inputs: previousKernelReturnValue,
    kernel_vk: innerVk,
  })
  addStackStep('chonk_c3_leaf_tail_kernel', circuits.tail, tailVkBytes, tailExecution.witness)

  const hidingExecution = await noirs.hiding.execute({
    prev_kernel_inputs: tailExecution.returnValue,
    kernel_vk: tailVk,
  })
  addStackStep('chonk_c3_leaf_hiding_kernel', circuits.hiding, hidingVkBytes, hidingExecution.witness)

  const expectedOutput = flattenReturnValue(hidingExecution.returnValue)
  if (expectedOutput.length !== C3_ACCUMULATOR_LEN) {
    throw new Error(`Expected ${C3_ACCUMULATOR_LEN} hiding-kernel fields, got ${expectedOutput.length}`)
  }

  const expectedStackLength = 2 * C3_LEAF_STACK_SIZE + 2
  if (circuitNames.length !== expectedStackLength) {
    throw new Error(`Expected ${expectedStackLength} leaf Chonk circuits, got ${circuitNames.length}`)
  }

  const chonkBackend = new AztecClientBackend(stackCircuits.map(uncompressedBytecode), chonkApi, circuitNames)
  const chonkStarted = performance.now()
  const chonkResult = await chonkBackend.prove(witnesses, stackVks)
  const chonkSeconds = (performance.now() - chonkStarted) / 1000
  if (!(await chonkBackend.verify(chonkResult.proof, chonkResult.vk))) throw new Error('Chonk proof did not verify')

  const chonkProofFields = chonkResult.proofFields.map(fieldToHex)
  if (chonkProofFields.length !== C3_ACCUMULATOR_LEN + CHONK_PROOF_LENGTH) {
    throw new Error(`Unexpected Chonk field count: ${chonkProofFields.length}`)
  }
  const chonkPublicInputs = chonkProofFields.slice(0, C3_ACCUMULATOR_LEN)
  const chonkProof = chonkProofFields.slice(C3_ACCUMULATOR_LEN)
  const chonkVkFields = fieldsFromBytes(chonkResult.vk)
  if (chonkVkFields.length !== CHONK_VK_LENGTH_IN_FIELDS) {
    throw new Error(`Expected ${CHONK_VK_LENGTH_IN_FIELDS} Chonk VK fields, got ${chonkVkFields.length}`)
  }
  const chonkKeyHash = await poseidonHash(api, chonkVkFields)
  if (expectedOutput.some((value, index) => canonicalField(value) !== canonicalField(chonkPublicInputs[index]))) {
    throw new Error('Hiding-kernel and Chonk accumulator values differ')
  }

  const c3PublicInputs = [
    leafKeyHash,
    '',
    numberToField(0),
    numberToField(slotIndices[slotIndices.length - 1]),
    c3FoldKernelKeyHash,
    '',
    ...chonkPublicInputs,
  ]

  const tubeBackend = new UltraHonkBackend(tubeCircuit.bytecode, api)
  const tubeVk = await tubeBackend.getVerificationKey({ verifierTarget: 'noir-rollup-no-zk' })
  const tubeVkFields = fieldsFromBytes(tubeVk)
  const tubeKeyHash = await poseidonHash(api, tubeVkFields)
  c3PublicInputs[1] = tubeKeyHash
  c3PublicInputs[5] = tubeKeyHash
  if (c3PublicInputs.length !== C3_FOLD_PUBLIC_LEN) {
    throw new Error(`Expected ${C3_FOLD_PUBLIC_LEN} tube public inputs, got ${c3PublicInputs.length}`)
  }

  const { witness: tubeWitness } = await tubeNoir.execute({
    verification_key: chonkVkFields,
    proof: chonkProof,
    chonk_public_inputs: chonkPublicInputs,
    key_hash: chonkKeyHash,
    c3_public_inputs: c3PublicInputs,
  })
  const tubeStarted = performance.now()
  const tubeProof = await tubeBackend.generateProof(tubeWitness, { verifierTarget: 'noir-rollup-no-zk' })
  const tubeSeconds = (performance.now() - tubeStarted) / 1000
  if (!(await tubeBackend.verifyProof(tubeProof, { verifierTarget: 'noir-rollup-no-zk' }))) {
    throw new Error('C3 tube proof did not verify')
  }
  if (fieldsFromBytes(tubeProof.proof).length !== ROLLUP_HONK_PROOF_LENGTH) {
    throw new Error(`Expected ${ROLLUP_HONK_PROOF_LENGTH} rollup proof fields, got ${fieldsFromBytes(tubeProof.proof).length}`)
  }
  if (tubeProof.publicInputs.length !== C3_FOLD_PUBLIC_LEN) {
    throw new Error(`Expected ${C3_FOLD_PUBLIC_LEN} tube proof public inputs, got ${tubeProof.publicInputs.length}`)
  }

  return {
    fixture: {
      proofFields: fieldsFromBytes(tubeProof.proof),
      publicInputs: tubeProof.publicInputs,
      verificationKey: tubeVkFields,
      keyHash: tubeKeyHash,
    },
    chonkSeconds,
    tubeSeconds,
  }
}

function uncompressedBytecode(circuit: CompiledCircuit): Uint8Array {
  return gunzipSync(Buffer.from(circuit.bytecode, 'base64'))
}

function chunk<T>(values: T[], size: number): T[][] {
  if (!Number.isInteger(size) || size <= 0 || values.length % size !== 0) {
    throw new Error(`Cannot split ${values.length} values into chunks of ${size}`)
  }
  return Array.from({ length: values.length / size }, (_, index) => values.slice(index * size, (index + 1) * size))
}

async function main(): Promise<void> {
  console.log('=== Interfold real C3 Chonk probe ===')
  ensureLeafArtifact()
  ensureProbeArtifacts()

  const commitmentCircuit = loadCircuit('c3_commitment')
  const appCircuit = loadCircuit('c3_batch_app')
  const initCircuit = loadCircuit('c3_init_kernel')
  const tailCircuit = loadCircuit('c3_tail_kernel')
  const hidingCircuit = loadCircuit('c3_hiding_kernel')
  const leafAppCircuit = loadCircuit('c3_leaf_app')
  const leafInitCircuit = loadCircuit('c3_leaf_init_kernel')
  const leafInnerCircuit = loadCircuit('c3_leaf_inner_kernel')
  const leafTailCircuit = loadCircuit('c3_leaf_tail_kernel')
  const leafHidingCircuit = loadCircuit('c3_leaf_hiding_kernel')
  const tubeCircuit = loadCircuit('c3_tube')

  const leafVkFields = fieldsFromBytes(loadBytes(resolve(dkgTargetDir, 'share_encryption.vk_noir')))
  if (leafVkFields.length !== CHONK_VK_LENGTH_IN_FIELDS) {
    throw new Error(`Expected ${CHONK_VK_LENGTH_IN_FIELDS} leaf VK fields, got ${leafVkFields.length}`)
  }
  const leafKeyHash = fieldToHex(loadBytes(resolve(dkgTargetDir, 'share_encryption.vk_noir_hash')))
  const leafStarted = performance.now()
  const input = loadLeafFixtureInput()
  const leafProofsA = input?.c3a ?? (await generateLeafProofs(commitmentCircuit))
  const leafProofsB = input?.c3b ?? leafProofsA
  const slotIndices = input?.slotIndices ?? SLOT_INDICES
  const leafSeconds = (performance.now() - leafStarted) / 1000
  for (const fixture of [...leafProofsA, ...(input ? leafProofsB : [])]) {
    if (fixture.publicInputs.length !== 5) throw new Error(`Expected five C3 public inputs, got ${fixture.publicInputs.length}`)
  }
  if (slotIndices.length !== C3_LEAF_COUNT) throw new Error(`Expected ${C3_LEAF_COUNT} C3 slots, got ${slotIndices.length}`)
  const slotChunks = chunk(slotIndices, C3_BATCH_SIZE)
  const c3aChunks = chunk(leafProofsA, C3_BATCH_SIZE)
  const c3bChunks = chunk(leafProofsB, C3_BATCH_SIZE)
  const leafSlotChunks = chonkMode === 'leaf' ? chunk(slotIndices, C3_LEAF_STACK_SIZE) : []
  const leafC3aChunks = chonkMode === 'leaf' ? chunk(leafProofsA, C3_LEAF_STACK_SIZE) : []
  const leafC3bChunks = chonkMode === 'leaf' ? chunk(leafProofsB, C3_LEAF_STACK_SIZE) : []

  const leafFixturesOutputPath = process.env.CHONK_C3_LEAF_FIXTURES_OUTPUT
  if (leafFixturesOutputPath && !input) {
    mkdirSync(dirname(leafFixturesOutputPath), { recursive: true })
    writeFileSync(leafFixturesOutputPath, `${JSON.stringify({ slotIndices, c3a: leafProofsA, c3b: leafProofsB }, null, 2)}\n`)
  }

  const api = await Barretenberg.new({ threads: 4 })
  const chonkApi = await Barretenberg.new({ threads: 4 })

  try {
    const appNoir = new Noir(appCircuit as any)
    const initNoir = new Noir(initCircuit as any)
    const tailNoir = new Noir(tailCircuit as any)
    const hidingNoir = new Noir(hidingCircuit as any)
    const leafNoirs: LeafChonkNoirs = {
      app: new Noir(leafAppCircuit as any),
      init: new Noir(leafInitCircuit as any),
      inner: new Noir(leafInnerCircuit as any),
      tail: new Noir(leafTailCircuit as any),
      hiding: new Noir(leafHidingCircuit as any),
    }
    const tubeNoir = new Noir(tubeCircuit as any)

    const c3FoldKernelKeyHash = fieldToHex(
      loadBytes(resolve(repoRoot, 'circuits/bin/recursive_aggregation/c3_fold_kernel/target/c3_fold_kernel.vk_recursive_hash')),
    )
    const c3aResults = []
    const c3bResults = []
    if (chonkMode === 'leaf') {
      const leafCircuits: LeafChonkCircuits = {
        app: leafAppCircuit,
        init: leafInitCircuit,
        inner: leafInnerCircuit,
        tail: leafTailCircuit,
        hiding: leafHidingCircuit,
      }
      for (let i = 0; i < C3_LEAF_STACK_COUNT; i++) {
        c3aResults.push(
          await proveChonkLeafTube(
            api,
            chonkApi,
            leafCircuits,
            tubeCircuit,
            leafVkFields,
            leafKeyHash,
            c3FoldKernelKeyHash,
            leafC3aChunks[i],
            leafSlotChunks[i],
            leafNoirs,
            tubeNoir,
          ),
        )
        c3bResults.push(
          await proveChonkLeafTube(
            api,
            chonkApi,
            leafCircuits,
            tubeCircuit,
            leafVkFields,
            leafKeyHash,
            c3FoldKernelKeyHash,
            leafC3bChunks[i],
            leafSlotChunks[i],
            leafNoirs,
            tubeNoir,
          ),
        )
      }
    } else {
      const circuitNames = ['chonk_c3_batch_app', 'chonk_c3_init_kernel', 'chonk_c3_tail_kernel', 'chonk_c3_hiding_kernel']
      const circuits = [appCircuit, initCircuit, tailCircuit, hidingCircuit]
      const vks = circuitNames.map((name) => loadBytes(resolve(probeRoot, name.replace('chonk_', ''), 'target/vk')))
      const proveChunk = (leaves: ProofFixture[], slots: number[]) =>
        proveChonkTube(
          api,
          chonkApi,
          circuits,
          vks,
          circuitNames,
          tubeCircuit,
          leafVkFields,
          leafKeyHash,
          c3FoldKernelKeyHash,
          leaves,
          slots,
          appNoir,
          initNoir,
          tailNoir,
          hidingNoir,
          tubeNoir,
        )
      for (let i = 0; i < C3_CHUNK_COUNT; i++) {
        c3aResults.push(await proveChunk(c3aChunks[i], slotChunks[i]))
        c3bResults.push(await proveChunk(c3bChunks[i], slotChunks[i]))
      }
    }
    const output: TubeFixtureOutput = {
      slotIndices,
      c3a: c3aResults.map((result) => result.fixture),
      c3b: c3bResults.map((result) => result.fixture),
    }
    const outputPath = process.env.CHONK_C3_OUTPUT
    if (outputPath) {
      mkdirSync(dirname(outputPath), { recursive: true })
      writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`)
    }

    console.log(`Real C3 leaf proofs: ${input ? leafProofsA.length + leafProofsB.length : leafProofsA.length}`)
    console.log(`C3 slots: ${C3_SLOTS}; accumulator fields: ${C3_ACCUMULATOR_LEN}`)
    if (!input) console.log(`Real C3 leaf generation time: ${leafSeconds.toFixed(2)}s`)
    if (chonkMode === 'leaf') {
      console.log(
        `Chonk leaf apps per stack: ${C3_LEAF_STACK_SIZE}; stacks per C3 chain: ${C3_LEAF_STACK_COUNT}; circuits per stack: ${2 * C3_LEAF_STACK_SIZE + 2}`,
      )
    } else {
      console.log(`Chonk chunks per C3 chain: ${C3_CHUNK_COUNT}; leaves per chunk: ${C3_BATCH_SIZE}`)
    }
    console.log(`Chonk proof fields including public inputs: ${C3_ACCUMULATOR_LEN + CHONK_PROOF_LENGTH}`)
    console.log(`C3a Chonk proving time: ${c3aResults.reduce((total, result) => total + result.chonkSeconds, 0).toFixed(2)}s`)
    console.log(`C3a tube proving time: ${c3aResults.reduce((total, result) => total + result.tubeSeconds, 0).toFixed(2)}s`)
    console.log(`C3b Chonk proving time: ${c3bResults.reduce((total, result) => total + result.chonkSeconds, 0).toFixed(2)}s`)
    console.log(`C3b tube proving time: ${c3bResults.reduce((total, result) => total + result.tubeSeconds, 0).toFixed(2)}s`)
    console.log(`Chonk verification: PASS`)
    console.log(`Tube verification: PASS`)
    console.log(`Tube public-input width: ${c3aResults[0].fixture.publicInputs.length}`)
  } finally {
    await api.destroy()
    await chonkApi.destroy()
  }
}

main().catch((error) => {
  console.error('Real C3 Chonk probe failed:', error)
  process.exitCode = 1
})
