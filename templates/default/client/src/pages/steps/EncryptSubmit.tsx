// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React, { useState, useEffect } from 'react'
import { LockIcon, CheckCircleIcon, WarningCircleIcon, CircleDashedIcon } from '@phosphor-icons/react'
import CardContent from '../components/CardContent'
import Spinner from '../components/Spinner'
import ErrorDisplay from '../components/ErrorDisplay'
import { useWizard, WizardStep } from '../../context/WizardContext'
import { decodePlaintextOutput } from '@interfold/sdk'

/**
 * EncryptSubmit component - Fourth step in the Interfold wizard flow
 *
 * This component handles the encryption and submission of user inputs to the E3.
 * It provides feedback on the encryption process and displays the status of the
 * submission to the E3.
 */
const EncryptSubmit: React.FC = () => {
  const { e3State, setE3State, setResult, setCurrentStep, inputPublishError, inputPublishSuccess, handleTryAgain, handleReset, sdk } =
    useWizard()
  const { isInitialized, onInterfoldEvent, off, InterfoldEventType } = sdk

  const [showErrorDetails, setShowErrorDetails] = useState(false)
  const [isExpired, setIsExpired] = useState(false)

  // Set up event listeners for this step
  useEffect(() => {
    if (!isInitialized) return

    const handleCiphertextOutput = (event: any) => {
      const { e3Id } = event.data
      setE3State((prev) => {
        if (prev.id !== null && e3Id === prev.id) {
          return { ...prev, isCiphertextPublished: true }
        }
        return prev
      })
    }

    const handlePlaintextOutput = (event: any) => {
      const { e3Id, plaintextOutput } = event.data
      setE3State((prev) => {
        if (prev.id !== null && e3Id === prev.id) {
          const decodedResult = decodePlaintextOutput(plaintextOutput)
          setResult(decodedResult)
          return {
            ...prev,
            plaintextOutput: plaintextOutput as string,
            hasPlaintextOutput: true,
          }
        }
        return prev
      })
    }

    onInterfoldEvent(InterfoldEventType.CIPHERTEXT_OUTPUT_PUBLISHED, handleCiphertextOutput)
    onInterfoldEvent(InterfoldEventType.PLAINTEXT_OUTPUT_PUBLISHED, handlePlaintextOutput)

    return () => {
      off(InterfoldEventType.CIPHERTEXT_OUTPUT_PUBLISHED, handleCiphertextOutput)
      off(InterfoldEventType.PLAINTEXT_OUTPUT_PUBLISHED, handlePlaintextOutput)
    }
  }, [isInitialized, onInterfoldEvent, off, InterfoldEventType, setE3State, setResult])

  // Check for E3 expiration
  useEffect(() => {
    if (!e3State.expiresAt || e3State.hasPlaintextOutput) return

    const checkExpiration = () => {
      const nowSeconds = BigInt(Math.floor(Date.now() / 1000))
      if (nowSeconds > e3State.expiresAt!) {
        setIsExpired(true)
      }
    }

    checkExpiration()
    const interval = setInterval(checkExpiration, 5000)
    return () => clearInterval(interval)
  }, [e3State.expiresAt, e3State.hasPlaintextOutput])

  // Auto-advance to results when output is available
  useEffect(() => {
    if (e3State.hasPlaintextOutput) {
      setCurrentStep(WizardStep.RESULTS)
    }
  }, [e3State.hasPlaintextOutput, setCurrentStep])

  // Progress steps for the computing phase. These happen sequentially, so only
  // the first not-yet-done step is "active" (shows a spinner); later steps are
  // still pending and must not appear to be running at the same time.
  const progressSteps = [
    { label: 'Inputs submitted', done: inputPublishSuccess },
    { label: 'FHE computation', done: e3State.isCiphertextPublished },
    { label: 'Committee decryption', done: e3State.hasPlaintextOutput },
  ]
  const activeStepIndex = progressSteps.findIndex((step) => !step.done)

  return (
    <CardContent>
      <div className='space-y-6 text-center'>
        <div className='flex justify-center'>
          <LockIcon size={48} className='text-accent-deep' />
        </div>
        <p className='eyebrow justify-center'>Step 4 · Encrypting & Submitting</p>
        <div className='space-y-4'>
          <h3 className='text-2xl'>Secure Process Execution</h3>

          {/* Only a genuine failure when the input window closed BEFORE inputs were
              submitted. After a successful submit the window closing is expected —
              FHE computation and decryption run afterwards — so we keep showing
              progress instead of flashing an "expired" warning. */}
          {isExpired && !inputPublishSuccess && !e3State.hasPlaintextOutput && (
            <div className='space-y-4'>
              <div className='flex justify-center'>
                <WarningCircleIcon size={48} className='text-amber-500' />
              </div>
              <div role='alert' className='rounded-field border border-amber-200 bg-amber-50 p-4 text-left text-sm text-amber-800'>
                <strong className='font-semibold'>E3 Input Window Expired</strong>
                <br />
                The input deadline for this computation has passed. The computation may not have received enough inputs to produce a result.
              </div>
              <button onClick={handleReset} className='btn-primary w-full'>
                Start New Computation
              </button>
            </div>
          )}

          {!isExpired && !inputPublishError && !inputPublishSuccess && (
            <div className='space-y-4'>
              <div className='flex justify-center'>
                <Spinner size={40} />
              </div>
              <p className='text-ink-3'>
                Your inputs are being encrypted to the committee's public key and submitted to the E3. The Compute Provider will execute the
                FHE computation over your encrypted data…
              </p>
              <div className='note-accent text-left'>
                <strong className='font-semibold'>Process:</strong> Encrypt to Key → Submit to E3 → FHE Computation → Ciphertext Output
              </div>
            </div>
          )}

          {inputPublishError && (
            <div className='space-y-4'>
              <ErrorDisplay
                error={inputPublishError}
                showDetails={showErrorDetails}
                onToggleDetails={() => setShowErrorDetails(!showErrorDetails)}
              />
              <button onClick={handleTryAgain} className='btn-danger w-full'>
                Try Again
              </button>
            </div>
          )}

          {inputPublishSuccess && (
            <div className='space-y-4'>
              <div className='flex justify-center'>
                <CheckCircleIcon size={48} className='text-accent-deep' />
              </div>

              {/* Progress tracker */}
              <div className='rounded-field border border-rule bg-paper p-4'>
                <ul className='space-y-2 text-left'>
                  {progressSteps.map((step, i) => {
                    const isActive = i === activeStepIndex
                    return (
                      <li key={i} className='flex items-center gap-2 text-sm'>
                        {step.done ? (
                          <CheckCircleIcon size={18} className='flex-shrink-0 text-accent-deep' />
                        ) : isActive ? (
                          <Spinner size={18} />
                        ) : (
                          <CircleDashedIcon size={18} className='flex-shrink-0 text-ink-4' />
                        )}
                        <span className={step.done ? 'font-medium text-accent-ink' : isActive ? 'text-ink-2' : 'text-ink-4'}>
                          {step.label}
                        </span>
                      </li>
                    )
                  })}
                </ul>
              </div>

              {!e3State.isCiphertextPublished && (
                <div className='note-muted text-left'>
                  The Compute Provider is executing the FHE computation over your encrypted inputs…
                </div>
              )}

              {e3State.isCiphertextPublished && !e3State.hasPlaintextOutput && (
                <div className='note-muted text-left'>
                  Ciphertext output published. Waiting for the Ciphernode Committee to collectively decrypt the result…
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </CardContent>
  )
}

export default EncryptSubmit
