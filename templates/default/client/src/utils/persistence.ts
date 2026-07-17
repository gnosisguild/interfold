// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import type { E3State, WizardStep } from '../context/WizardContext'
import { INTERFOLD_ADDRESS, E3_PROGRAM_ADDRESS, REGISTRY_ADDRESS, FEE_TOKEN_ADDRESS } from './env-config'

// ============================================================================
// WIZARD STATE PERSISTENCE
// ============================================================================
//
// Persists the wizard state to localStorage so a browser refresh resumes from
// the same step instead of resetting to the beginning.
//
// Because localStorage outlives the tab, stale state is invalidated two ways:
//   1. Fingerprint — the state is tagged with the contract addresses it was
//      created against. If the app is pointed at a different deployment, the
//      persisted state is discarded on load (the user has effectively started
//      over against a new set of contracts).
//   2. Reset — `clearWizardState` is called whenever the wizard is reset
//      (e.g. "Start New Computation" / disconnect), so a fresh run starts clean.

const STORAGE_KEY = 'interfold-wizard-state'

// Identifies the deployment this state belongs to. If any address changes, the
// persisted state no longer applies and is thrown away.
const APP_FINGERPRINT = [INTERFOLD_ADDRESS, E3_PROGRAM_ADDRESS, REGISTRY_ADDRESS, FEE_TOKEN_ADDRESS].join('|')

// The snapshot of wizard state that is persisted across refreshes.
export interface PersistedWizardState {
  currentStep: WizardStep
  submittedInputs: { input1: string; input2: string } | null
  lastTransactionHash: string | undefined
  inputPublishError: string | null
  inputPublishSuccess: boolean
  result: number | null
  e3State: E3State
}

// What actually lands in storage: the snapshot plus its deployment fingerprint.
interface StoredWizardState extends PersistedWizardState {
  __fingerprint: string
}

// `bigint` is not JSON-serializable, so tag it on the way out and rebuild it on
// the way in. E3State carries bigints (`id`, `expiresAt`).
const BIGINT_TAG = '__bigint__'

const replacer = (_key: string, value: unknown): unknown => (typeof value === 'bigint' ? { [BIGINT_TAG]: value.toString() } : value)

const reviver = (_key: string, value: unknown): unknown => {
  if (value !== null && typeof value === 'object' && BIGINT_TAG in (value as Record<string, unknown>)) {
    return BigInt((value as Record<string, string>)[BIGINT_TAG])
  }
  return value
}

/**
 * Load previously persisted wizard state, or `null` if none exists, it is
 * unreadable, or it belongs to a different deployment (stale state is cleared).
 */
export const loadWizardState = (): PersistedWizardState | null => {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null

    const stored = JSON.parse(raw, reviver) as StoredWizardState

    // Different contract addresses => stale state from another deployment.
    if (stored.__fingerprint !== APP_FINGERPRINT) {
      clearWizardState()
      return null
    }

    const { __fingerprint: _fingerprint, ...state } = stored
    return state
  } catch {
    return null
  }
}

/** Persist the current wizard state. Failures (e.g. storage disabled) are ignored. */
export const saveWizardState = (state: PersistedWizardState): void => {
  try {
    const stored: StoredWizardState = { ...state, __fingerprint: APP_FINGERPRINT }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored, replacer))
  } catch {
    // Ignore — persistence is best-effort and must never break the app.
  }
}

/** Remove any persisted wizard state (used on reset / disconnect). */
export const clearWizardState = (): void => {
  try {
    localStorage.removeItem(STORAGE_KEY)
  } catch {
    // Ignore.
  }
}
