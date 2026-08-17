// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { expect } from 'chai'
import { createHash } from 'crypto'
import { deployCRISPProgram, ethers } from './utils'
import type { CRISPProgram } from '../types'

const SNARK_SCALAR_FIELD = 21888242871839275222246405745257275088548364400416034343698204186575808495617n

/// The same vector is asserted by `leaf_layout_matches_the_contract` in
/// `crates/compute-provider/src/compute_input.rs`. If either side changes, both tests fail.
const VECTOR = {
  ciphertext: '0x' + Buffer.from(Array.from({ length: 64 }, (_, i) => i)).toString('hex'),
  commitment: '0x' + 'ab'.repeat(32),
  slot: '0x' + 'cd'.repeat(20),
  leaf: 3005744733328395831398716072572247490877798047068525662443106668216528579058n,
}

/// The input tree leaf binds the published ciphertext bytes to the commitment the Noir proof
/// constrained.
///
/// The proof never sees the serialized bytes, so without this binding a submitter can publish a
/// valid commitment beside unrelated bytes. The Secure Process then cannot reproduce the on-chain
/// root and the round is lost. Binding both lets it detect the mismatch and exclude that one input.
///
/// The Secure Process rebuilds this leaf in Rust. A divergence of a single byte between the two
/// implementations makes every root mismatch, and nothing else in the system would catch it —
/// which is what these tests exist for.
describe('CRISPProgram input leaf', function () {
  this.timeout(120000)

  let crispProgram: CRISPProgram

  before(async () => {
    crispProgram = await deployCRISPProgram()
  })

  function expectedLeaf(ciphertext: string, commitment: string, slot: string): bigint {
    const inner = createHash('sha256')
      .update(Buffer.from(ciphertext.slice(2), 'hex'))
      .digest()
    const outer = createHash('sha256')
      .update(inner)
      .update(Buffer.from(commitment.slice(2), 'hex'))
      .update(Buffer.from(slot.slice(2), 'hex'))
      .digest()
    return BigInt('0x' + outer.toString('hex')) % SNARK_SCALAR_FIELD
  }

  it('matches the shared cross-language vector', async () => {
    const leaf = await crispProgram.inputLeaf(VECTOR.ciphertext, VECTOR.commitment, VECTOR.slot)
    expect(leaf).to.equal(VECTOR.leaf)
  })

  it('is sha256(sha256(bytes) || commitment || slot) reduced into the scalar field', async () => {
    const leaf = await crispProgram.inputLeaf(VECTOR.ciphertext, VECTOR.commitment, VECTOR.slot)
    expect(leaf).to.equal(expectedLeaf(VECTOR.ciphertext, VECTOR.commitment, VECTOR.slot))
  })

  it('always produces a leaf the Poseidon tree accepts', async () => {
    // A leaf at or above the field order is rejected by LazyIMT, so the reduction is not optional.
    for (let i = 0; i < 8; i += 1) {
      const ciphertext = ethers.hexlify(ethers.randomBytes(96))
      const commitment = ethers.hexlify(ethers.randomBytes(32))
      const leaf = await crispProgram.inputLeaf(ciphertext, commitment, ethers.hexlify(ethers.randomBytes(20)))
      expect(leaf).to.be.lessThan(SNARK_SCALAR_FIELD)
    }
  })

  it('changes when the ciphertext bytes change', async () => {
    // This is the property the whole fix rests on: swapping the bytes beside a valid commitment
    // must be visible.
    const a = await crispProgram.inputLeaf(VECTOR.ciphertext, VECTOR.commitment, VECTOR.slot)
    const b = await crispProgram.inputLeaf(VECTOR.ciphertext.replace(/0f/, '1f'), VECTOR.commitment, VECTOR.slot)
    expect(a).to.not.equal(b)
  })

  it('changes when the commitment changes', async () => {
    const a = await crispProgram.inputLeaf(VECTOR.ciphertext, VECTOR.commitment, VECTOR.slot)
    const b = await crispProgram.inputLeaf(VECTOR.ciphertext, '0x' + 'ef'.repeat(32), VECTOR.slot)
    expect(a).to.not.equal(b)
  })

  it('does not let the two fields be traded off against each other', async () => {
    // Hashing the bytes before concatenating means the boundary between the two fields is fixed,
    // so no pair of (bytes, commitment) can be rearranged into another pair with the same leaf.
    const a = await crispProgram.inputLeaf('0x' + 'aa'.repeat(32) + 'bb'.repeat(32), '0x' + '11'.repeat(32), VECTOR.slot)
    const b = await crispProgram.inputLeaf('0x' + 'aa'.repeat(32), '0x' + 'bb'.repeat(32), VECTOR.slot)
    expect(a).to.not.equal(b)
  })

  it('accepts an empty ciphertext without reverting', async () => {
    const leaf = await crispProgram.inputLeaf('0x', VECTOR.commitment, VECTOR.slot)
    expect(leaf).to.equal(expectedLeaf('0x', VECTOR.commitment, VECTOR.slot))
  })

  it('changes when the slot changes', async () => {
    // The tree is append-only and the Secure Process groups entries by slot, so a prover must not
    // be able to move an entry to a different slot.
    const a = await crispProgram.inputLeaf(VECTOR.ciphertext, VECTOR.commitment, VECTOR.slot)
    const b = await crispProgram.inputLeaf(VECTOR.ciphertext, VECTOR.commitment, '0x' + '01'.repeat(20))
    expect(a).to.not.equal(b)
  })
})
