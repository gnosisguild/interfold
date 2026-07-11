// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { getProgramRunnerConfig } from './utils'

export async function callFheRunner(e3Id: bigint, params: string, ciphertextInputs: Array<[string, number]>): Promise<void> {
  const { PROGRAM_RUNNER_URL, CALLBACK_URL } = getProgramRunnerConfig()

  const payload = {
    e3_id: Number(e3Id),
    params,
    ciphertext_inputs: ciphertextInputs,
    callback_url: CALLBACK_URL,
  }
  console.log(`Submitting E3 ${e3Id} to FHE runner with ${ciphertextInputs.length} ciphertext input(s)`)

  const response = await fetch(`${PROGRAM_RUNNER_URL}/run_compute`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  })

  if (!response.ok) {
    throw new Error(`FHE Runner failed: ${response.status} ${response.statusText}`)
  }

  const result = await response.json()
  console.log(`✓ FHE Runner accepted E3 ${e3Id}:`, result)
}
