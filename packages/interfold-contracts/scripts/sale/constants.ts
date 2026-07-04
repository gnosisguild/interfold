// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

export const ZERO = ethersLib.ZeroAddress;
export const MSG_SENDER_SENTINEL = "0x0000000000000000000000000000000000000001";
export const DAY = 24n * 60n * 60n;
export const FORTY_DAYS = 40n * DAY;
export const FOUR_YEARS = 4n * 365n * DAY;
export const ONE_MONTH = 30n * DAY;
export const LOCK_SUNSET_DELAY = FOUR_YEARS + ONE_MONTH;
export const DEFAULT_SALE_AMOUNT = ethersLib.parseEther("120000000").toString();

export const FOLD_TOKEN_SAFE_ABI = [
  "function setClaimSource(address claimSource)",
];

export const PREDICATE_VALIDATION_HOOK_ABI = [
  "function auction() view returns (address)",
  "function owner() view returns (address)",
  "function getRegistry() view returns (address)",
  "function getPolicyID() view returns (string)",
  "function requireSenderIsOwner() view returns (bool)",
  "function setAuction(address auction)",
];

export const abi = ethersLib.AbiCoder.defaultAbiCoder();
