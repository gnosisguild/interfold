// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React from 'react'

interface SDKErrorDisplayProps {
  error: string
}

const SDKErrorDisplay: React.FC<SDKErrorDisplayProps> = ({ error }) => (
  <div className='min-h-screen bg-paper px-4 py-12 sm:px-6 lg:px-8'>
    <div className='mx-auto max-w-md'>
      <div className='note-danger'>
        <h3 className='text-sm font-semibold text-danger-ink'>SDK Error</h3>
        <div className='mt-2 text-sm'>{error}</div>
      </div>
    </div>
  </div>
)

export default SDKErrorDisplay
