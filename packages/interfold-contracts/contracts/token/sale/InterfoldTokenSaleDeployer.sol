// SPDX-License-Identifier: LGPL-3.0-only
pragma solidity 0.8.28;

import {
    Distribution,
    ILBPStrategy,
    ILiquidityLauncher,
    MigratorParameters,
    PoolParameters
} from "../../interfaces/external/IUniswapLiquidityLauncher.sol";

/// @notice The minimal slice of {InterfoldToken} the deployer needs.
interface IFoldToken {
    function mint(address recipient, uint256 amount, bytes32 label) external;

    function setTransferWhitelisted(address account, bool whitelisted) external;

    function transferOwnership(address newOwner) external;

    function owner() external view returns (address);
}

/**
 * @title InterfoldTokenSaleDeployer
 * @notice Operator-callable sale deployment factory. The Safe passed as
 *         `protocolAdmin` becomes the FOLD owner; the caller only pays gas.
 * @dev The CCA auction address is discovered by the script from LBPStrategy
 *      logs after deployment. The Safe then accepts FOLD ownership and sets
 *      FOLD's claim source once.
 */
contract InterfoldTokenSaleDeployer {
    /// @param liquidityLauncher The deployed Uniswap LiquidityLauncher.
    /// @param lbpStrategy The deployed Uniswap LBPStrategy singleton.
    /// @param ccaStart FOLD CCA lifecycle start timestamp.
    /// @param ccaEnd FOLD CCA lifecycle end timestamp.
    /// @param noMoreLocks FOLD lock sunset timestamp.
    /// @param bondingRegistry Bonding registry proxy used by FOLD.
    /// @param auctionAmount FOLD sent to the CCA auction.
    /// @param reservedTokenAmountForLP FOLD reserved by LBPStrategy for LP.
    /// @param distributionSalt Salt passed to LiquidityLauncher.distributeToken.
    /// @param currency Auction/LBP currency; zero means native ETH.
    /// @param migrationBlock Block after which LBP migration can run.
    /// @param recipient Recipient for LBP migration proceeds.
    /// @param positionRecipient Recipient for LP positions.
    /// @param poolParameters Uniswap v4 pool settings for migration.
    /// @param positionDefinitions Encoded LBPStrategy position definitions.
    /// @param lpAllocationSchedule Encoded LBPStrategy currency allocation.
    /// @param auctionConfigData Encoded CCA auction parameters.
    /// @param saleLabel Label recorded on the FOLD mint event.
    /// @param foldInitCodeHash keccak256 of the FOLD creation code + constructor args.
    struct LbpSaleConfig {
        address liquidityLauncher;
        address lbpStrategy;
        uint64 ccaStart;
        uint64 ccaEnd;
        uint64 noMoreLocks;
        address bondingRegistry;
        uint256 auctionAmount;
        uint256 reservedTokenAmountForLP;
        bytes32 distributionSalt;
        address currency;
        uint64 migrationBlock;
        address recipient;
        address positionRecipient;
        PoolParameters poolParameters;
        bytes positionDefinitions;
        bytes lpAllocationSchedule;
        bytes auctionConfigData;
        bytes32 saleLabel;
        bytes32 foldInitCodeHash;
    }

    error ZeroAddress();
    error ConfigAlreadyUsed(bytes32 configHash);
    error FoldInitCodeMismatch();
    error FoldDeployFailed();
    error SaleAmountTooLarge();
    error FoldOwnershipNotRetained(address owner);
    error ArithmeticOverflow();

    /// @notice The Safe that becomes FOLD owner/admin.
    // solhint-disable-next-line immutable-vars-naming
    address public immutable protocolAdmin;

    /// @notice Replay guard: each config can be deployed exactly once.
    mapping(bytes32 configHash => bool used) public usedConfigHashes;

    /// @notice Emitted once a sale is fully deployed and funded.
    event SaleDeployed(
        bytes32 indexed configHash,
        address indexed fold,
        uint256 saleAmount,
        address operator
    );

    /// @notice Emitted for LiquidityLauncher/LBP deployments.
    event LbpSaleDeployed(
        bytes32 indexed configHash,
        address indexed fold,
        address liquidityLauncher,
        address lbpStrategy,
        uint256 auctionAmount,
        uint256 reservedTokenAmountForLP,
        address operator
    );

    constructor(address protocolAdmin_) {
        if (protocolAdmin_ == address(0)) revert ZeroAddress();
        protocolAdmin = protocolAdmin_;
    }

    /**
     * @notice Deploys FOLD and starts a Uniswap LiquidityLauncher/LBPStrategy
     *         distribution in one transaction.
     * @dev Callable by the operator wallet. FOLD ownership is transferred to
     *      the Safe at the end, but remains pending until the Safe accepts it.
     */
    function deploySaleWithLiquidityLauncher(
        LbpSaleConfig calldata config,
        bytes calldata foldInitCode
    ) external returns (address fold) {
        bytes32 configHash = _checkLbpConfig(config, foldInitCode);

        fold = _create(foldInitCode);
        if (IFoldToken(fold).owner() != address(this)) {
            revert FoldOwnershipNotRetained(IFoldToken(fold).owner());
        }

        uint256 distributionAmount = config.auctionAmount +
            config.reservedTokenAmountForLP;
        if (distributionAmount < config.auctionAmount) {
            revert ArithmeticOverflow();
        }
        if (distributionAmount > type(uint128).max) {
            revert SaleAmountTooLarge();
        }

        _prepareLbpTransferAllowances(fold, config);

        IFoldToken(fold).mint(
            config.liquidityLauncher,
            distributionAmount,
            config.saleLabel
        );

        ILiquidityLauncher(config.liquidityLauncher).distributeToken(
            fold,
            Distribution({
                strategy: config.lbpStrategy,
                amount: uint128(distributionAmount),
                configData: _lbpConfigData(fold, config)
            }),
            config.distributionSalt
        );

        IFoldToken(fold).transferOwnership(protocolAdmin);

        emit SaleDeployed(configHash, fold, config.auctionAmount, msg.sender);
        emit LbpSaleDeployed(
            configHash,
            fold,
            config.liquidityLauncher,
            config.lbpStrategy,
            config.auctionAmount,
            config.reservedTokenAmountForLP,
            msg.sender
        );
    }

    function hashLbpConfig(
        LbpSaleConfig calldata config
    ) public view returns (bytes32) {
        return
            keccak256(
                abi.encode(
                    block.chainid,
                    address(this),
                    config.liquidityLauncher,
                    config.lbpStrategy,
                    keccak256(
                        abi.encode(
                            config.ccaStart,
                            config.ccaEnd,
                            config.noMoreLocks,
                            config.bondingRegistry
                        )
                    ),
                    keccak256(
                        abi.encode(
                            config.auctionAmount,
                            config.reservedTokenAmountForLP,
                            config.distributionSalt
                        )
                    ),
                    _hashMigrationConfig(config),
                    keccak256(config.auctionConfigData),
                    config.saleLabel,
                    config.foldInitCodeHash
                )
            );
    }

    function _hashMigrationConfig(
        LbpSaleConfig calldata config
    ) internal pure returns (bytes32) {
        return
            keccak256(
                abi.encode(
                    config.currency,
                    config.migrationBlock,
                    config.recipient,
                    config.positionRecipient,
                    config.poolParameters.fee,
                    config.poolParameters.tickSpacing,
                    config.poolParameters.hook,
                    keccak256(config.positionDefinitions),
                    keccak256(config.lpAllocationSchedule)
                )
            );
    }

    function _checkLbpConfig(
        LbpSaleConfig calldata config,
        bytes calldata foldInitCode
    ) internal returns (bytes32 configHash) {
        if (
            config.liquidityLauncher == address(0) ||
            config.lbpStrategy == address(0) ||
            config.bondingRegistry == address(0) ||
            config.recipient == address(0) ||
            config.positionRecipient == address(0)
        ) revert ZeroAddress();
        if (config.auctionAmount > type(uint128).max) {
            revert SaleAmountTooLarge();
        }
        if (config.reservedTokenAmountForLP > type(uint128).max) {
            revert SaleAmountTooLarge();
        }
        if (keccak256(foldInitCode) != config.foldInitCodeHash) {
            revert FoldInitCodeMismatch();
        }

        configHash = hashLbpConfig(config);
        if (usedConfigHashes[configHash]) revert ConfigAlreadyUsed(configHash);
        usedConfigHashes[configHash] = true;
    }

    function _lbpConfigData(
        address fold,
        LbpSaleConfig calldata config
    ) internal pure returns (bytes memory) {
        MigratorParameters memory migratorParams = MigratorParameters({
            token: fold,
            currency: config.currency,
            migrationBlock: config.migrationBlock,
            reservedTokenAmountForLP: uint128(config.reservedTokenAmountForLP),
            recipient: config.recipient,
            positionRecipient: config.positionRecipient,
            poolParameters: config.poolParameters,
            positionDefinitions: config.positionDefinitions,
            lpAllocationSchedule: config.lpAllocationSchedule
        });
        return abi.encode(migratorParams, config.auctionConfigData);
    }

    function _prepareLbpTransferAllowances(
        address fold,
        LbpSaleConfig calldata config
    ) internal {
        IFoldToken(fold).setTransferWhitelisted(config.liquidityLauncher, true);
        IFoldToken(fold).setTransferWhitelisted(config.lbpStrategy, true);

        address positionManager = ILBPStrategy(config.lbpStrategy)
            .positionManager();
        if (positionManager != address(0)) {
            IFoldToken(fold).setTransferWhitelisted(positionManager, true);
        }
    }

    /// @dev Deploys `initCode` with plain CREATE and returns the new address.
    function _create(bytes calldata initCode) internal returns (address addr) {
        bytes memory code = initCode;
        // solhint-disable-next-line no-inline-assembly
        assembly ("memory-safe") {
            addr := create(0, add(code, 0x20), mload(code))
        }
        if (addr == address(0)) revert FoldDeployFailed();
    }
}
