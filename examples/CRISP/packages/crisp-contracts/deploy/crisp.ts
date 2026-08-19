// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { getDeploymentChain, readDeploymentArgs, storeDeploymentArgs } from '@interfold/contracts/scripts'
import { Interfold__factory as InterfoldFactory } from '@interfold/contracts/types'
import { readFileSync } from 'fs'

import hre from 'hardhat'

import { CRISPProgram__factory as CRISPProgramFactory } from '../types'

const imageIdContent = readFileSync('../../.interfold/generated/contracts/ImageID.sol', 'utf-8')
const match = imageIdContent.match(/bytes32 public constant PROGRAM_ID = bytes32\((0x[a-fA-F0-9]+)\)/)
const IMAGE_ID = match ? match[1] : null

if (!IMAGE_ID) {
  throw new Error('IMAGE_ID not found')
}

export interface CRISPDeploymentResult {
  governanceComplete: boolean
}

export const deployCRISPContracts = async (): Promise<CRISPDeploymentResult> => {
  const { ethers } = await hre.network.connect()
  const [owner] = await ethers.getSigners()
  const ownerAddress = await owner.getAddress()

  const chain = getDeploymentChain(hre)
  const configuredOwner = process.env.CRISP_INITIAL_OWNER
  if (chain === 'mainnet' && !configuredOwner) {
    throw new Error('CRISP_INITIAL_OWNER is required on mainnet')
  }
  const initialOwner = configuredOwner ? ethers.getAddress(configuredOwner) : ownerAddress

  const useMocks = Boolean(process.env.USE_MOCKS)

  const verifier = await deployVerifier(useMocks, ethers)

  const encryptionSchemeId = ethers.keccak256(ethers.toUtf8Bytes('fhe.rs:BFV'))

  const ciphertextVerifier = await ethers.deployContract('Risc0BfvCiphertextVerifier', [verifier, IMAGE_ID])
  await ciphertextVerifier.waitForDeployment()
  const ciphertextVerifierAddress = await ciphertextVerifier.getAddress()
  storeDeploymentArgs(
    {
      address: ciphertextVerifierAddress,
      blockNumber: await ethers.provider.getBlockNumber(),
      constructorArgs: { verifier, imageId: IMAGE_ID },
    },
    'Risc0BfvCiphertextVerifier',
    chain,
  )
  let poseidonT3Address = readDeploymentArgs('PoseidonT3', chain)?.address
  if (!poseidonT3Address || (await ethers.provider.getCode(poseidonT3Address)) === '0x') {
    const poseidonT3 = await ethers.deployContract('PoseidonT3')
    await poseidonT3.waitForDeployment()
    poseidonT3Address = await poseidonT3.getAddress()
    storeDeploymentArgs(
      {
        address: poseidonT3Address,
        blockNumber: await ethers.provider.getBlockNumber(),
      },
      'PoseidonT3',
      chain,
    )
  }

  const zkTranscriptLib = await ethers.deployContract('contracts/CRISPVerifier.sol:ZKTranscriptLib')
  await zkTranscriptLib.waitForDeployment()
  const zkTranscriptLibAddress = await zkTranscriptLib.getAddress()
  const relationsLib = await ethers.deployContract('contracts/CRISPVerifier.sol:RelationsLib')
  await relationsLib.waitForDeployment()
  const relationsLibAddress = await relationsLib.getAddress()

  // Both generated verifiers declare a contract called `HonkVerifier`, so every factory lookup
  // here has to be fully qualified by file. A bare 'HonkVerifier' is ambiguous.
  const honkVerifierFactory = await ethers.getContractFactory('contracts/CRISPVerifier.sol:HonkVerifier', {
    libraries: {
      'project/contracts/CRISPVerifier.sol:ZKTranscriptLib': zkTranscriptLibAddress,
      'project/contracts/CRISPVerifier.sol:RelationsLib': relationsLibAddress,
    },
  })
  const honkVerifier = await honkVerifierFactory.deploy()
  await honkVerifier.waitForDeployment()
  const honkVerifierAddress = await honkVerifier.getAddress()

  storeDeploymentArgs(
    {
      address: honkVerifierAddress,
      blockNumber: await ethers.provider.getBlockNumber(),
    },
    'HonkVerifier',
    chain,
  )

  // The `CensusMode.ONCHAIN` verifier. Generated from the `crisp_onchain` circuit, which has no
  // Merkle inputs and takes voting power as a public input, so it needs its own libraries.
  const onchainZkTranscriptLib = await ethers.deployContract('contracts/CRISPOnchainVerifier.sol:ZKTranscriptLib')
  await onchainZkTranscriptLib.waitForDeployment()
  const onchainRelationsLib = await ethers.deployContract('contracts/CRISPOnchainVerifier.sol:RelationsLib')
  await onchainRelationsLib.waitForDeployment()

  const onchainHonkVerifierFactory = await ethers.getContractFactory('contracts/CRISPOnchainVerifier.sol:HonkVerifier', {
    libraries: {
      'project/contracts/CRISPOnchainVerifier.sol:ZKTranscriptLib': await onchainZkTranscriptLib.getAddress(),
      'project/contracts/CRISPOnchainVerifier.sol:RelationsLib': await onchainRelationsLib.getAddress(),
    },
  })
  const onchainHonkVerifier = await onchainHonkVerifierFactory.deploy()
  await onchainHonkVerifier.waitForDeployment()
  const onchainHonkVerifierAddress = await onchainHonkVerifier.getAddress()

  storeDeploymentArgs(
    {
      address: onchainHonkVerifierAddress,
      blockNumber: await ethers.provider.getBlockNumber(),
    },
    'OnchainHonkVerifier',
    chain,
  )

  const crispFactory = await ethers.getContractFactory(
    CRISPProgramFactory.abi,
    CRISPProgramFactory.linkBytecode({
      'npm/poseidon-solidity@0.0.5/PoseidonT3.sol:PoseidonT3': poseidonT3Address,
    }),
    owner,
  )

  const crisp = await crispFactory.deploy(initialOwner, verifier, honkVerifierAddress, onchainHonkVerifierAddress, IMAGE_ID)
  await crisp.waitForDeployment()

  const crispAddress = await crisp.getAddress()
  storeDeploymentArgs(
    {
      address: crispAddress,
      blockNumber: await ethers.provider.getBlockNumber(),
      constructorArgs: {
        initialOwner,
        verifierAddress: verifier,
        honkVerifierAddress,
        onchainHonkVerifierAddress,
        imageId: IMAGE_ID,
      },
    },
    'CRISPProgram',
    chain,
  )

  let governanceComplete = false
  const interfoldAddress = readDeploymentArgs('Interfold', chain)?.address
  if (interfoldAddress && (await ethers.provider.getCode(interfoldAddress)) !== '0x') {
    const interfold = InterfoldFactory.connect(interfoldAddress, owner)
    const interfoldOwner = await interfold.owner()
    const registered = await interfold.e3Programs(crispAddress)
    const boundInterfold = await crisp.interfold()
    const configuredVerifier = await interfold.getCiphertextVerifier(encryptionSchemeId)
    if (
      registered &&
      boundInterfold.toLowerCase() === interfoldAddress.toLowerCase() &&
      configuredVerifier.toLowerCase() === ciphertextVerifierAddress.toLowerCase()
    ) {
      governanceComplete = true
    } else if (interfoldOwner.toLowerCase() === ownerAddress.toLowerCase() && initialOwner.toLowerCase() === ownerAddress.toLowerCase()) {
      await (await interfold.setCiphertextVerifier(encryptionSchemeId, ciphertextVerifierAddress)).wait()
      if (!registered) {
        await (await interfold.registerE3Program(crispAddress)).wait()
      }
      await (await crisp.bindInterfold(interfoldAddress)).wait()
      governanceComplete = true
    } else {
      console.log(
        'CRISP integration is incomplete. Protocol governance must set the ciphertext verifier, register the program, and bind Interfold.',
      )
    }
  }

  let tokenAddress
  if (useMocks) {
    const token = await ethers.deployContract('MockVotingToken')
    await token.waitForDeployment()
    tokenAddress = await token.getAddress()

    storeDeploymentArgs(
      {
        address: tokenAddress,
        blockNumber: await ethers.provider.getBlockNumber(),
      },
      'MockVotingToken',
      chain,
    )
  }

  console.log(`
      Deployments:
      ----------------------------------------------------------------------
      Interfold: ${interfoldAddress ?? '(bind during protocol governance wiring)'}
      Risc0Verifier: ${verifier}
      Risc0BfvCiphertextVerifier: ${ciphertextVerifierAddress}
      HonkVerifier: ${honkVerifierAddress}
      OnchainHonkVerifier: ${onchainHonkVerifierAddress}
      CRISPProgram: ${crispAddress}
      TokenAddress: ${tokenAddress}
      `)

  return { governanceComplete }
}

/**
 * Deploys the verifier contract
 * @param useMockVerifier - whether to use a mock verifier
 * @returns The address of the verifier
 */
export const deployVerifier = async (useMockVerifier: boolean, connectedEthers?: any): Promise<string> => {
  const ethers = connectedEthers ?? (await hre.network.connect()).ethers
  const chain = getDeploymentChain(hre)

  if (!useMockVerifier) {
    const existingVerifier = readDeploymentArgs('RiscZeroGroth16Verifier', chain)
    if (existingVerifier?.address && (await ethers.provider.getCode(existingVerifier.address)) !== '0x') {
      console.log('RiscZeroGroth16Verifier already deployed at:', existingVerifier.address)
      return existingVerifier.address
    }
    const verifierFactory = await ethers.getContractFactory('RiscZeroGroth16Verifier')
    const verifier = await verifierFactory.deploy()
    await verifier.waitForDeployment()
    const address = await verifier.getAddress()

    storeDeploymentArgs(
      {
        address,
        blockNumber: await ethers.provider.getBlockNumber(),
      },
      'RiscZeroGroth16Verifier',
      chain,
    )
    return address
  }
  // Check if mock verifier already deployed
  const existingMockVerifier = readDeploymentArgs('MockRISC0Verifier', chain)
  if (existingMockVerifier?.address && (await ethers.provider.getCode(existingMockVerifier.address)) !== '0x') {
    console.log('MockRISC0Verifier already deployed at:', existingMockVerifier.address)
    return existingMockVerifier.address
  }
  const mockVerifierFactory = await ethers.getContractFactory('MockRISC0Verifier')
  const mockVerifier = await mockVerifierFactory.deploy()
  await mockVerifier.waitForDeployment()
  const mockVerifierAddress = await mockVerifier.getAddress()
  storeDeploymentArgs(
    {
      address: mockVerifierAddress,
      blockNumber: await ethers.provider.getBlockNumber(),
    },
    'MockRISC0Verifier',
    chain,
  )

  return mockVerifierAddress
}
