// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/// @title IRandomnessProvider
/// @notice Supplies one asynchronous random word for each E3 committee request.
interface IRandomnessProvider {
    /// @notice Emitted after the provider accepts one E3 request.
    event RandomnessRequested(uint256 indexed requestId, uint256 indexed e3Id);

    /// @notice Emitted after the provider records one coordinator response.
    event RandomnessFulfilled(
        uint256 indexed requestId,
        uint256 indexed e3Id,
        uint256 randomWord,
        uint256 fulfilledAt
    );

    /// @notice Returns the only contract that can request randomness.
    function requester() external view returns (address);

    /// @notice Requests one random word for an E3.
    /// @param e3Id ID of the E3 that will use the random word.
    /// @return requestId Provider-specific request identifier.
    function requestRandomness(
        uint256 e3Id
    ) external returns (uint256 requestId);

    /// @notice Returns the result for a request.
    /// @param requestId Provider-specific request identifier.
    /// @return fulfilled Whether the provider recorded a valid response.
    /// @return randomWord Random word returned by the provider.
    /// @return fulfilledAt Timestamp when the provider recorded the response.
    /// @return fulfilledBlock Ethereum block when the provider recorded the response.
    function getRandomness(
        uint256 requestId
    )
        external
        view
        returns (
            bool fulfilled,
            uint256 randomWord,
            uint256 fulfilledAt,
            uint256 fulfilledBlock
        );
}
