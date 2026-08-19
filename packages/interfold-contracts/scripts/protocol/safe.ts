// SPDX-License-Identifier: LGPL-3.0-only
import SafeApiKit from "@safe-global/api-kit";
import Safe from "@safe-global/protocol-kit";
import { MetaTransactionData, OperationType } from "@safe-global/types-kit";
import { ethers as ethersLib } from "ethers";
import { stderr, stdin } from "node:process";
import readline from "node:readline/promises";
import { Writable } from "node:stream";

import { arg } from "./cli";
import { safeProposalPath, writeJson } from "./files";
import type {
  ProtocolConfigFile,
  SafeProposal,
  SafeTransaction,
} from "./types";
import { address } from "./values";

const aragonAdminPluginInterface = new ethersLib.Interface([
  "function executeProposal(bytes metadata,tuple(address to,uint256 value,bytes data)[] actions,uint256 allowFailureMap)",
]);

export function safeTx(to: string, data: string): SafeTransaction {
  return {
    to,
    value: "0",
    data,
    operation: 0,
    contractMethod: null,
    contractInputsValues: null,
  };
}

export function safeBatch(
  config: ProtocolConfigFile,
  transactions: SafeTransaction[],
) {
  return {
    version: "1.0",
    chainId: config.chainId.toString(),
    createdAt: Date.now(),
    meta: {
      name: `${config.name} protocol wiring`,
      description:
        "Upgrade the existing bonding registry proxy and wire the Interfold protocol contracts.",
      txBuilderVersion: "1.18.0",
      createdFromSafeAddress: config.safe,
    },
    transactions,
  };
}

export function governanceBatch(
  config: ProtocolConfigFile,
  transactions: SafeTransaction[],
) {
  if (config.safe) return safeBatch(config, transactions);
  return {
    version: "1.0",
    chainId: config.chainId.toString(),
    createdAt: Date.now(),
    meta: {
      name: `${config.name} protocol wiring`,
      description:
        "Upgrade the existing bonding registry proxy and wire the Interfold protocol contracts.",
      executor: config.protocolOwner,
    },
    transactions,
  };
}

export function aragonAdminSafeTransactions(
  config: ProtocolConfigFile,
  transactions: SafeTransaction[],
): SafeTransaction[] {
  if (!config.governance) {
    throw new Error("Aragon governance is not configured");
  }
  if (transactions.length === 0) {
    throw new Error("Governance batch has no transactions to wrap");
  }
  for (const [index, tx] of transactions.entries()) {
    if (tx.operation !== 0) {
      throw new Error(
        `Governance transaction ${index + 1} is not a CALL operation`,
      );
    }
  }

  const actions = transactions.map((tx) => ({
    to: tx.to,
    value: BigInt(tx.value),
    data: tx.data,
  }));
  const metadata = config.governance.proposalMetadata ?? "0x";
  return [
    safeTx(
      config.governance.adminPlugin,
      aragonAdminPluginInterface.encodeFunctionData("executeProposal", [
        metadata,
        actions,
        0n,
      ]),
    ),
  ];
}

export function aragonAdminSafeBatch(
  config: ProtocolConfigFile,
  transactions: SafeTransaction[],
) {
  if (!config.governance) {
    throw new Error("Aragon governance is not configured");
  }
  return {
    version: "1.0",
    chainId: config.chainId.toString(),
    createdAt: Date.now(),
    meta: {
      name: `${config.name} Aragon Admin proposal`,
      description:
        "Execute the protocol wiring actions through the Aragon Admin plugin.",
      txBuilderVersion: "1.18.0",
      createdFromSafeAddress: config.governance.proposerSafe,
    },
    transactions: aragonAdminSafeTransactions(config, transactions),
  };
}

function safeAppPrefix(chainId: number): string | undefined {
  if (chainId === 1) return "eth";
  if (chainId === 11155111) return "sep";
  if (chainId === 8453) return "base";
  if (chainId === 84532) return "basesep";
  if (chainId === 42161) return "arb1";
  if (chainId === 10) return "oeth";
  if (chainId === 137) return "matic";
  return undefined;
}

function safeTransactionUrl(
  chainId: number,
  safeAddress: string,
  safeTxHash: string,
) {
  const prefix = safeAppPrefix(chainId);
  if (!prefix) return undefined;
  return `https://app.safe.global/transactions/tx?safe=${prefix}:${safeAddress}&id=multisig_${safeAddress}_${safeTxHash}`;
}

async function readSecretFromStdin(prompt: string): Promise<string> {
  if (!stdin.isTTY) {
    throw new Error(
      "Run this command from an interactive terminal so the Safe proposal signing key can be read from stdin.",
    );
  }

  let muted = false;
  const hiddenOutput = new Writable({
    write(chunk, encoding, callback) {
      if (!muted) stderr.write(chunk, encoding);
      callback();
    },
  });
  const rl = readline.createInterface({
    input: stdin,
    output: hiddenOutput,
    terminal: true,
  });

  stderr.write(prompt);
  muted = true;
  const secret = (await rl.question("")).trim();
  muted = false;
  stderr.write("\n");
  rl.close();

  if (!secret) throw new Error("Safe proposal signing key is required.");
  return secret;
}

async function privateKeyForSafeProposal(): Promise<string> {
  return readSecretFromStdin("Safe proposal private key: ");
}

function rpcUrlForSafeProposal(): string {
  const rpcUrl = process.env.RPC_URL;
  if (!rpcUrl)
    throw new Error("Set RPC_URL so the Safe SDK can read the Safe.");
  return rpcUrl;
}

function toMetaTransaction(tx: SafeTransaction): MetaTransactionData {
  return {
    to: tx.to,
    value: tx.value,
    data: tx.data,
    operation:
      tx.operation === 1 ? OperationType.DelegateCall : OperationType.Call,
  };
}

export async function proposeSafeBatch(
  config: ProtocolConfigFile,
  transactions: SafeTransaction[],
  safeAddress = config.safe,
): Promise<SafeProposal> {
  if (!safeAddress) {
    throw new Error("No Safe is configured for this governance transaction.");
  }
  if (transactions.length === 0)
    throw new Error("Safe batch has no transactions to propose");

  const privateKey = await privateKeyForSafeProposal();
  const proposer = address(
    new ethersLib.Wallet(privateKey).address,
    "Safe proposal signer",
  );
  const apiKit = new SafeApiKit({
    chainId: BigInt(config.chainId),
    apiKey: process.env.SAFE_API_KEY || undefined,
    txServiceUrl: process.env.SAFE_TX_SERVICE_URL || undefined,
  });
  const protocolKit = await Safe.init({
    provider: rpcUrlForSafeProposal(),
    signer: privateKey,
    safeAddress,
  });

  const nonce = Number(
    arg("safe-nonce") ?? (await apiKit.getNextNonce(safeAddress)),
  );
  const origin = arg("origin") ?? `Interfold ${config.name} protocol wiring`;
  const safeTransaction = await protocolKit.createTransaction({
    transactions: transactions.map(toMetaTransaction),
    onlyCalls: true,
    options: { nonce },
  });
  const safeTxHash = await protocolKit.getTransactionHash(safeTransaction);
  const signature = await protocolKit.signHash(safeTxHash);

  await apiKit.proposeTransaction({
    safeAddress,
    safeTransactionData: safeTransaction.data,
    safeTxHash,
    senderAddress: proposer,
    senderSignature: signature.data,
    origin,
  });

  const proposal = {
    safeTxHash,
    safeAddress,
    proposer,
    nonce,
    transactionCount: transactions.length,
    origin,
    url: safeTransactionUrl(config.chainId, safeAddress, safeTxHash),
    proposedAt: new Date().toISOString(),
  };
  writeJson(safeProposalPath(config), proposal);
  return proposal;
}
