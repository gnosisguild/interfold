// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/**
 * @title ICiphertextVerifier
 * @notice Verifies that a ciphertext creates a valid decryption task for its E3.
 */
interface ICiphertextVerifier {
    /// @notice Verifies a ciphertext and its proof-derived commitment.
    /// @param e3Id The E3 that will receive the ciphertext.
    /// @param encryptionSchemeId The request-time encryption scheme.
    /// @param paramsHash The request-time BFV parameter hash.
    /// @param committeePublicKey The request-time committee public key commitment.
    /// @param ciphertextOutputHash The hash of the serialized ciphertext.
    /// @param ciphertextCommitment The SAFE commitment authenticated by the proof.
    /// @param proof The scheme verifier proof.
    function verify(
        uint256 e3Id,
        bytes32 encryptionSchemeId,
        bytes32 paramsHash,
        bytes32 committeePublicKey,
        bytes32 ciphertextOutputHash,
        bytes32 ciphertextCommitment,
        bytes calldata proof
    ) external view returns (bool);
}
