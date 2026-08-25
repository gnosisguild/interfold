// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { network } from 'hardhat'
import { zeroHash } from 'viem'
import { CRISPProgram, HonkVerifier, MockInterfold, MockRISC0Verifier, PoseidonT3 } from '../types'
import { verifierNames } from '../scripts/verifiers'

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
  // Fully qualified: every generated verifier declares a `HonkVerifier`, and there is one per
  // census mode per preset, so the bare name is ambiguous. See scripts/verifiers.ts.
  const names = verifierNames('merkle')
  const zkTranscriptLib = await deployContract(names.zkTranscriptLib)
  const relationsLib = await deployContract(names.relationsLib)

  const HonkVerifierFactory = await ethers.getContractFactory(names.honkVerifier, {
    libraries: {
      [names.libraryKeys.zkTranscriptLib]: await zkTranscriptLib.getAddress(),
      [names.libraryKeys.relationsLib]: await relationsLib.getAddress(),
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
  const names = verifierNames('onchain')
  const zkTranscriptLib = await deployContract(names.zkTranscriptLib)
  const relationsLib = await deployContract(names.relationsLib)

  const HonkVerifierFactory = await ethers.getContractFactory(names.honkVerifier, {
    libraries: {
      [names.libraryKeys.zkTranscriptLib]: await zkTranscriptLib.getAddress(),
      [names.libraryKeys.relationsLib]: await relationsLib.getAddress(),
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
    bindInterfold?: boolean
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
  const [owner] = await ethers.getSigners()

  const program = await programFactory.deploy(
    await owner.getAddress(),
    risc0Verifier,
    await honkVerifier.getAddress(),
    await onchainHonkVerifier.getAddress(),
    zeroHash,
  )

  await program.waitForDeployment()

  if (contracts.bindInterfold !== false) {
    await (await mockInterfold.registerE3Program(await program.getAddress())).wait()
    await (await program.bindInterfold(await mockInterfold.getAddress())).wait()
  }

  return program as unknown as CRISPProgram
}
