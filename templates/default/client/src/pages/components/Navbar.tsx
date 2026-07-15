// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React from 'react'
import { ConnectKitButton } from 'connectkit'
import { useAccount, useSwitchChain, useConfig } from 'wagmi'
import { CaretDownIcon, ArrowCounterClockwiseIcon } from '@phosphor-icons/react'
import { useWizard, WizardStep } from '../../context/WizardContext'

const NetworkSwitchButton: React.FC = () => {
  const { isConnected, chain } = useAccount()
  const config = useConfig()
  const { switchChain, isPending } = useSwitchChain()

  // Only show if connected and there are multiple chains
  if (!isConnected || config.chains.length <= 1) {
    return null
  }

  const handleNetworkSwitch = (chainId: number) => {
    if (chainId !== chain?.id) {
      switchChain({ chainId })
    }
  }

  return (
    <div className='relative'>
      <select
        value={chain?.id || ''}
        onChange={(e) => handleNetworkSwitch(Number(e.target.value))}
        disabled={isPending}
        className='appearance-none rounded-full border border-rule bg-paper-2 px-3 py-2 pr-8 text-sm font-medium text-ink hover:border-ink-4 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50'
      >
        {config.chains.map((supportedChain) => (
          <option key={supportedChain.id} value={supportedChain.id}>
            {supportedChain.name}
          </option>
        ))}
      </select>
      <CaretDownIcon className='pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 transform text-ink-4' />
    </div>
  )
}

// Clears the persisted wizard state and returns to the start of the flow.
// Only shown once the user has moved past the first step, so a stuck or stale
// (localStorage-persisted) run can always be abandoned.
const StartOverButton: React.FC = () => {
  const { currentStep, handleReset } = useWizard()

  if (currentStep === WizardStep.CONNECT_WALLET) {
    return null
  }

  return (
    <button
      onClick={handleReset}
      className='inline-flex items-center gap-1.5 rounded-full border border-rule bg-paper-2 px-3 py-2 text-sm font-medium text-ink-3 transition-colors hover:border-ink-4 hover:text-ink focus:outline-none'
      title='Discard the current computation and start over'
    >
      <ArrowCounterClockwiseIcon className='h-4 w-4' />
      Start over
    </button>
  )
}

const Navbar: React.FC = () => {
  return (
    <nav className='w-full border-b border-rule bg-paper/80 px-6 backdrop-blur-sm lg:px-9'>
      <div className='mx-auto max-w-screen-xl'>
        <div className='flex h-20 items-center justify-between'>
          <h1 className='font-serif text-2xl font-normal tracking-tight text-ink'>Interfold E3</h1>
          <div className='flex items-center gap-3'>
            <StartOverButton />
            <NetworkSwitchButton />
            <ConnectKitButton />
          </div>
        </div>
      </div>
    </nav>
  )
}

export default Navbar
