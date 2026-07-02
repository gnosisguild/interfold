// SPDX-License-Identifier: LGPL-3.0-only
pragma solidity 0.8.28;

import {
    MigratorParameters
} from "../interfaces/external/IUniswapLiquidityLauncher.sol";

interface IERC20TransferFromLike {
    function transferFrom(
        address from,
        address to,
        uint256 amount
    ) external returns (bool);
}

interface IMockCCAFactory {
    function create(
        address token,
        uint256 amount,
        bytes calldata configData,
        bytes32 salt
    ) external returns (address auction);
}

interface IMockCCAAuction {
    function onTokensReceived() external;
}

contract MockLBPStrategy {
    address public immutable initializerFactory;
    address public immutable positionManager;
    address public immutable poolManager;

    mapping(address initializer => MigratorParameters) public initializers;

    event InitializerCreated(
        address indexed initializer,
        MigratorParameters migrationParams
    );

    constructor(
        address initializerFactory_,
        address positionManager_,
        address poolManager_
    ) {
        initializerFactory = initializerFactory_;
        positionManager = positionManager_;
        poolManager = poolManager_;
    }

    function initializeDistribution(
        address token,
        uint256 totalSupply,
        bytes calldata configData,
        bytes32 salt
    ) external {
        (
            MigratorParameters memory migrationParams,
            bytes memory initializerParams
        ) = abi.decode(configData, (MigratorParameters, bytes));

        uint256 auctionSupply = totalSupply -
            migrationParams.reservedTokenAmountForLP;
        bytes32 initializerSalt = keccak256(abi.encode(salt, migrationParams));
        address auction = IMockCCAFactory(initializerFactory).create(
            token,
            auctionSupply,
            initializerParams,
            initializerSalt
        );

        require(
            IERC20TransferFromLike(token).transferFrom(
                msg.sender,
                auction,
                auctionSupply
            ),
            "auction transfer"
        );
        require(
            IERC20TransferFromLike(token).transferFrom(
                msg.sender,
                address(this),
                migrationParams.reservedTokenAmountForLP
            ),
            "reserve transfer"
        );
        IMockCCAAuction(auction).onTokensReceived();
        initializers[auction] = migrationParams;
        emit InitializerCreated(auction, migrationParams);
    }
}
