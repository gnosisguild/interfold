// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { ICiphertextVerifier } from "../interfaces/ICiphertextVerifier.sol";

/// @notice Declares the request-time ciphertext verifier state used by Interfold.
abstract contract CiphertextVerifierStorage {
    struct RequestConfig {
        ICiphertextVerifier verifier;
        bytes32 paramsHash;
    }

    /// @custom:storage-location erc7201:interfold.storage.CiphertextVerifier
    struct Layout {
        mapping(bytes32 schemeId => ICiphertextVerifier verifier) current;
        mapping(uint256 e3Id => RequestConfig config) requests;
    }
}
