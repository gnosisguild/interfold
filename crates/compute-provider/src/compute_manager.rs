// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::ciphertext_output::ComputeProvider;
use crate::compute_input::{ComputeInput, FHEInputs, PublishedData};
use crate::FHEProcessor;

pub struct ComputeManager<P>
where
    P: ComputeProvider + Send + Sync,
{
    input: ComputeInput,
    provider: P,
    processor: FHEProcessor,
}

impl<P> ComputeManager<P>
where
    P: ComputeProvider + Send + Sync,
{
    pub fn new(provider: P, fhe_inputs: FHEInputs, fhe_processor: FHEProcessor) -> Self {
        Self::with_published(provider, fhe_inputs, Vec::new(), fhe_processor)
    }

    /// Carries what the E3 program published alongside each ciphertext, which its
    /// [`crate::InputPolicy`] reads to build leaves and select inputs.
    pub fn with_published(
        provider: P,
        fhe_inputs: FHEInputs,
        published: Vec<PublishedData>,
        fhe_processor: FHEProcessor,
    ) -> Self {
        Self {
            provider,
            input: ComputeInput {
                fhe_inputs,
                published,
            },
            processor: fhe_processor,
        }
    }

    pub fn start(&mut self) -> (P::Output, Vec<u8>) {
        // The host computes the ciphertext only to return it to the caller for publication. The
        // proof covers the copy the Secure Process computes for itself, so this value never
        // reaches the journal.
        let ciphertext = (self.processor)(&self.input.fhe_inputs);

        (self.provider.prove(&self.input), ciphertext)
    }
}
