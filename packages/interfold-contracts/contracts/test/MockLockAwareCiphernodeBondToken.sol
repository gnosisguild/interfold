// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract MockLockAwareCiphernodeBondToken is ERC20 {
    enum ResponseMode {
        Valid,
        Revert,
        Malformed
    }

    ResponseMode public responseMode;

    constructor(ResponseMode mode) ERC20("Lock-aware ciphernodeBond", "LOCK") {
        responseMode = mode;
    }

    function setResponseMode(ResponseMode mode) external {
        responseMode = mode;
    }

    function mint(address account, uint256 amount) external {
        _mint(account, amount);
    }

    function lockedBalanceOf(address) external view returns (uint256) {
        if (responseMode == ResponseMode.Revert) {
            revert("locked balance unavailable");
        }
        if (responseMode == ResponseMode.Malformed) {
            assembly ("memory-safe") {
                mstore(0, 0)
                return(0, 31)
            }
        }
        return 0;
    }
}
