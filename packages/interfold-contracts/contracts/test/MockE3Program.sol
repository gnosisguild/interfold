// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { IE3Program } from "../interfaces/IE3Program.sol";

/// @title MockE3Program
/// @notice Provides a stateless BFV program for protocol tests on public networks.
/// @dev This contract applies no application-specific input or output rules. Interfold still
///      verifies the BFV ciphertext proof and the committee decryption proof for every E3.
contract MockE3Program is IE3Program {
    bytes32 public constant ENCRYPTION_SCHEME_ID = keccak256("fhe.rs:BFV");

    /// @notice Emitted when a caller publishes test input data.
    event InputPublished(
        uint256 indexed e3Id,
        address indexed publisher,
        bytes data
    );

    /// @inheritdoc IE3Program
    function validate(
        uint256,
        uint256,
        bytes calldata,
        bytes calldata,
        bytes calldata
    ) external pure returns (bytes32) {
        return ENCRYPTION_SCHEME_ID;
    }

    /// @inheritdoc IE3Program
    function publishInput(uint256 e3Id, bytes memory data) external {
        emit InputPublished(e3Id, msg.sender, data);
    }

    /// @inheritdoc IE3Program
    function verify(
        uint256,
        bytes32,
        bytes32,
        bytes memory
    ) external pure returns (bool success) {
        return true;
    }
}
