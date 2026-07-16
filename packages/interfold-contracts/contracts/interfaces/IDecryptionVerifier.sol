// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/**
 * @title IDecryptionVerifier
 * @notice Interface for the DecryptionAggregator (EVM) proof verifier.
 * @dev The DecryptionAggregator circuit internally verifies the C6-fold and C7
 *      (decrypted_shares_aggregation) sub-proofs; this on-chain wrapper verifies
 *      the final EVM proof and enforces:
 *        - the immutable recursive sub-circuit VK hashes
 *        - the plaintext slot matches the caller-supplied hash
 *        - a domain-binding slot supplied by Interfold and derived from
 *          (chainId, Interfold address, e3Id, committeeHash,
 *           ciphertextOutputHash, committeePublicKey)
 *      and reverts on any mismatch.
 */
interface IDecryptionVerifier {
    /// @notice Proof was structurally well-formed but the underlying honk
    ///         verifier rejected it. Used in place of a `bool false` return.
    error InvalidProof();
    /// @notice `publicInputs` is shorter than the layout the wrapper expects
    ///         (must hold the two VK-hash slots, the domain-binding slot and the
    ///         100 message-coefficient slots).
    error InvalidPublicInputsLength();
    /// @notice One of the recursive-aggregation sub-circuit VK hashes embedded
    ///         in the proof does not match the immutable value committed at
    ///         construction time.
    error VkHashMismatch();
    /// @notice The 100 plaintext-coefficient slots do not hash to
    ///         `plaintextOutputHash`.
    error PlaintextHashMismatch();
    /// @notice The domain-binding public-input slot does not equal the value
    ///         recomputed on-chain from the call context.
    error DomainBindingMismatch();

    /// @notice Verify a DecryptionAggregator EVM proof and bind it to the E3
    ///         domain recomputed by Interfold.
    /// @param decryptionDomain `keccak256(abi.encode(chainId, interfold, e3Id,
    ///        committeeHash, ciphertextOutputHash, committeePublicKey))`.
    /// @param plaintextOutputHash `keccak256(plaintextOutput)` expected by the Interfold.
    /// @param committeeHash `keccak256(abi.encodePacked(topNodes))` for the on-chain committee.
    /// @param proof ABI-encoded `(bytes rawProof, bytes32[] publicInputs)`.
    /// @return success Always `true` on success; the wrapper reverts on any failure.
    function verify(
        bytes32 decryptionDomain,
        bytes32 plaintextOutputHash,
        bytes32 committeeHash,
        bytes calldata proof
    ) external view returns (bool success);
}
