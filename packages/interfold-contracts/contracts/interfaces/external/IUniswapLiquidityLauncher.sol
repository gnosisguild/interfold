// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/// @notice One LiquidityLauncher distribution instruction.
struct Distribution {
    address strategy;
    uint128 amount;
    bytes configData;
}

/// @notice Generic Uniswap Liquidity Launcher distribution strategy.
interface IStrategy {
    function initializeDistribution(
        address token,
        uint256 totalSupply,
        bytes calldata configData,
        bytes32 salt
    ) external;
}

/// @notice Minimal interface for the canonical Uniswap LiquidityLauncher v3.
interface ILiquidityLauncher {
    function createToken(
        address factory,
        string calldata name,
        string calldata symbol,
        uint8 decimals,
        uint128 initialSupply,
        address recipient,
        bytes calldata tokenData
    ) external returns (address tokenAddress);

    function depositToken(address token, uint160 amount) external;

    function distributeToken(
        address tokenAddress,
        Distribution calldata distribution,
        bytes32 salt
    ) external;

    function getGraffiti(
        address originalCreator
    ) external view returns (bytes32 graffiti);
}

struct PoolParameters {
    uint24 fee;
    int24 tickSpacing;
    address hook;
}

/// @notice Uniswap LiquidityLauncher v3 LBP migration parameters.
/// @dev Field order must match the deployed LBPStrategy v3 ABI.
struct MigratorParameters {
    address token;
    address currency;
    uint64 migrationBlock;
    uint128 reservedTokenAmountForLP;
    address recipient;
    address positionRecipient;
    PoolParameters poolParameters;
    bytes positionDefinitions;
    bytes lpAllocationSchedule;
}

struct LiquidityAllocationBracket {
    uint128 lowerThreshold;
    uint24 rate;
}

struct PositionDefinition {
    int24 offsetLower;
    int24 offsetUpper;
    uint24 weight;
    address overridePositionRecipient;
}

/// @notice Minimal initializer interface shared by CCA auctions in LBP mode.
interface ILBPInitializer {
    function token() external view returns (address);

    function currency() external view returns (address);

    function totalSupply() external view returns (uint128);

    function tokensRecipient() external view returns (address);

    function fundsRecipient() external view returns (address);

    function startBlock() external view returns (uint64);

    function endBlock() external view returns (uint64);
}

/// @notice Minimal interface for the canonical Uniswap LBPStrategy v3.
interface ILBPStrategy is IStrategy {
    function initializerFactory() external view returns (address);

    function positionManager() external view returns (address);

    function poolManager() external view returns (address);

    function initializers(
        address initializer
    ) external view returns (MigratorParameters memory);

    function migrate(address initializer) external;
}
