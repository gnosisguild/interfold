// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { encodeAbiParameters, parseAbiParameters, WalletClient } from 'viem'
import { MyProgram__factory as MyProgram } from '../types/factories/contracts'

/**
 * Publish an input to the program
 * @param walletClient - The wallet client to use for the transaction
 * @param e3Id - The E3 ID
 * @param input - The input data
 * @param ciphertextCommitment - The SAFE commitment for the input
 * @param sender - The sender address
 * @param programAddress - The program contract address
 */
export const publishInput = async (
  walletClient: WalletClient,
  e3Id: bigint,
  input: `0x${string}`,
  ciphertextCommitment: `0x${string}`,
  sender: `0x${string}`,
  programAddress: `0x${string}`,
): Promise<`0x${string}`> => {
  const data = encodeAbiParameters(parseAbiParameters('bytes, bytes32'), [input, ciphertextCommitment])

  return walletClient.writeContract({
    address: programAddress as `0x${string}`,
    abi: MyProgram.abi,
    functionName: 'publishInput',
    args: [e3Id, data],
    chain: walletClient.chain,
    account: sender,
  })
}
