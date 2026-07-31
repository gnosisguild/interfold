// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { expect } from 'chai'
import { deployCRISPProgram, deployHonkVerifier, deployMockInterfold, deployMockRISC0Verifier, ethers } from './utils'

describe('CRISP journal', () => {
  it('should match the journal returned by the RISC Zero guest', async () => {
    const ciphertextHash = ethers.hexlify(Uint8Array.from({ length: 32 }, (_, index) => index))
    const ciphertextCommitment = ethers.hexlify(Uint8Array.from({ length: 32 }, (_, index) => index + 32))
    const paramsHash = ethers.keccak256('0x')
    const inputRoot = '0x2134e76ac5d21aab186c2be1dd8f84ee880a1e46eaf712f9d371b6df22191f3e'

    const encodeRisc0Vec32 = (value: string) => {
      const encoded = [32, 0, 0, 0]
      for (const byte of ethers.getBytes(value)) {
        encoded.push(byte, 0, 0, 0)
      }
      return Uint8Array.from(encoded)
    }

    const journal = ethers.concat([ciphertextHash, ciphertextCommitment, paramsHash, inputRoot].map(encodeRisc0Vec32))
    const journalDigest = ethers.sha256(journal)
    expect(journalDigest).to.equal('0xce9d56ad04f773831f389cf277232ba89722e7e25c83f54022ce056abc9cf5c5')

    const mockInterfold = await deployMockInterfold()
    const honkVerifier = await deployHonkVerifier()
    const risc0Verifier = await deployMockRISC0Verifier()
    await risc0Verifier.setExpectedJournalDigest(journalDigest)
    const program = await deployCRISPProgram({ mockInterfold, honkVerifier, risc0Verifier })
    const e3Id = await mockInterfold.nextE3Id()
    await mockInterfold.request(await program.getAddress())

    await program.verify(e3Id, ciphertextHash, ciphertextCommitment, '0x')
  })
})
