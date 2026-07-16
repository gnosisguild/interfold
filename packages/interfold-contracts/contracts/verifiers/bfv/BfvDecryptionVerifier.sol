// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { IDecryptionVerifier } from "../../interfaces/IDecryptionVerifier.sol";
import { ICircuitVerifier } from "../../interfaces/ICircuitVerifier.sol";
import { CommitteeHashLib } from "../../lib/CommitteeHashLib.sol";

/**
 * @title BfvDecryptionVerifier
 * @notice Verifies the DecryptionAggregator (EVM) proof produced by the
 *         recursive aggregation pipeline (C6 folds + C7/decrypted_shares
 *         verified internally) and binds it to the full on-chain call context.
 * @dev Used when the Interfold is configured with encryptionSchemeId
 *      keccak256("fhe.rs:BFV"). Constructor `threshold` must match the
 *      compiled DecryptionAggregator circuit `T` (`lib::configs::default::T`).
 *
 *      Expected `publicInputs` layout for DecryptionAggregator EVM outputs:
 *        [0]                = expectedC6FoldKeyHash  (VK anchor)
 *        [1]                = expectedC7KeyHash      (VK anchor)
 *        [2]                = committee_hash_hi
 *        [3]                = committee_hash_lo
 *        [4]                = decryption_domain_hi
 *        [5]                = decryption_domain_lo
 *        [6 .. 6+1+(3*(T+1))) = circuit-internal (sk, esm, ct columns)
 *        [last 100]         = plaintext message coefficients (100 u64 LE)
 *        Total: expectedPublicInputsLen = 6 + 1 + 3*(T+1) + 100.
 *
 *      The two VK-hash slots are checked against contract immutables set at
 *      construction; this anchors the recursive aggregation trust and
 *      prevents a malicious aggregator from substituting a forged sub-VK.
 *
 *      Each secret-bearing C6 proof commits to the same domain limbs. C6Fold
 *      preserves them, and DecryptionAggregator exposes them here, so an
 *      aggregator cannot re-label an existing proof for another E3.
 */
contract BfvDecryptionVerifier is IDecryptionVerifier {
    error InvalidCircuitVerifier(address verifier);
    error InvalidVerificationKeyHash();

    /// @dev Message is always the last 100 public inputs (100 uint64 coeffs = 800 bytes plaintext).
    uint256 internal constant MESSAGE_COEFFS_COUNT = 100;

    /// @dev `decryption_aggregator` return tail: `1 + 3*(T+1) + MESSAGE_COEFFS_COUNT` fields.
    uint256 internal constant DEC_RETURN_PREFIX_LEN = 1;

    /// @dev `decryption_aggregator` return columns after the leading key hash (sk, esm, ct).
    uint256 internal constant DEC_RETURN_COLUMN_COUNT = 3;

    /// @dev `publicInputs` index for `committee_hash_hi` (after sub-circuit key hashes).
    uint256 internal constant COMMITTEE_HASH_HI_IDX = 2;

    /// @dev `publicInputs` index for `committee_hash_lo`.
    uint256 internal constant COMMITTEE_HASH_LO_IDX = 3;

    /// @dev Public input indices for the E3 decryption-domain limbs.
    uint256 internal constant DECRYPTION_DOMAIN_HI_IDX = 4;
    uint256 internal constant DECRYPTION_DOMAIN_LO_IDX = 5;

    /// @notice BFV threshold `T`; must match the compiled DecryptionAggregator circuit.
    uint256 public immutable threshold;

    /// @dev `6 + DEC_RETURN_PREFIX_LEN + DEC_RETURN_COLUMN_COUNT*(T+1) + MESSAGE_COEFFS_COUNT`.
    uint256 internal immutable expectedPublicInputsLen;

    /// @notice Underlying Honk verifier for the DecryptionAggregator circuit.
    ICircuitVerifier public immutable circuitVerifier;

    /// @notice keccak256 commitment to the C6-fold recursive VK; expected at
    ///         `publicInputs[0]`. Provenance: `bb verify_key -b
    ///         circuits/bin/recursive_aggregation/c6_fold/target/...` -- pinned
    ///         at deployment time.
    bytes32 public immutable expectedC6FoldKeyHash;

    /// @notice keccak256 commitment to the C7 (decrypted_shares_aggregation)
    ///         recursive VK; expected at `publicInputs[1]`. Same provenance.
    bytes32 public immutable expectedC7KeyHash;

    constructor(
        address _circuitVerifier,
        bytes32 _expectedC6FoldKeyHash,
        bytes32 _expectedC7KeyHash,
        uint256 _threshold
    ) {
        require(_threshold > 0, "BfvDecryptionVerifier: threshold=0");
        if (_circuitVerifier.code.length == 0) {
            revert InvalidCircuitVerifier(_circuitVerifier);
        }
        if (
            _expectedC6FoldKeyHash == bytes32(0) ||
            _expectedC7KeyHash == bytes32(0)
        ) revert InvalidVerificationKeyHash();
        threshold = _threshold;
        expectedPublicInputsLen =
            6 +
            DEC_RETURN_PREFIX_LEN +
            (DEC_RETURN_COLUMN_COUNT * (_threshold + 1)) +
            MESSAGE_COEFFS_COUNT;

        circuitVerifier = ICircuitVerifier(_circuitVerifier);
        expectedC6FoldKeyHash = _expectedC6FoldKeyHash;
        expectedC7KeyHash = _expectedC7KeyHash;
    }

    /// @inheritdoc IDecryptionVerifier
    function verify(
        bytes32 decryptionDomain,
        bytes32 plaintextOutputHash,
        bytes32 committeeHash,
        bytes calldata proof
    ) external view override returns (bool) {
        (bytes memory rawProof, bytes32[] memory publicInputs) = abi.decode(
            proof,
            (bytes, bytes32[])
        );

        if (publicInputs.length != expectedPublicInputsLen) {
            revert InvalidPublicInputsLength();
        }

        // Anchor recursive-aggregation trust to immutable VK hashes.
        if (publicInputs[0] != expectedC6FoldKeyHash) {
            revert VkHashMismatch();
        }
        if (publicInputs[1] != expectedC7KeyHash) {
            revert VkHashMismatch();
        }

        // Bind to the on-chain committee hash (hi/lo split per Noir field convention).
        if (
            publicInputs[COMMITTEE_HASH_HI_IDX] !=
            CommitteeHashLib.hi(committeeHash)
        ) {
            revert DomainBindingMismatch();
        }
        if (
            publicInputs[COMMITTEE_HASH_LO_IDX] !=
            CommitteeHashLib.lo(committeeHash)
        ) {
            revert DomainBindingMismatch();
        }
        if (
            publicInputs[DECRYPTION_DOMAIN_HI_IDX] !=
            CommitteeHashLib.hi(decryptionDomain)
        ) {
            revert DomainBindingMismatch();
        }
        if (
            publicInputs[DECRYPTION_DOMAIN_LO_IDX] !=
            CommitteeHashLib.lo(decryptionDomain)
        ) {
            revert DomainBindingMismatch();
        }

        // Plaintext hash check: 100-coefficient plaintext must hash to the claimed value.
        if (!_verifyPlaintextHash(publicInputs, plaintextOutputHash)) {
            revert PlaintextHashMismatch();
        }

        // Bubble up as a revert instead of a silent `false`.
        if (!circuitVerifier.verify(rawProof, publicInputs)) {
            revert InvalidProof();
        }
        return true;
    }

    function _verifyPlaintextHash(
        bytes32[] memory publicInputs,
        bytes32 expected
    ) internal view returns (bool) {
        uint256 offset = expectedPublicInputsLen - MESSAGE_COEFFS_COUNT;
        bytes memory plaintext = new bytes(MESSAGE_COEFFS_COUNT * 8);
        for (uint256 i = 0; i < MESSAGE_COEFFS_COUNT; i++) {
            uint64 coeff = uint64(uint256(publicInputs[offset + i]));
            for (uint256 j = 0; j < 8; j++) {
                plaintext[i * 8 + j] = bytes1(uint8(coeff >> (j * 8)));
            }
        }
        return keccak256(plaintext) == expected;
    }
}
