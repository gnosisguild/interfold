// SPDX-License-Identifier: LGPL-3.0-only

import { readFile } from 'node:fs/promises'

import { createPublicClient, createWalletClient, encodeAbiParameters, http, keccak256, parseAbi, parseAbiItem } from 'viem'
import { anvil } from 'viem/chains'

// Keep local time moving and fulfill mock randomness in a separate transaction. Production VRF
// also fulfills after the request transaction, so this preserves the block boundary that nodes
// validate during live processing and replay.
const rpcUrl = process.env.ANVIL_RPC_URL ?? 'http://127.0.0.1:8545'
const deploymentsUrl = new URL('../deployed_contracts.json', import.meta.url)
const transport = http(rpcUrl)
const publicClient = createPublicClient({ chain: anvil, transport })
const walletClient = createWalletClient({ chain: anvil, transport })
const providerAbi = parseAbi([
  'function fulfill(uint256 requestId, uint256 randomWord)',
  'function getRandomness(uint256 requestId) view returns (bool fulfilled, uint256 randomWord, uint256 fulfilledAt, uint256 fulfilledBlock)',
])
const randomnessRequested = parseAbiItem('event RandomnessRequested(uint256 indexed requestId, uint256 indexed e3Id)')
const logIntervalMs = 30_000
const maximumConsecutiveFailures = 10

let activeDeployment
let nextBlock
let failureCount = 0
let lastLoggedTime = 0

async function rpc(method, params = []) {
  const response = await fetch(rpcUrl, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  })
  if (!response.ok) {
    throw new Error(`RPC ${method} returned HTTP ${response.status}`)
  }
  const payload = await response.json()
  if (payload.error) {
    throw new Error(payload.error.message ?? JSON.stringify(payload.error))
  }
}

async function readMockDeployment() {
  let contents
  try {
    contents = await readFile(deploymentsUrl, 'utf8')
  } catch (error) {
    if (error?.code === 'ENOENT') return undefined
    throw error
  }

  const deployment = JSON.parse(contents).localhost?.MockRandomnessProvider
  if (!deployment?.address || deployment.blockNumber === undefined) {
    return undefined
  }
  return {
    address: deployment.address,
    blockNumber: BigInt(deployment.blockNumber),
    key: `${deployment.address.toLowerCase()}:${deployment.blockNumber}`,
  }
}

async function fulfillPendingRandomness() {
  const deployment = await readMockDeployment()
  if (!deployment) return

  if (deployment.key !== activeDeployment) {
    activeDeployment = deployment.key
    nextBlock = deployment.blockNumber
  }

  const latestBlock = await publicClient.getBlockNumber()
  if (nextBlock > latestBlock) return

  const logs = await publicClient.getLogs({
    address: deployment.address,
    event: randomnessRequested,
    fromBlock: nextBlock,
    toBlock: latestBlock,
  })

  for (const log of logs) {
    const { e3Id, requestId } = log.args
    if (e3Id === undefined || requestId === undefined) {
      throw new Error('RandomnessRequested log is missing indexed arguments')
    }

    const [fulfilled, , , fulfilledBlock] = await publicClient.readContract({
      address: deployment.address,
      abi: providerAbi,
      functionName: 'getRandomness',
      args: [requestId],
    })
    if (fulfilled) {
      if (fulfilledBlock <= log.blockNumber) {
        throw new Error(`Randomness for E3 ${e3Id} was fulfilled in request block ${log.blockNumber}; disable mock auto-fulfillment`)
      }
      continue
    }

    const randomWord = BigInt(
      keccak256(
        encodeAbiParameters(
          [{ type: 'string' }, { type: 'uint256' }, { type: 'uint256' }],
          ['interfold-local-randomness', e3Id, requestId],
        ),
      ),
    )
    const localAccounts = await walletClient.getAddresses()
    const fulfillmentAccount = localAccounts.at(-1)
    if (!fulfillmentAccount) {
      throw new Error('Local chain does not expose an account for mock randomness fulfillment')
    }
    const hash = await walletClient.writeContract({
      account: fulfillmentAccount,
      address: deployment.address,
      abi: providerAbi,
      functionName: 'fulfill',
      args: [requestId, randomWord],
    })
    const receipt = await publicClient.waitForTransactionReceipt({ hash })
    if (receipt.blockNumber <= log.blockNumber) {
      throw new Error(`Randomness for E3 ${e3Id} was not fulfilled after request block ${log.blockNumber}`)
    }
    console.log(`[anvil-automine] fulfilled randomness for E3 ${e3Id} in block ${receipt.blockNumber}`)
  }

  nextBlock = latestBlock + 1n
}

async function loop() {
  for (;;) {
    try {
      await rpc('evm_mine')
      await fulfillPendingRandomness()
      failureCount = 0
    } catch (error) {
      failureCount++
      const now = Date.now()
      if (failureCount === 1 || now - lastLoggedTime >= logIntervalMs) {
        console.error(`[anvil-automine] local chain update failed (attempt ${failureCount}):`, error)
        lastLoggedTime = now
      }
      if (failureCount >= maximumConsecutiveFailures) {
        throw new Error(`Local chain updates failed ${failureCount} consecutive times`, {
          cause: error,
        })
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 1000))
  }
}

loop().catch((error) => {
  console.error('[anvil-automine] stopped:', error)
  process.exitCode = 1
})
