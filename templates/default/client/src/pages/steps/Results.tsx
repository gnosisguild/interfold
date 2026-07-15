// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React, { useState } from 'react'
import { CheckCircleIcon, CopyIcon, CheckIcon } from '@phosphor-icons/react'
import CardContent from '../components/CardContent'
import { useWizard } from '../../context/WizardContext'

/**
 * Results component - Fifth step in the Interfold wizard flow
 *
 * This component displays the results of the computation, including the encrypted
 * computation, the E3 ID, the transaction hash, and the raw output.
 */
const Results: React.FC = () => {
  const { submittedInputs, result, e3State, lastTransactionHash, handleReset } = useWizard()
  const [copiedField, setCopiedField] = useState<string | null>(null)

  const copyToClipboard = async (text: string, field: string) => {
    try {
      await navigator.clipboard.writeText(text)
      setCopiedField(field)
      setTimeout(() => setCopiedField(null), 2000)
    } catch {
      // Fallback for environments without clipboard API
    }
  }

  const renderCopyButton = (text: string, field: string) => (
    <button
      onClick={() => copyToClipboard(text, field)}
      className='ml-2 inline-flex items-center text-ink-4 transition-colors hover:text-ink'
      aria-label={`Copy ${field}`}
    >
      {copiedField === field ? <CheckIcon size={14} className='text-accent-deep' /> : <CopyIcon size={14} />}
    </button>
  )

  return (
    <CardContent>
      <div className='space-y-6 text-center'>
        <div className='flex justify-center'>
          <CheckCircleIcon size={48} className='text-accent-deep' />
        </div>
        <p className='eyebrow justify-center'>Step 5 · Results</p>
        <div className='space-y-4'>
          <h3 className='text-2xl'>Computation Complete</h3>

          <div className='rounded-field border border-accent-soft bg-accent-bg p-6'>
            <div className='space-y-3'>
              <p className='eyebrow justify-center text-accent-ink/70'>Your Encrypted Computation</p>
              <p className='font-serif text-3xl text-accent-ink'>
                {submittedInputs ? `${submittedInputs.input1} + ${submittedInputs.input2} = ${result !== null ? result : '…'}` : '…'}
              </p>
              {result !== null && (
                <p className='text-sm text-accent-ink/70'>Computed securely using FHE with distributed key decryption.</p>
              )}
            </div>
          </div>

          <div className='grid grid-cols-1 gap-3 text-left'>
            <div className='note-muted'>
              <span className='flex items-center'>
                <strong className='mr-1 font-semibold text-ink-2'>E3 ID:</strong> {String(e3State.id)}
                {e3State.id !== null && renderCopyButton(String(e3State.id), 'e3id')}
              </span>
            </div>
            {lastTransactionHash && (
              <div className='note-muted'>
                <span className='flex items-center'>
                  <strong className='mr-1 font-semibold text-ink-2'>Transaction:</strong>
                  <span className='font-mono'>
                    {lastTransactionHash.slice(0, 10)}...{lastTransactionHash.slice(-8)}
                  </span>
                  {renderCopyButton(lastTransactionHash, 'txhash')}
                </span>
              </div>
            )}
            {e3State.plaintextOutput && (
              <div className='note-muted'>
                <span className='flex items-center'>
                  <strong className='mr-1 font-semibold text-ink-2'>Raw Output:</strong>
                  <span className='font-mono'>{e3State.plaintextOutput.slice(0, 20)}...</span>
                  {renderCopyButton(e3State.plaintextOutput, 'output')}
                </span>
              </div>
            )}
          </div>

          <div className='note-accent text-left'>
            <strong className='font-semibold'>Cryptographic Guarantees:</strong> Your inputs remained encrypted throughout the entire
            process. The Ciphernode Committee used distributed key cryptography to decrypt only the verified output, ensuring data privacy,
            data integrity, and correct execution.
          </div>
        </div>

        <button onClick={handleReset} className='btn-primary w-full'>
          Start New Computation
        </button>
      </div>
    </CardContent>
  )
}

export default Results
