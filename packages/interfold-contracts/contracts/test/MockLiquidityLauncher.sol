// SPDX-License-Identifier: LGPL-3.0-only
pragma solidity 0.8.28;

import {
    Distribution,
    IStrategy
} from "../interfaces/external/IUniswapLiquidityLauncher.sol";

interface IERC20AllowanceLike {
    function approve(address spender, uint256 amount) external returns (bool);

    function allowance(
        address owner,
        address spender
    ) external view returns (uint256);
}

contract MockLiquidityLauncher {
    event TokenDistributed(
        address indexed tokenAddress,
        address indexed strategy,
        uint256 amount
    );

    error AllowanceNotFullyConsumed();

    function distributeToken(
        address tokenAddress,
        Distribution calldata distribution,
        bytes32 salt
    ) external {
        IERC20AllowanceLike(tokenAddress).approve(
            distribution.strategy,
            distribution.amount
        );
        IStrategy(distribution.strategy).initializeDistribution(
            tokenAddress,
            distribution.amount,
            distribution.configData,
            keccak256(abi.encode(msg.sender, salt))
        );
        if (
            IERC20AllowanceLike(tokenAddress).allowance(
                address(this),
                distribution.strategy
            ) != 0
        ) {
            revert AllowanceNotFullyConsumed();
        }
        emit TokenDistributed(
            tokenAddress,
            distribution.strategy,
            distribution.amount
        );
    }
}
