// SPDX-License-Identifier: LGPL-3.0-only
import { connect } from "./cli";
import {
  configPath,
  jsonFileHash,
  planPath,
  readJson,
  writeJson,
} from "./files";
import { buildSalePlan, printPlan } from "./plan";
import type { SalePlan } from "./types";
import { loadConfig } from "./values";

function hasRuntimeFields(raw: unknown): boolean {
  const value = raw as {
    saleDeployer?: string;
    fold?: { bondingRegistry?: string };
  };
  return Boolean(value.saleDeployer || value.fold?.bondingRegistry);
}

export async function actionPlan(): Promise<SalePlan> {
  const { ethers } = await connect();
  const configFile = configPath();
  const rawConfig = readJson<unknown>(configFile);
  const config = loadConfig(configFile);
  const plan = await buildSalePlan(ethers, config);
  plan.sourceConfigHash = jsonFileHash(configFile);
  if (hasRuntimeFields(rawConfig)) writeJson(configFile, config);
  const planFile = planPath(config);
  writeJson(planFile, plan);
  printPlan(plan, planFile);
  return plan;
}
