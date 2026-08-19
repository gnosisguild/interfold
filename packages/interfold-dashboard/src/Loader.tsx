// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
// Shared on-chain loading indicator: accent spinner + shimmering skeleton rows.

import { NETWORK_NAME } from './lib/chain'

export default function Loader({
  label = 'Loading on-chain data',
  sub = `Reading from ${NETWORK_NAME}…`,
}: {
  label?: string
  sub?: string
}) {
  return (
    <div className='loader' role='status' aria-live='polite'>
      <span className='loader__ring' aria-hidden='true' />
      <div className='loader__text'>
        <div className='loader__label'>{label}</div>
        <div className='loader__sub mono'>{sub}</div>
      </div>
      <div className='loader__skeleton' aria-hidden='true'>
        <span className='loader__bar' />
        <span className='loader__bar' />
        <span className='loader__bar' />
      </div>
    </div>
  )
}
