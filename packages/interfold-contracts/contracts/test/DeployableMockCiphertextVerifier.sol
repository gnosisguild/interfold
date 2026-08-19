// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { ICiphertextVerifier } from "../interfaces/ICiphertextVerifier.sol";

/// @title DeployableMockCiphertextVerifier
/// @notice Stateless ciphertext verifier for public-network rehearsal deployments.
/// @dev Always accepts proofs. Replace before enabling production CRISP workloads.
contract DeployableMockCiphertextVerifier is ICiphertextVerifier {
    function verify(
        uint256,
        bytes32,
        bytes32,
        bytes32,
        bytes32,
        bytes32,
        bytes calldata
    ) external pure override returns (bool) {
        return true;
    }
}
