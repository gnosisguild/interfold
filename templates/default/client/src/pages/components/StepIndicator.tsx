// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React from 'react'
import { WizardStep } from '../../context/WizardContext'

interface StepConfig {
  step: WizardStep
  icon: React.ComponentType<any>
}

interface StepIndicatorProps {
  currentStep: WizardStep
  steps: StepConfig[]
}

const StepIndicator: React.FC<StepIndicatorProps> = React.memo(({ currentStep, steps }) => {
  return (
    <div className='mb-8 flex justify-center'>
      <div className='flex items-center space-x-2'>
        {steps.map(({ step, icon: IconComponent }, index) => {
          const isActive = currentStep >= step
          const isCompleted = currentStep > step

          return (
            <div key={step} className='flex items-center'>
              <div
                className={`flex h-10 w-10 items-center justify-center rounded-full border transition-all duration-200 ${
                  isCompleted
                    ? 'border-accent-deep bg-accent-deep text-white'
                    : isActive
                      ? 'border-accent-deep bg-paper-2 text-accent-deep shadow-[0_0_0_4px_theme(colors.accent.soft)]'
                      : 'border-rule bg-paper-2 text-ink-4'
                }`}
              >
                <IconComponent size={22} weight={isActive ? 'bold' : 'regular'} />
              </div>
              {index < steps.length - 1 && (
                <div className={`mx-2 h-px w-8 transition-all duration-200 ${isCompleted ? 'bg-accent-deep' : 'bg-rule'}`} />
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
})

export default StepIndicator
