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
    plaintextModulus: 131072n,
    moduli: [0x0400000001460001n, 0x0400000000ea0001n, 0x0400000000920001n],
    error1Variance: "2331171231419734472395201298275918858425592709120",
  },
  secure16384: {
    degree: 16384n,
    plaintextModulus: 1000n,
    moduli: [
      0x00040000009f0001n,
      0x00040000008a0001n,
      0x0004000000800001n,
      0x00040000007e0001n,
      0x0004000000750001n,
    ],
    error1Variance: "4326914048779023023775413607683413333",
  },
} as const;
