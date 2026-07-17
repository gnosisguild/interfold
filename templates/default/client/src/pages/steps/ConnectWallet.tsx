// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React from 'react'
import { WalletIcon } from '@phosphor-icons/react'
import { useAccount } from 'wagmi'
import CardContent from '../components/CardContent'
import Spinner from '../components/Spinner'
import { useWizard } from '../../context/WizardContext'

/**
 * ConnectWallet component - First step in the Interfold wizard flow
 *
 * This component introduces users to the Interfold protocol and provides the initial
 * wallet connection interface. It explains the E3 (Encrypted Execution Environment)
 * concept and guides users through the secure computation workflow using FHE,
 * zero-knowledge proofs, and distributed key cryptography.
 */
const ConnectWallet: React.FC = () => {
  const { sdk } = useWizard()
  const { isConnected, status } = useAccount()

  // The wallet is (re)connecting — e.g. wagmi restoring a session after a refresh.
  const isReconnecting = status === 'connecting' || status === 'reconnecting'
  // Wallet is connected but the SDK is still spinning up; the wizard can't advance yet.
  const isInitializingSdk = isConnected && !sdk.isInitialized && !sdk.error

  // While an async step blocks progress, show a loader and clear feedback instead
  // of the idle connect prompt (which the user can't act on yet).
  if (isReconnecting || isInitializingSdk) {
    return (
      <CardContent>
        <div className='flex flex-col items-center space-y-6 py-8 text-center' role='status' aria-live='polite'>
          <Spinner size={48} />
          <div className='space-y-2'>
            <h3 className='text-xl'>{isReconnecting ? 'Reconnecting your wallet…' : 'Initializing secure environment…'}</h3>
            <p className='leading-relaxed text-ink-3'>
              {isReconnecting
                ? 'Restoring your wallet connection. This will only take a moment.'
                : 'Setting up the Interfold SDK and cryptographic context. Please wait before continuing.'}
            </p>
          </div>
        </div>
      </CardContent>
    )
  }

  return (
    <CardContent>
      <div className='space-y-6 text-center'>
        <div className='flex justify-center'>
          <WalletIcon size={48} className='text-accent-deep' />
        </div>
        <p className='eyebrow justify-center'>Step 1 · Connect Your Wallet</p>
        <div className='space-y-4'>
          <h3 className='text-2xl'>Welcome to Interfold</h3>
          <p className='leading-relaxed text-ink-3'>
            Interfold is a protocol for Encrypted Execution Environments (E3) that enables secure computations on private data using fully
            homomorphic encryption (FHE), zero-knowledge proofs, and distributed key cryptography. Connect your wallet to experience
            privacy-preserving computation.
          </p>
          <div className='note-accent text-left'>
            <strong className='font-semibold'>How it works:</strong> You'll request an E3 computation → Ciphernode committee is selected →
            Committee publishes shared public key → You encrypt and submit inputs → Secure computation executes → Only verified outputs are
            decrypted by the committee.
          </div>
        </div>
      </div>
    </CardContent>
  )
}

export default ConnectWallet
