// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { network } from 'hardhat'
import { zeroHash } from 'viem'
import { CRISPProgram, HonkVerifier, MockInterfold, MockRISC0Verifier, PoseidonT3 } from '../types'

// Non-zero address used in the tests.
export const nonZeroAddress = '0xc6e7DF5E7b4f2A278906862b61205850344D4e7d'

export const { ethers } = await network.connect()
export const abiCoder = ethers.AbiCoder.defaultAbiCoder()

/**
 * Deploy a contract and return the address.
 * @param contractName - The name of the contract to deploy.
 * @returns The address of the deployed contract.
 */
export async function deployContract(contractName: string) {
  const contract = await ethers.deployContract(contractName)
  await contract.waitForDeployment()

  return contract
}

/**
 * Deploy PoseidonT3 and return the address.
 * @returns The address of the deployed PoseidonT3 contract.
 */
export async function deployPoseidonT3() {
  const contract = await deployContract('PoseidonT3')

  return contract as unknown as PoseidonT3
}

/**
 * Deploy MockInterfold and return the address.
 * @returns The address of the deployed MockInterfold contract.
 */
export async function deployMockInterfold() {
  const contract = await deployContract('MockInterfold')

  return contract as unknown as MockInterfold
}

export async function deployMockRISC0Verifier() {
  const contract = await deployContract('MockRISC0Verifier')

  return contract as unknown as MockRISC0Verifier
}

/**
 * Deploy HonkVerifier and return the address.
 * @returns The address of the deployed HonkVerifier contract.
 */
export async function deployHonkVerifier() {
  const zkTranscriptLib = await deployContract('contracts/CRISPVerifier.sol:ZKTranscriptLib')
  const relationsLib = await deployContract('contracts/CRISPVerifier.sol:RelationsLib')

  // Fully qualified: `CRISPOnchainVerifier.sol` declares a `HonkVerifier` too, so the bare name
  // is ambiguous.
  const HonkVerifierFactory = await ethers.getContractFactory('contracts/CRISPVerifier.sol:HonkVerifier', {
    libraries: {
      'project/contracts/CRISPVerifier.sol:ZKTranscriptLib': await zkTranscriptLib.getAddress(),
      'project/contracts/CRISPVerifier.sol:RelationsLib': await relationsLib.getAddress(),
    },
  })

  const honkVerifier = await HonkVerifierFactory.deploy()

  await honkVerifier.waitForDeployment()

  return honkVerifier as unknown as HonkVerifier
}

/**
 * Deploy the `CensusMode.ONCHAIN` HonkVerifier, generated from the `crisp_onchain` circuit.
 * @returns The deployed verifier.
 */
export async function deployOnchainHonkVerifier() {
  const zkTranscriptLib = await deployContract('contracts/CRISPOnchainVerifier.sol:ZKTranscriptLib')
  const relationsLib = await deployContract('contracts/CRISPOnchainVerifier.sol:RelationsLib')

  const HonkVerifierFactory = await ethers.getContractFactory('contracts/CRISPOnchainVerifier.sol:HonkVerifier', {
    libraries: {
      'project/contracts/CRISPOnchainVerifier.sol:ZKTranscriptLib': await zkTranscriptLib.getAddress(),
      'project/contracts/CRISPOnchainVerifier.sol:RelationsLib': await relationsLib.getAddress(),
    },
  })

  const honkVerifier = await HonkVerifierFactory.deploy()

  await honkVerifier.waitForDeployment()

  return honkVerifier as unknown as HonkVerifier
}

export async function deployCRISPProgram(
  contracts: {
    mockInterfold?: MockInterfold
    honkVerifier?: HonkVerifier
    onchainHonkVerifier?: HonkVerifier
    poseidonT3?: PoseidonT3
    risc0Verifier?: MockRISC0Verifier
  } = {},
) {
  const poseidonT3 = contracts.poseidonT3 || (await deployPoseidonT3())
  const honkVerifier = contracts.honkVerifier || (await deployHonkVerifier())
  // The `CensusMode.ONCHAIN` verifier is generated from a different circuit, but the constructor
  // only needs a non-zero address unless a test actually verifies an ONCHAIN ballot. Tests that do
  // must pass the real one.
  const onchainHonkVerifier = contracts.onchainHonkVerifier || honkVerifier
  const mockInterfold = contracts.mockInterfold || (await deployMockInterfold())
  const risc0Verifier = contracts.risc0Verifier ? await contracts.risc0Verifier.getAddress() : nonZeroAddress

  const programFactory = await ethers.getContractFactory('CRISPProgram', {
    libraries: {
      'npm/poseidon-solidity@0.0.5/PoseidonT3.sol:PoseidonT3': await poseidonT3.getAddress(),
    },
  })

  const program = await programFactory.deploy(
    await mockInterfold.getAddress(),
    risc0Verifier,
    await honkVerifier.getAddress(),
    await onchainHonkVerifier.getAddress(),
    zeroHash,
  )

  await program.waitForDeployment()

  return program as unknown as CRISPProgram
}
