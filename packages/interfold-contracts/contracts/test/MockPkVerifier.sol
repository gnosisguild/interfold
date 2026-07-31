// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { IPkVerifier } from "../interfaces/IPkVerifier.sol";

contract MockPkVerifier is IPkVerifier {
    bytes4 private constant _RETURN_FALSE_MAGIC = 0xfafafafa;

    /// @dev Permissive test mock: only enforces the pk-commitment slot the
    ///      real wrapper enforces, so existing fixtures (`[pkCommitment]`)
    ///      keep working. Intentionally ignores VK-hash slots and domain
    ///      binding — those are exercised by `BfvPkVerifier.spec.ts` against
    ///      the real wrapper.
    function verify(
        uint256,
        uint256,
        address[] calldata,
        bytes32 pkCommitment,
        bytes32,
        bytes calldata proof
    ) external pure returns (bool) {
        (bytes memory rawProof, bytes32[] memory publicInputs) = abi.decode(
            proof,
            (bytes, bytes32[])
        );
        if (publicInputs.length == 0) revert InvalidPublicInputsLength();
        if (publicInputs[publicInputs.length - 1] != pkCommitment) {
            revert PkCommitmentMismatch();
        }
        if (
            keccak256(rawProof) ==
            keccak256(abi.encodePacked(_RETURN_FALSE_MAGIC))
        ) return false;
        return true;
    }
}
