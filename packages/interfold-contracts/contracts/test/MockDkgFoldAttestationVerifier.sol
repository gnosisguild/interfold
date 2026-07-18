// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import {
    IDkgFoldAttestationVerifier
} from "../interfaces/IDkgFoldAttestationVerifier.sol";

/// @notice Test-only verifier used when ciphernodes skip recursive proof aggregation.
/// @dev Production deployments must use DkgFoldAttestationVerifier.
contract MockDkgFoldAttestationVerifier is IDkgFoldAttestationVerifier {
    function verify(
        address,
        uint256,
        uint256,
        bytes calldata,
        bytes calldata
    )
        external
        pure
        returns (
            uint256[] memory partyIds,
            bytes32[] memory skAggCommits,
            bytes32[] memory esmAggCommits
        )
    {
        return (new uint256[](0), new bytes32[](0), new bytes32[](0));
    }
}
