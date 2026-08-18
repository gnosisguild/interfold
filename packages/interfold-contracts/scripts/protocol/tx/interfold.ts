// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { ACTIVE_BFV_PARAM_SET } from "../../utils";
import { BFV_PARAMS } from "../constants";
import { safeTx } from "../safe";
import type {
  ProtocolConfigFile,
  ProtocolContracts,
  ProtocolInterfaces,
  SafeTransaction,
} from "../types";
import { encodeBfvParams, optionalAddress, pricingConfig } from "../values";

export function appendInterfoldTxs(
  txs: SafeTransaction[],
  config: ProtocolConfigFile,
  c: ProtocolContracts,
  i: ProtocolInterfaces,
) {
  txs.push(
    safeTx(
      c.interfold,
      i.interfold.encodeFunctionData("setE3RefundManager", [c.e3RefundManager]),
    ),
    safeTx(
      c.interfold,
      i.interfold.encodeFunctionData("setSlashingManager", [c.slashingManager]),
    ),
  );

  if (BigInt(config.interfold.markFailedGracePeriod) > 0n) {
    txs.push(
      safeTx(
        c.interfold,
        i.interfold.encodeFunctionData("setMarkFailedGracePeriod", [
          BigInt(config.interfold.markFailedGracePeriod),
        ]),
      ),
    );
  }
  if (config.interfold.allowFeeToken) {
    txs.push(
      safeTx(
        c.interfold,
        i.interfold.encodeFunctionData("setFeeTokenAllowed", [
          config.feeToken,
          true,
        ]),
      ),
    );
  }
  appendCommitteeAndPricingTxs(txs, config, c, i);
}

function appendCommitteeAndPricingTxs(
  txs: SafeTransaction[],
  config: ProtocolConfigFile,
  c: ProtocolContracts,
  i: ProtocolInterfaces,
) {
  for (const threshold of config.interfold.committeeThresholds) {
    txs.push(
      safeTx(
        c.interfold,
        i.interfold.encodeFunctionData("setCommitteeThresholds", [
          BigInt(threshold.size),
          [BigInt(threshold.quorum), BigInt(threshold.total)],
        ]),
      ),
    );
  }
  if (config.interfold.registerActiveBfvParamSet) {
    const activeParams =
      ACTIVE_BFV_PARAM_SET === 0
        ? BFV_PARAMS.insecure512
        : BFV_PARAMS.secure8192;
    txs.push(
      safeTx(
        c.interfold,
        i.interfold.encodeFunctionData("setParamSet", [
          ACTIVE_BFV_PARAM_SET,
          encodeBfvParams(activeParams),
        ]),
      ),
    );
  }
  txs.push(
    safeTx(
      c.interfold,
      i.interfold.encodeFunctionData("setFeeAssetConfig", [
        {
          token: config.feeToken,
          expectedDecimals: config.feeTokenDecimals,
          pricing: pricingConfig(config.interfold.pricing),
        },
      ]),
    ),
  );
  appendVerifierTxs(txs, config, c, i);
}

function appendVerifierTxs(
  txs: SafeTransaction[],
  config: ProtocolConfigFile,
  c: ProtocolContracts,
  i: ProtocolInterfaces,
) {
  const decryption =
    c.decryptionVerifier ??
    optionalAddress(config.verifiers?.decryptionVerifier, "decryptionVerifier");
  if (decryption) {
    txs.push(
      safeTx(
        c.interfold,
        i.interfold.encodeFunctionData("setDecryptionVerifier", [
          ethersLib.id("fhe.rs:BFV"),
          decryption,
        ]),
      ),
    );
  }
  const pk =
    c.pkVerifier ?? optionalAddress(config.verifiers?.pkVerifier, "pkVerifier");
  if (pk) {
    txs.push(
      safeTx(
        c.interfold,
        i.interfold.encodeFunctionData("setPkVerifier", [
          ethersLib.id("fhe.rs:BFV"),
          pk,
        ]),
      ),
    );
  }
  const ciphertext = optionalAddress(
    config.ciphertextVerifier,
    "ciphertextVerifier",
  );
  if (ciphertext) {
    txs.push(
      safeTx(
        c.interfold,
        i.interfold.encodeFunctionData("setCiphertextVerifier", [
          ethersLib.id("fhe.rs:BFV"),
          ciphertext,
        ]),
      ),
    );
  }
  if (config.bindInitialE3Program) {
    const program = new ethersLib.Interface([
      "function bindInterfold(address interfold)",
    ]);
    txs.push(
      safeTx(
        config.e3Programs[0],
        program.encodeFunctionData("bindInterfold", [c.interfold]),
      ),
    );
  }
}
