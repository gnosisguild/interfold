// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/**
 * @title IBondedCheckpoints
 * @notice Historical bonded-collateral totals, so bonded FOLD can carry voting power.
 */
interface IBondedCheckpoints {
    /// @notice Thrown when a caller other than the registry tries to write history.
    error OnlyRegistry(address caller);

    /// @notice Thrown when a constructor argument is the zero address.
    error ZeroAddress();

    /// @notice A lookup named a timepoint that has not settled yet.
    error FutureLookup(uint256 timepoint, uint48 clock);

    /// @notice Emitted whenever an owner's bonded total is recorded.
    event BondedCheckpointed(
        address indexed bondOwner,
        uint48 indexed timepoint,
        uint256 amount
    );

    /**
     * @notice Record an owner's current bonded total.
     * @dev Takes the total rather than a delta on purpose. The history then mirrors the
     * registry's `_bondedByOwner` mapping, so a mutation site that forgets to call this is caught
     * by comparing the two — a delta-derived history would drift undetected, and it would drift
     * in voting weight.
     * @param bondOwner The owner whose total changed.
     * @param amount The owner's bonded total after the change.
     */
    function sync(address bondOwner, uint256 amount) external;

    /**
     * @notice Get an owner's bonded total at a past timepoint.
     * @param account The owner to read.
     * @param timepoint Timepoint in this contract's ERC-6372 clock units, which must have settled.
     * @return The bonded total at that timepoint.
     */
    function getPastBonded(
        address account,
        uint256 timepoint
    ) external view returns (uint256);

    /**
     * @notice Get an owner's bonded total right now.
     * @dev Distinct from `getPastBonded(account, clock() - 1)`, which is a timepoint behind. A
     * caller mixing that with a current wallet balance would sum two different instants, and a
     * bond that changed in this block would make the total exceed what the owner actually holds.
     * @param account The owner to read.
     * @return The bonded total at the latest checkpoint.
     */
    function bonded(address account) external view returns (uint256);

    /**
     * @notice Get the current timepoint, in ERC-6372 clock units.
     * @return Current timepoint.
     */
    function clock() external view returns (uint48);

    /**
     * @notice Get the ERC-6372 description of this contract's clock.
     * @return Machine-readable clock mode.
     */
    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() external pure returns (string memory);

    /**
     * @notice The registry allowed to write history.
     * @return The registry address.
     */
    function registry() external view returns (address);
}
