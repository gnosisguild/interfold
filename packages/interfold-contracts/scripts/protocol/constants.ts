// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

export const ZERO = ethersLib.ZeroAddress;
export const ADDRESS_ONE = "0x0000000000000000000000000000000000000001";

export const abi = ethersLib.AbiCoder.defaultAbiCoder();

export const proxyAdminInterface = new ethersLib.Interface([
  "function owner() view returns (address)",
  "function upgradeAndCall(address proxy,address implementation,bytes data) payable",
]);

export const BFV_PARAMS = {
  insecure512: {
    degree: 512n,
    plaintextModulus: 100n,
    moduli: [0xffffee001n, 0xffffc4001n],
    error1Variance: "3",
  },
  secure8192: {
    degree: 8192n,
    plaintextModulus: 1000000n,
    moduli: [0x02000000015a0001n, 0x0200000001460001n, 0x0200000001210001n],
    error1Variance: "18148392902450051384713312396360971277653333",
  },
} as const;
