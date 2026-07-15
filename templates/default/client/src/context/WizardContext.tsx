// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React, { createContext, useContext, useEffect, useMemo, useCallback, useState, ReactNode } from 'react'
import { useAccount } from 'wagmi'
import { useInterfoldSDK, UseInterfoldSDKReturn } from '@interfold/react'
import { getInterfoldSDKConfig } from '@/utils/sdk-config'
import { loadWizardState, saveWizardState, clearWizardState } from '@/utils/persistence'

// ============================================================================
// TYPES & ENUMS
// ============================================================================

// eslint-disable-next-line react-refresh/only-export-components
export enum WizardStep {
  CONNECT_WALLET = 1,
  REQUEST_COMPUTATION = 2,
  ENTER_INPUTS = 3,
  ENCRYPT_SUBMIT = 4,
  RESULTS = 5,
}

export interface E3State {
  id: bigint | null
  isRequested: boolean
  isCommitteePublished: boolean
  isCiphertextPublished: boolean
  publicKey: `0x${string}` | null
  expiresAt: bigint | null
  plaintextOutput: string | null
  hasPlaintextOutput: boolean
}

const INITIAL_E3_STATE: E3State = {
  id: null,
  isRequested: false,
  isCommitteePublished: false,
  isCiphertextPublished: false,
  publicKey: null,
  expiresAt: null,
  plaintextOutput: null,
  hasPlaintextOutput: false,
}

interface WizardContextType {
  currentStep: WizardStep
  submittedInputs: { input1: string; input2: string } | null
  lastTransactionHash: string | undefined
  inputPublishError: string | null
  inputPublishSuccess: boolean
  result: number | null
  e3State: E3State

  // Setters
  setCurrentStep: (step: WizardStep) => void
  setSubmittedInputs: (inputs: { input1: string; input2: string } | null) => void
  setLastTransactionHash: (hash: string | undefined) => void
  setInputPublishError: (error: string | null) => void
  setInputPublishSuccess: (success: boolean) => void
  setResult: (result: number | null) => void
  setE3State: (state: E3State | ((prev: E3State) => E3State)) => void

  // Handlers
  handleReset: () => void
  handleTryAgain: () => void

  // SDK
  sdk: UseInterfoldSDKReturn
}

const WizardContext = createContext<WizardContextType | undefined>(undefined)

// eslint-disable-next-line react-refresh/only-export-components
export const useWizard = () => {
  const context = useContext(WizardContext)
  if (!context) {
    throw new Error('useWizard must be used within a WizardProvider')
  }
  return context
}

interface WizardProviderProps {
  children: ReactNode
}

/**
 * WizardProvider component - Provides the WizardContext to the application
 *
 * This component is used to provide the WizardContext to the application,
 * which is used to manage the wizard state and logic.
 */
export const WizardProvider: React.FC<WizardProviderProps> = ({ children }) => {
  const { isConnected, status } = useAccount()

  // Memoize the SDK config to prevent unnecessary re-initializations.
  const sdkConfig = useMemo(() => getInterfoldSDKConfig(), [])
  const sdk = useInterfoldSDK(sdkConfig)

  // Hydrate from any state persisted before a refresh so the user resumes from
  // the same step instead of starting over. Read once on mount.
  const persisted = useMemo(() => loadWizardState(), [])

  const [currentStep, setCurrentStep] = useState<WizardStep>(persisted?.currentStep ?? WizardStep.CONNECT_WALLET)
  const [submittedInputs, setSubmittedInputs] = useState<{ input1: string; input2: string } | null>(persisted?.submittedInputs ?? null)
  const [lastTransactionHash, setLastTransactionHash] = useState<string | undefined>(persisted?.lastTransactionHash ?? undefined)
  const [inputPublishError, setInputPublishError] = useState<string | null>(persisted?.inputPublishError ?? null)
  const [inputPublishSuccess, setInputPublishSuccess] = useState<boolean>(persisted?.inputPublishSuccess ?? false)
  const [result, setResult] = useState<number | null>(persisted?.result ?? null)
  const [e3State, setE3State] = useState<E3State>(persisted?.e3State ?? INITIAL_E3_STATE)

  const resetWizardState = useCallback((step: WizardStep) => {
    setCurrentStep(step)
    setSubmittedInputs(null)
    setLastTransactionHash(undefined)
    setInputPublishError(null)
    setInputPublishSuccess(false)
    setResult(null)
    setE3State(INITIAL_E3_STATE)
    clearWizardState()
  }, [])

  // Persist the wizard state on every change so a refresh can resume from here.
  useEffect(() => {
    saveWizardState({
      currentStep,
      submittedInputs,
      lastTransactionHash,
      inputPublishError,
      inputPublishSuccess,
      result,
      e3State,
    })
  }, [currentStep, submittedInputs, lastTransactionHash, inputPublishError, inputPublishSuccess, result, e3State])

  // Auto-advance steps based on connection & SDK state.
  //
  // Only reset when the wallet is genuinely disconnected — NOT while wagmi is
  // still `connecting`/`reconnecting` on a fresh page load, otherwise a refresh
  // would wipe the persisted state before the wallet finishes reconnecting.
  /* eslint-disable react-hooks/set-state-in-effect */
  useEffect(() => {
    if (status === 'disconnected') {
      resetWizardState(WizardStep.CONNECT_WALLET)
    } else if (isConnected && sdk.isInitialized && currentStep === WizardStep.CONNECT_WALLET) {
      setCurrentStep(WizardStep.REQUEST_COMPUTATION)
    }
  }, [status, isConnected, sdk.isInitialized, currentStep, resetWizardState])
  /* eslint-enable react-hooks/set-state-in-effect */

  const handleReset = useCallback(() => {
    const step = isConnected && sdk.isInitialized ? WizardStep.REQUEST_COMPUTATION : WizardStep.CONNECT_WALLET
    resetWizardState(step)
  }, [isConnected, sdk.isInitialized, resetWizardState])

  const handleTryAgain = useCallback(() => {
    setCurrentStep(WizardStep.ENTER_INPUTS)
    setInputPublishError(null)
    setInputPublishSuccess(false)
  }, [])

  const contextValue: WizardContextType = useMemo(
    () => ({
      currentStep,
      submittedInputs,
      lastTransactionHash,
      inputPublishError,
      inputPublishSuccess,
      result,
      e3State,
      setCurrentStep,
      setSubmittedInputs,
      setLastTransactionHash,
      setInputPublishError,
      setInputPublishSuccess,
      setResult,
      setE3State,
      handleReset,
      handleTryAgain,
      sdk,
    }),
    [
      currentStep,
      submittedInputs,
      lastTransactionHash,
      inputPublishError,
      inputPublishSuccess,
      result,
      e3State,
      handleReset,
      handleTryAgain,
      sdk,
    ],
  )

  return <WizardContext.Provider value={contextValue}>{children}</WizardContext.Provider>
}
