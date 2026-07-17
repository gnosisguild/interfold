// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React, { useEffect, useState } from 'react'
import { NumberSquareOneIcon } from '@phosphor-icons/react'
import { hexToBytes } from 'viem'
import { useAccount, useWalletClient } from 'wagmi'
import CardContent from '../components/CardContent'
import { useWizard, WizardStep } from '../../context/WizardContext'
import { publishInput } from '../../utils/input'
import { getContractAddresses } from '../../utils/env-config'

/**
 * EnterInputs component - Third step in the Interfold wizard flow
 *
 * This component handles the input of two numbers for a privacy-preserving addition
 * using fully homomorphic encryption (FHE). It provides feedback on the input process
 * and displays the status of the input submission.
 */
const EnterInputs: React.FC = () => {
  const [input1, setInput1] = useState('')
  const [input2, setInput2] = useState('')
  const {
    e3State,
    setCurrentStep,
    setLastTransactionHash,
    setInputPublishError,
    setInputPublishSuccess,
    setSubmittedInputs,
    handleReset,
    sdk,
  } = useWizard()
  const { address } = useAccount()
  const { data: walletClient } = useWalletClient()
  const contracts = getContractAddresses()

  // Track the input window countdown so we can block submissions once it closes —
  // publishing after the deadline would revert on-chain. Tick every second.
  const [nowSeconds, setNowSeconds] = useState(() => Math.floor(Date.now() / 1000))
  useEffect(() => {
    const interval = setInterval(() => setNowSeconds(Math.floor(Date.now() / 1000)), 1000)
    return () => clearInterval(interval)
  }, [])

  const expiresAt = e3State.expiresAt !== null ? Number(e3State.expiresAt) : null
  const secondsLeft = expiresAt !== null ? Math.max(0, expiresAt - nowSeconds) : null
  const isWindowClosed = expiresAt !== null && nowSeconds >= expiresAt

  const handleInputSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!input1 || !input2 || e3State.publicKey === null || e3State.id === null || !walletClient || !address) {
      return
    }

    // Guard against a race: the window may have closed between render and click.
    if (isWindowClosed) {
      return
    }

    setCurrentStep(WizardStep.ENCRYPT_SUBMIT)
    setInputPublishError(null)
    setInputPublishSuccess(false)

    try {
      // Store the inputs in context for the Results component
      setSubmittedInputs({ input1, input2 })

      // Parse inputs
      const num1 = BigInt(input1)
      const num2 = BigInt(input2)

      // Convert hex public key to bytes
      const publicKeyBytes = hexToBytes(e3State.publicKey)

      // Encrypt both inputs
      const encryptedInput1 = await sdk.sdk?.encryptNumber(num1, publicKeyBytes)
      const encryptedInput2 = await sdk.sdk?.encryptNumber(num2, publicKeyBytes)

      if (!encryptedInput1 || !encryptedInput2) {
        throw new Error('Failed to encrypt inputs')
      }

      const toHex = (bytes: Uint8Array): `0x${string}` => `0x${Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')}`

      // Publish first input
      await publishInput(walletClient, e3State.id, toHex(encryptedInput1), address, contracts.e3Program)

      // Publish second input
      const hash2 = await publishInput(walletClient, e3State.id, toHex(encryptedInput2), address, contracts.e3Program)

      setLastTransactionHash(hash2)
      setInputPublishSuccess(true)
    } catch (error) {
      setInputPublishError(error instanceof Error ? error.message : 'Failed to encrypt and publish inputs')
      console.error('Error encrypting/publishing inputs:', error)
    }
  }

  return (
    <CardContent>
      <form onSubmit={handleInputSubmit} className='space-y-6 text-center'>
        <div className='flex justify-center'>
          <NumberSquareOneIcon size={48} className='text-accent-deep' />
        </div>
        <p className='eyebrow justify-center'>Step 3 · Enter Your Numbers</p>
        <div className='space-y-4'>
          <h3 className='text-2xl'>Homomorphic Encrypted Computation</h3>
          <p className='leading-relaxed text-ink-3'>
            Enter two numbers for a privacy-preserving addition using fully homomorphic encryption (FHE). Your inputs will be encrypted
            locally and remain encrypted throughout the entire computation process, with only the final result being decrypted.
          </p>
          <div className='note-accent text-left'>
            <strong className='font-semibold'>Privacy Guarantee:</strong> FHE allows computation on encrypted data. Your numbers remain
            private throughout the process — inputs, intermediate states, and execution are all encrypted.
          </div>

          <div className='space-y-4 text-left'>
            <div>
              <label htmlFor='input1' className='mb-2 block text-sm font-medium text-ink-2'>
                First Number
              </label>
              <input
                id='input1'
                type='number'
                value={input1}
                onChange={(e) => setInput1(e.target.value)}
                className='field'
                placeholder='Enter first number'
                required
              />
            </div>
            <div>
              <label htmlFor='input2' className='mb-2 block text-sm font-medium text-ink-2'>
                Second Number
              </label>
              <input
                id='input2'
                type='number'
                value={input2}
                onChange={(e) => setInput2(e.target.value)}
                className='field'
                placeholder='Enter second number'
                required
              />
            </div>
          </div>

          {isWindowClosed ? (
            <div className='note-danger text-left' role='alert'>
              <strong className='font-semibold'>Input window closed.</strong> The deadline to submit inputs for this E3 has passed. Start a
              new computation to try again.
            </div>
          ) : (
            secondsLeft !== null &&
            e3State.isCommitteePublished && (
              <div className='note-muted flex items-center justify-center gap-2 text-center'>
                <span className='dot-live' />
                Input window closes in <span className='font-mono font-semibold text-ink-2'>{secondsLeft}s</span>
              </div>
            )
          )}

          {input1 && input2 && !isWindowClosed && (
            <div className='note-accent text-left'>
              <strong className='font-semibold'>Ready to compute:</strong> {input1} + {input2} = ?
            </div>
          )}
        </div>

        {isWindowClosed ? (
          <button type='button' onClick={handleReset} className='btn-primary w-full'>
            Start New Computation
          </button>
        ) : (
          <button
            type='submit'
            disabled={!input1 || !input2 || !e3State.isCommitteePublished || !e3State.publicKey || !walletClient || !address}
            className='btn-primary w-full'
          >
            {!e3State.isCommitteePublished || !e3State.publicKey
              ? 'Waiting for Committee Key...'
              : !input1 || !input2
                ? 'Enter Both Numbers'
                : 'Proceed to Encryption'}
          </button>
        )}
      </form>
    </CardContent>
  )
}

export default EnterInputs
