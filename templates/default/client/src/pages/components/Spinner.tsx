// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// ============================================================================
// SIMPLE SPINNER
// ============================================================================

import React from 'react'

interface SpinnerProps {
  size?: number
}

const Spinner: React.FC<SpinnerProps> = ({ size = 24 }) => (
  <div
    className='shrink-0 animate-spin rounded-full border-2 border-accent-deep border-t-transparent'
    style={{ width: size, height: size }}
  />
)

export default Spinner
