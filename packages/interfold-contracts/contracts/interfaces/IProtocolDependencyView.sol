// SPDX-License-Identifier: LGPL-3.0-only

pragma solidity 0.8.28;

/// @notice Common address getters used to validate protocol wiring.
interface IProtocolDependencyView {
    function interfold() external view returns (address);

    function bondingRegistry() external view returns (address);

    function slashingManager() external view returns (address);

    function ciphernodeRegistry() external view returns (address);

    function e3RefundManager() external view returns (address);

    function registry() external view returns (address);
}
