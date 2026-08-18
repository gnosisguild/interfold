// SPDX-License-Identifier: LGPL-3.0-only
import { deployMockBondingRegistryProxy } from "../sale/deployContracts";
import { arg, connect } from "./cli";
import { ZERO } from "./constants";
import { protocolDir, writeJson } from "./files";
import type { ProtocolConfigFile } from "./types";
import { address, deployedAddress, requireContract } from "./values";

export async function actionPrepareRehearsal(): Promise<void> {
  const { ethers } = await connect();
  const network = await ethers.provider.getNetwork();
  const chainId = Number(network.chainId);
  if (chainId !== 11155111 && chainId !== 31337) {
    throw new Error(
      "Protocol rehearsal preparation is restricted to Sepolia and local Hardhat",
    );
  }

  const [operator] = await ethers.getSigners();
  const protocolOwner = address(await operator.getAddress(), "protocolOwner");
  const e3Program = address(arg("e3-program") ?? "", "e3-program");
  const ciphertextVerifier = address(
    arg("ciphertext-verifier") ?? "",
    "ciphertext-verifier",
  );
  await Promise.all([
    requireContract(ethers.provider, e3Program, "e3-program"),
    requireContract(ethers.provider, ciphertextVerifier, "ciphertext-verifier"),
  ]);

  const registry = await deployMockBondingRegistryProxy(ethers, protocolOwner);
  const latest = await ethers.provider.getBlock("latest");
  if (!latest) throw new Error("Could not read the latest block");
  const day = 24n * 60n * 60n;
  const ccaStart = BigInt(latest.timestamp) + day;
  const ccaEnd = ccaStart + day;
  const noMoreLocks = ccaEnd + 41n * day;
  const fold = await ethers.deployContract("InterfoldToken", [
    protocolOwner,
    ccaStart,
    ccaEnd,
    noMoreLocks,
    registry.proxy,
  ]);
  await fold.waitForDeployment();
  const feeToken = await ethers.deployContract("MockFeeOnTransferToken", [0]);
  await feeToken.waitForDeployment();
  const ticketUnderlying = await ethers.deployContract(
    "MockFeeOnTransferToken",
    [0],
  );
  await ticketUnderlying.waitForDeployment();

  const config: ProtocolConfigFile = {
    name:
      chainId === 11155111
        ? "sepolia-protocol-rehearsal"
        : "localhost-protocol-rehearsal",
    chainId,
    protocolOwner,
    fold: await deployedAddress(fold),
    bondingRegistryProxy: registry.proxy,
    bondingRegistryProxyAdmin: registry.proxyAdmin,
    feeToken: await deployedAddress(feeToken),
    feeTokenDecimals: 18,
    ticketUnderlyingToken: await deployedAddress(ticketUnderlying),
    protocolTreasury: protocolOwner,
    slashedFundsTreasury: protocolOwner,
    slasher: ZERO,
    ticketToken: { lockRegistry: true },
    bonding: {
      ticketPrice: "1000000000000000000000",
      requiredCiphernodeBond: "32000000000000000000000",
      ticketTokenDecimals: 18,
      ciphernodeBondTokenDecimals: 18,
      minTicketBalance: "1",
      exitDelay: "2592000",
    },
    registry: { sortitionSubmissionWindow: "600" },
    slashing: { initialDelay: "172800" },
    interfold: {
      maxDuration: "2592000",
      markFailedGracePeriod: "3600",
      timeoutConfig: {
        dkgWindow: "21600",
        computeWindow: "604800",
        decryptionWindow: "21600",
      },
      pricing: {
        keyGenFixedPerNode: "100000000000000000",
        keyGenPerEncryptionProof: "100000000000000000",
        coordinationPerPair: "10000000000000000",
        availabilityPerNodePerSec: "10000000000000",
        decryptionPerNode: "300000000000000000",
        publicationBase: "1000000000000000000",
        verificationPerProof: "5000000000000000",
        protocolTreasury: protocolOwner,
        marginBps: "1000",
        protocolShareBps: "182",
        dkgUtilizationBps: "3000",
        computeUtilizationBps: "4000",
        decryptUtilizationBps: "3000",
        minCommitteeSize: "3",
        minThreshold: "2",
      },
      committeeThresholds: [{ size: "0", quorum: "2", total: "3" }],
      registerActiveBfvParamSet: true,
      allowFeeToken: true,
    },
    verifiers: { deploy: true },
    ciphertextVerifier,
    bindInitialE3Program: true,
    e3Programs: [e3Program],
  };

  const file = `${protocolDir}/${config.name}.config.json`;
  writeJson(file, config);
  console.log(`
Protocol rehearsal prerequisites deployed
  protocol owner:              ${protocolOwner}
  FOLD test token:             ${config.fold}
  fee test token:              ${config.feeToken}
  ticket-underlying test token:${config.ticketUnderlyingToken}
  BondingRegistry proxy:       ${config.bondingRegistryProxy}
  BondingRegistry ProxyAdmin:  ${config.bondingRegistryProxyAdmin}
  CRISP program:               ${e3Program}
  ciphertext verifier:         ${ciphertextVerifier}
  config:                      ${file}
`);
}
