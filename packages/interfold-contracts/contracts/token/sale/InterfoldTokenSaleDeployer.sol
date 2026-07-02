// SPDX-License-Identifier: LGPL-3.0-only
pragma solidity 0.8.28;

import { IContinuousClearingAuction } from "../../interfaces/external/ICCA.sol";
import {
    Distribution,
    ILBPStrategy,
    ILiquidityLauncher
} from "../../interfaces/external/IUniswapLiquidityLauncher.sol";

/// @notice The minimal slice of {InterfoldToken} the deployer needs.
interface IFoldToken {
    function mint(address recipient, uint256 amount, bytes32 label) external;

    function setTransferWhitelisted(address account, bool whitelisted) external;

    function transferOwnership(address newOwner) external;

    function owner() external view returns (address);

    // solhint-disable-next-line func-name-mixedcase
    function CLAIM_SOURCE() external view returns (address);
}

/**
 * @title InterfoldTokenSaleDeployer
 * @notice Operator-callable sale deployment factory. The Safe passed as
 *         `protocolAdmin` becomes the FOLD owner; the caller only pays gas.
 * @dev FOLD and the CCA auction depend on each other's addresses. The deploy
 *      script predicts the auction created by Uniswap's LiquidityLauncher /
 *      LBPStrategy path, this contract checks the prediction on-chain, and the
 *      whole transaction reverts on any mismatch.
 */
contract InterfoldTokenSaleDeployer {
    /// @param liquidityLauncher The deployed Uniswap LiquidityLauncher.
    /// @param lbpStrategy The deployed Uniswap LBPStrategy singleton.
    /// @param expectedAuction The predicted CCA auction/initializer address.
    /// @param auctionAmount FOLD sent to the CCA auction.
    /// @param reservedTokenAmountForLP FOLD reserved by LBPStrategy for LP.
    /// @param distributionSalt Salt passed to LiquidityLauncher.distributeToken.
    /// @param lbpConfigData abi.encode(MigratorParameters, bytes auctionConfigData).
    /// @param saleLabel Label recorded on the FOLD mint event.
    /// @param foldInitCodeHash keccak256 of the FOLD creation code + constructor
    ///        args with claimSource = expectedAuction.
    struct LbpSaleConfig {
        address liquidityLauncher;
        address lbpStrategy;
        address expectedAuction;
        uint256 auctionAmount;
        uint256 reservedTokenAmountForLP;
        bytes32 distributionSalt;
        bytes lbpConfigData;
        bytes32 saleLabel;
        bytes32 foldInitCodeHash;
    }

    error ZeroAddress();
    error ConfigAlreadyUsed(bytes32 configHash);
    error FoldInitCodeMismatch();
    error FoldDeployFailed();
    error SaleAmountTooLarge();
    error AuctionMismatch(address expected, address actual);
    error FoldOwnershipNotRetained(address owner);
    error AuctionTokenMismatch(address expected, address actual);
    error AuctionSupplyMismatch(uint256 expected, uint256 actual);
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
        address indexed auction,
        uint256 saleAmount,
        address operator
    );

    /// @notice Emitted for LiquidityLauncher/LBP deployments.
    event LbpSaleDeployed(
        bytes32 indexed configHash,
        address indexed fold,
        address indexed auction,
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
     * @dev Callable by the operator wallet. FOLD's immutable {CLAIM_SOURCE}
     *      must equal the CCA auction address predicted from the LBP strategy's
     *      initializer factory flow.
     */
    function deploySaleWithLiquidityLauncher(
        LbpSaleConfig calldata config,
        bytes calldata foldInitCode
    ) external returns (address fold, address auction) {
        bytes32 configHash = _checkLbpConfig(config, foldInitCode);

        fold = _create(foldInitCode);
        if (IFoldToken(fold).owner() != address(this)) {
            revert FoldOwnershipNotRetained(IFoldToken(fold).owner());
        }

        auction = config.expectedAuction;
        if (IFoldToken(fold).CLAIM_SOURCE() != auction) {
            revert AuctionMismatch(IFoldToken(fold).CLAIM_SOURCE(), auction);
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
                configData: config.lbpConfigData
            }),
            config.distributionSalt
        );

        if (auction.code.length == 0)
            revert AuctionMismatch(auction, address(0));
        if (IContinuousClearingAuction(auction).token() != fold) {
            revert AuctionTokenMismatch(
                fold,
                IContinuousClearingAuction(auction).token()
            );
        }
        if (
            IContinuousClearingAuction(auction).totalSupply() !=
            config.auctionAmount
        ) {
            revert AuctionSupplyMismatch(
                config.auctionAmount,
                IContinuousClearingAuction(auction).totalSupply()
            );
        }

        IFoldToken(fold).transferOwnership(protocolAdmin);

        emit SaleDeployed(
            configHash,
            fold,
            auction,
            config.auctionAmount,
            msg.sender
        );
        emit LbpSaleDeployed(
            configHash,
            fold,
            auction,
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
                    config.expectedAuction,
                    config.auctionAmount,
                    config.reservedTokenAmountForLP,
                    config.distributionSalt,
                    keccak256(config.lbpConfigData),
                    config.saleLabel,
                    config.foldInitCodeHash
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
            config.expectedAuction == address(0)
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
