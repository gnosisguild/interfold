// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React from 'react'
import { WarningIcon } from '@phosphor-icons/react'

interface EnvironmentErrorProps {
  missingVars: string[]
}

const EnvironmentError: React.FC<EnvironmentErrorProps> = ({ missingVars }) => {
  return (
    <div className='flex min-h-screen items-center justify-center bg-paper p-4'>
      <div className='w-full max-w-2xl rounded-card border border-rule bg-paper-2 p-8 shadow-card'>
        <div className='text-center'>
          <div className='mx-auto mb-6 flex h-16 w-16 items-center justify-center rounded-full bg-danger-bg'>
            <WarningIcon size={32} className='text-danger-ink' />
          </div>

          <h1 className='mb-4 text-3xl'>Environment Configuration Required</h1>

          <p className='mb-6 text-ink-3'>
            The following environment variables need to be configured before you can use the encrypted computation features:
          </p>

          <div className='mb-6 rounded-field bg-paper p-4'>
            <ul className='space-y-2 text-left'>
              {missingVars.map((varName) => (
                <li key={varName} className='flex items-center space-x-2'>
                  <code className='rounded bg-danger-bg px-2 py-1 font-mono text-sm text-danger-ink'>{varName}</code>
                </li>
              ))}
            </ul>
          </div>

          <div className='note-accent text-left'>
            <h3 className='mb-2 font-semibold text-accent-ink'>How to configure:</h3>
            <ol className='list-inside list-decimal space-y-1 text-sm'>
              <li>
                Create a <code className='rounded bg-accent-soft px-1'>.env</code> file in the client directory
              </li>
              <li>Add the missing environment variables with their appropriate values</li>
              <li>Restart the development server</li>
            </ol>
          </div>

          <div className='mt-6'>
            <button onClick={() => window.location.reload()} className='btn-primary w-full'>
              Reload Page
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

export default EnvironmentError
