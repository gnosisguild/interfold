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
import { verifierNames } from '../scripts/verifiers'

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

  // Every generated verifier declares a contract called `HonkVerifier`, and there is one stack per
  // census mode per preset, so factory lookups have to be fully qualified by file. `verifierNames`
  // owns that convention and the preset choice — see scripts/verifiers.ts.
  const merkleVerifier = verifierNames('merkle')

  const zkTranscriptLib = await ethers.deployContract(merkleVerifier.zkTranscriptLib)
  await zkTranscriptLib.waitForDeployment()
  const zkTranscriptLibAddress = await zkTranscriptLib.getAddress()
  const relationsLib = await ethers.deployContract(merkleVerifier.relationsLib)
  await relationsLib.waitForDeployment()
  const relationsLibAddress = await relationsLib.getAddress()

  const honkVerifierFactory = await ethers.getContractFactory(merkleVerifier.honkVerifier, {
    libraries: {
      [merkleVerifier.libraryKeys.zkTranscriptLib]: zkTranscriptLibAddress,
      [merkleVerifier.libraryKeys.relationsLib]: relationsLibAddress,
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
  const onchainVerifier = verifierNames('onchain')

  const onchainZkTranscriptLib = await ethers.deployContract(onchainVerifier.zkTranscriptLib)
  await onchainZkTranscriptLib.waitForDeployment()
  const onchainRelationsLib = await ethers.deployContract(onchainVerifier.relationsLib)
  await onchainRelationsLib.waitForDeployment()

  const onchainHonkVerifierFactory = await ethers.getContractFactory(onchainVerifier.honkVerifier, {
    libraries: {
      [onchainVerifier.libraryKeys.zkTranscriptLib]: await onchainZkTranscriptLib.getAddress(),
      [onchainVerifier.libraryKeys.relationsLib]: await onchainRelationsLib.getAddress(),
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

  // The open census for ONCHAIN rounds: anyone registers themselves and votes with weight 1.
  // Deployed alongside the program so a self-registration round only needs its address passed as
  // the round's token — no census indexer, no merkle root, no minting.
  const selfRegistry = await ethers.deployContract('SelfRegistry')
  await selfRegistry.waitForDeployment()
  const selfRegistryAddress = await selfRegistry.getAddress()

  storeDeploymentArgs(
    {
      address: selfRegistryAddress,
      blockNumber: await ethers.provider.getBlockNumber(),
    },
    'SelfRegistry',
    chain,
  )

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
      SelfRegistry: ${selfRegistryAddress}
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
