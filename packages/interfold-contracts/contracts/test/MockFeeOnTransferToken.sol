// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice Test ERC-20 that redirects a configurable fee to `0xdead`.
/// @dev The fee can reduce the recipient amount or add an extra sender debit.
contract MockFeeOnTransferToken is ERC20 {
    uint256 public feeBps;
    bool public feeIsChargedOnTop;

    constructor(uint256 _feeBps) ERC20("FoT", "FoT") {
        require(_feeBps <= 10_000, "fee>100%");
        feeBps = _feeBps;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function setFeeBps(uint256 newFeeBps) external {
        require(newFeeBps <= 10_000, "fee>100%");
        feeBps = newFeeBps;
    }

    function setFeeIsChargedOnTop(bool enabled) external {
        feeIsChargedOnTop = enabled;
    }

    function lockedBalanceOf(address) external pure returns (uint256) {
        return 0;
    }

    function _update(
        address from,
        address to,
        uint256 value
    ) internal override {
        if (from == address(0) || to == address(0) || feeBps == 0) {
            super._update(from, to, value);
            return;
        }
        uint256 fee = (value * feeBps) / 10_000;
        if (feeIsChargedOnTop) {
            super._update(from, to, value);
            if (fee > 0) {
                super._update(from, address(0xdead), fee);
            }
            return;
        }
        uint256 net = value - fee;
        super._update(from, to, net);
        if (fee > 0) {
            super._update(from, address(0xdead), fee);
        }
    }
}
