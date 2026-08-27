// SPDX-License-Identifier: LGPL-3.0-only

import { readFile } from 'node:fs/promises'

import { AbiCoder, Contract, Interface, JsonRpcProvider, keccak256 } from 'ethers'

const rpcUrl = process.env.ANVIL_RPC_URL ?? 'http://127.0.0.1:8545'
const deploymentsUrl = new URL('../packages/crisp-contracts/deployed_contracts.json', import.meta.url)
const chain = new JsonRpcProvider(rpcUrl, 31337, { staticNetwork: true })
const providerInterface = new Interface([
  'event RandomnessRequested(uint256 indexed requestId, uint256 indexed e3Id)',
  'function fulfill(uint256 requestId, uint256 randomWord)',
  'function getRandomness(uint256 requestId) view returns (bool fulfilled, uint256 randomWord, uint256 fulfilledAt, uint256 fulfilledBlock)',
])
const registryInterface = new Interface(['function randomnessProvider() view returns (address)'])
const coordinatorInterface = new Interface([
  'function fulfillRandomWordsWithOverride(uint256 requestId, address consumer, uint256[] words)',
])
const maximumConsecutiveFailures = 10
const maximumBlockRange = 10_000
const pollIntervalMs = 500
const transactionTimeoutMs = 30_000

let activeDeployment
let nextBlock
let consecutiveFailures = 0

function sleep(durationMs) {
  return new Promise((resolve) => setTimeout(resolve, durationMs))
}

async function withTimeout(promise, durationMs, description) {
  let timer
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`${description} timed out after ${durationMs}ms`)), durationMs)
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

async function readConfiguredDeployment() {
  let contents
  try {
    contents = await readFile(deploymentsUrl, 'utf8')
  } catch (error) {
    if (error?.code === 'ENOENT') return undefined
    throw error
  }

  const records = JSON.parse(contents).localhost
  const registryRecord = records?.CiphernodeRegistryOwnable
  if (!registryRecord?.address || (await chain.getCode(registryRecord.address)) === '0x') return undefined
  const registry = new Contract(registryRecord.address, registryInterface, chain)
  const configuredAddress = await registry.randomnessProvider()
  if (/^0x0{40}$/i.test(configuredAddress)) return undefined
  const candidates = [
    { kind: 'mock', record: records.MockRandomnessProvider },
    { kind: 'coordinator', record: records.ChainlinkVrfRandomnessProvider },
  ]
  const selected = candidates.find(({ record }) => record?.address?.toLowerCase() === configuredAddress.toLowerCase())
  if (!selected) {
    throw new Error(`Configured randomness provider ${configuredAddress} has no deployment record`)
  }
  const { kind, record } = selected
  if (record.blockNumber === undefined) {
    throw new Error(`Randomness provider ${configuredAddress} has no deployment block`)
  }
  if ((await chain.getCode(configuredAddress)) === '0x') return undefined
  const coordinator = record.constructorArgs?.coordinator
  if (kind === 'coordinator' && !coordinator) {
    throw new Error(`Chainlink randomness provider ${configuredAddress} has no coordinator record`)
  }

  return {
    address: configuredAddress,
    blockNumber: Number(record.blockNumber),
    coordinator,
    kind,
    key: `${kind}:${configuredAddress.toLowerCase()}:${record.blockNumber}`,
  }
}

async function fulfillmentSigner() {
  const accounts = await chain.send('eth_accounts', [])
  const account = accounts.at(-1)
  if (!account) throw new Error('Anvil does not expose an account for mock randomness fulfillment')
  return chain.getSigner(account)
}

function randomWord(e3Id, requestId) {
  return BigInt(
    keccak256(AbiCoder.defaultAbiCoder().encode(['string', 'uint256', 'uint256'], ['interfold-local-randomness', e3Id, requestId])),
  )
}

async function fulfillPendingRandomness() {
  const deployment = await readConfiguredDeployment()
  if (!deployment) return

  if (deployment.key !== activeDeployment) {
    activeDeployment = deployment.key
    nextBlock = deployment.blockNumber
  }

  const latestBlock = await chain.getBlockNumber()
  if (nextBlock > latestBlock) return

  const lastBlock = Math.min(latestBlock, nextBlock + maximumBlockRange - 1)
  const logs = await chain.getLogs({
    address: deployment.address,
    topics: [providerInterface.getEvent('RandomnessRequested').topicHash],
    fromBlock: nextBlock,
    toBlock: lastBlock,
  })
  if (logs.length === 0) {
    nextBlock = lastBlock + 1
    return
  }
  const provider = new Contract(deployment.address, providerInterface, await fulfillmentSigner())

  for (const log of logs) {
    const event = providerInterface.parseLog(log)
    if (!event) throw new Error(`Cannot decode RandomnessRequested log ${log.transactionHash}`)

    const { e3Id, requestId } = event.args
    const [fulfilled, , , fulfilledBlock] = await provider.getRandomness(requestId)
    if (fulfilled) {
      if (Number(fulfilledBlock) <= log.blockNumber) {
        throw new Error(
          `Randomness for E3 ${e3Id} was fulfilled in request block ${log.blockNumber}; mock auto-fulfillment must stay disabled`,
        )
      }
      continue
    }

    const word = randomWord(e3Id, requestId)
    const transaction =
      deployment.kind === 'mock'
        ? await provider.fulfill(requestId, word)
        : await new Contract(deployment.coordinator, coordinatorInterface, await fulfillmentSigner()).fulfillRandomWordsWithOverride(
            requestId,
            deployment.address,
            [word],
          )
    const receipt = await withTimeout(transaction.wait(), transactionTimeoutMs, `Randomness fulfillment for E3 ${e3Id}`)
    if (!receipt) throw new Error(`Randomness fulfillment for E3 ${e3Id} was not mined`)
    if (receipt.status !== 1) throw new Error(`Randomness fulfillment for E3 ${e3Id} reverted`)
    if (receipt.blockNumber <= log.blockNumber) {
      throw new Error(`Randomness for E3 ${e3Id} was not fulfilled after request block ${log.blockNumber}`)
    }
    const [accepted] = await provider.getRandomness(requestId)
    if (!accepted) throw new Error(`Randomness provider did not record fulfillment for E3 ${e3Id}`)
    console.log(`[anvil-randomness] fulfilled E3 ${e3Id} in block ${receipt.blockNumber}`)
  }

  nextBlock = lastBlock + 1
}

async function run() {
  for (;;) {
    try {
      await fulfillPendingRandomness()
      consecutiveFailures = 0
    } catch (error) {
      consecutiveFailures += 1
      console.error(`[anvil-randomness] update failed (${consecutiveFailures}/${maximumConsecutiveFailures}):`, error)
      if (consecutiveFailures >= maximumConsecutiveFailures) throw error
    }
    await sleep(pollIntervalMs)
  }
}

run().catch((error) => {
  console.error('[anvil-randomness] stopped:', error)
  process.exitCode = 1
})
