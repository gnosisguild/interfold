// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::Result;
use e3_compute_provider::{
    ComputeInput, ComputeManager, ComputeProvider, ComputeResult, InputPolicy,
};
use e3_program_server::E3ProgramServer;
use e3_user_program::fhe_processor;

struct MockProofProvider;

impl ComputeProvider for MockProofProvider {
    type Output = ComputeResult;

    fn prove(&self, input: &ComputeInput, policy: InputPolicy) -> Self::Output {
        // The policy comes from the caller rather than from `e3_user_program::policy()` here.
        // Reading it twice would let the ciphertext this run publishes and the one it proves be
        // selected by different rules, which is the divergence the argument exists to remove.
        //
        // This dev provider stands in for the zkVM, where a failure aborts the guest. Panicking
        // with the reason keeps that behaviour while naming the input that could not be used.
        input
            .process(fhe_processor, policy)
            .expect("the Secure Process rejected its inputs")
    }
}

fn encode_mock_compute_proof(seal: &[u8], result: &ComputeResult) -> Result<Vec<u8>> {
    let params_hash: [u8; 32] = result
        .params_hash
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("parameter hash must be 32 bytes"))?;
    let input_root: [u8; 32] = result
        .merkle_root
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("input root must be 32 bytes"))?;
    let padded_seal_len = seal.len().div_ceil(32) * 32;
    let seal_len = u64::try_from(seal.len())?;
    let mut encoded = vec![0_u8; 128 + padded_seal_len];

    encoded[31] = 96;
    encoded[32..64].copy_from_slice(&params_hash);
    encoded[64..96].copy_from_slice(&input_root);
    encoded[120..128].copy_from_slice(&seal_len.to_be_bytes());
    encoded[128..128 + seal.len()].copy_from_slice(seal);

    Ok(encoded)
}

#[tokio::main]
async fn main() -> Result<()> {
    let server = E3ProgramServer::builder(|job| async move {
        // `with_published`, not `new`: a program whose policy reads the commitment or the slot
        // needs what the E3 program published, and `new` supplies none of it.
        let mut manager = ComputeManager::with_published(
            MockProofProvider,
            job.inputs,
            job.published,
            fhe_processor,
        );
        let (result, ciphertext) = manager.start(e3_user_program::policy())?;
        let proof = encode_mock_compute_proof(&[3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5], &result)?;

        Ok((proof, ciphertext))
    })
    .build()?;

    server.run().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_proof_uses_the_current_solidity_envelope() {
        let result = ComputeResult {
            ciphertext_hash: vec![0; 32],
            ciphertext_commitment: vec![0; 32],
            params_hash: vec![0x22; 32],
            merkle_root: vec![0x33; 32],
        };

        let encoded = encode_mock_compute_proof(&[1, 2, 3], &result).unwrap();

        assert_eq!(encoded.len(), 160);
        assert_eq!(encoded[31], 96);
        assert_eq!(&encoded[32..64], &[0x22; 32]);
        assert_eq!(&encoded[64..96], &[0x33; 32]);
        assert_eq!(&encoded[120..128], &3_u64.to_be_bytes());
        assert_eq!(&encoded[128..131], &[1, 2, 3]);
        assert!(encoded[131..].iter().all(|byte| *byte == 0));
    }
}
