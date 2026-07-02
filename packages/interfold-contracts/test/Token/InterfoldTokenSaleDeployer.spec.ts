// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";

import {
  InterfoldToken__factory as InterfoldTokenFactory,
  MockBondingRegistry__factory as MockBondingRegistryFactory,
  MockCCAFactory__factory as MockCCAFactoryFactory,
  MockLBPStrategy__factory as MockLBPStrategyFactory,
  MockLiquidityLauncher__factory as MockLiquidityLauncherFactory,
  InterfoldTokenSaleDeployer__factory as SaleDeployerFactory,
} from "../../types";
import { ethers, networkHelpers } from "../fixtures";

const { time } = networkHelpers;

const DAY = 24n * 60n * 60n;
const FORTY_DAYS = 40n * DAY;
const FOUR_YEARS = 4n * 365n * DAY;
const SALE_AMOUNT = ethers.parseEther("120000000"); // 120M FOLD
const LP_RESERVE = ethers.parseEther("1000000"); // 1M FOLD

const AUCTION_PARAMETERS_TUPLE =
  "tuple(" +
  "address currency," +
  "address tokensRecipient," +
  "address fundsRecipient," +
  "uint64 startBlock," +
  "uint64 endBlock," +
  "uint64 claimBlock," +
  "uint256 tickSpacing," +
  "address validationHook," +
  "uint256 floorPrice," +
  "uint128 requiredCurrencyRaised," +
  "bytes auctionStepsData" +
  ")";
const MIGRATOR_PARAMETERS_TUPLE =
  "tuple(" +
  "address token," +
  "address currency," +
  "uint64 migrationBlock," +
  "uint128 reservedTokenAmountForLP," +
  "address recipient," +
  "address positionRecipient," +
  "tuple(uint24 fee,int24 tickSpacing,address hook) poolParameters," +
  "bytes positionDefinitions," +
  "bytes lpAllocationSchedule" +
  ")";

/** Read a deployed mock auction's shared views (version-agnostic). */
function auctionAt(
  address: string,
  runner: Parameters<typeof InterfoldTokenFactory.connect>[1],
) {
  const abi = [
    "function token() view returns (address)",
    "function totalSupply() view returns (uint128)",
    "function tokensReceived() view returns (bool)",
    "function fundsRecipient() view returns (address)",
  ];
  return new ethers.Contract(address, abi, runner);
}

interface TestConfig {
  name: string;
  chainId: number;
  saleDeployer: string;
  safe: string;
  saleAmount: string;
  ccaSalt: string;
  saleLabel: string;
  fold: {
    ccaStart: string;
    ccaEnd: string;
    bondingRegistry: string;
  };
  auction: {
    currency: string;
    tokensRecipient: string;
    fundsRecipient: string;
    startBlock: string;
    endBlock: string;
    claimBlock: string;
    tickSpacing: string;
    validationHook: string;
    floorPrice: string;
    requiredCurrencyRaised: string;
    auctionStepsData: string;
  };
}

interface TestLbpSalePlan {
  predictedFold: string;
  predictedAuction: string;
  foldInitCode: string;
  lbpSaleConfig: {
    liquidityLauncher: string;
    lbpStrategy: string;
    expectedAuction: string;
    auctionAmount: bigint;
    reservedTokenAmountForLP: bigint;
    distributionSalt: string;
    lbpConfigData: string;
    saleLabel: string;
    foldInitCodeHash: string;
  };
}

describe("InterfoldTokenSaleDeployer", function () {
  async function buildConfig(opts: {
    saleDeployer: string;
    safe: string;
    bondingRegistry: string;
  }): Promise<TestConfig> {
    const now = BigInt(await time.latest());
    const ccaStart = now + 10n * DAY;
    const ccaEnd = ccaStart + 7n * DAY;
    const currentBlock = BigInt(await ethers.provider.getBlockNumber());

    return {
      name: "test-sale",
      chainId: Number((await ethers.provider.getNetwork()).chainId),
      saleDeployer: opts.saleDeployer,
      safe: opts.safe,
      saleAmount: SALE_AMOUNT.toString(),
      ccaSalt: ethers.ZeroHash,
      saleLabel: "cca-sale",
      fold: {
        ccaStart: ccaStart.toString(),
        ccaEnd: ccaEnd.toString(),
        bondingRegistry: opts.bondingRegistry,
      },
      auction: {
        currency: "ETH",
        tokensRecipient: opts.safe,
        fundsRecipient: opts.safe,
        startBlock: (currentBlock + 100n).toString(),
        endBlock: (currentBlock + 200n).toString(),
        claimBlock: (currentBlock + 210n).toString(),
        tickSpacing: "1000000000000",
        validationHook: ethers.ZeroAddress,
        floorPrice: "1000000000000",
        requiredCurrencyRaised: "0",
        auctionStepsData: "0x",
      },
    };
  }

  async function setup() {
    const [deployer, operator, safeAdmin, stranger] = await ethers.getSigners();

    const safeAddress = await safeAdmin.getAddress();

    const bondingRegistry = await new MockBondingRegistryFactory(
      deployer,
    ).deploy();
    await bondingRegistry.waitForDeployment();
    const bondingRegistryAddress = await bondingRegistry.getAddress();

    const ccaFactory = await new MockCCAFactoryFactory(deployer).deploy(
      ethers.ZeroAddress,
    );
    await ccaFactory.waitForDeployment();
    // NB: the mock exposes a `getAddress(token,...)` method (v2 ABI) which
    // shadows ethers' BaseContract.getAddress(); read `.target` instead.
    const ccaFactoryAddress = ccaFactory.target as string;

    const launcher = await new MockLiquidityLauncherFactory(deployer).deploy();
    await launcher.waitForDeployment();
    const launcherAddress = await launcher.getAddress();

    const mockPositionManager = await stranger.getAddress();
    const mockPoolManager = await deployer.getAddress();
    const lbpStrategy = await new MockLBPStrategyFactory(deployer).deploy(
      ccaFactoryAddress,
      mockPositionManager,
      mockPoolManager,
    );
    await lbpStrategy.waitForDeployment();
    const lbpStrategyAddress = await lbpStrategy.getAddress();

    // Operator/gas payer deploys the sale factory, but the immutable
    // protocolAdmin is the Safe.
    const saleDeployerContract = await new SaleDeployerFactory(operator).deploy(
      safeAddress,
    );
    await saleDeployerContract.waitForDeployment();
    const saleDeployerAddress = await saleDeployerContract.getAddress();
    const saleDeployer = SaleDeployerFactory.connect(
      saleDeployerAddress,
      operator,
    );

    return {
      deployer,
      operator,
      safeAdmin,
      stranger,
      safeAddress,
      bondingRegistryAddress,
      ccaFactory,
      ccaFactoryAddress,
      launcher,
      launcherAddress,
      lbpStrategy,
      lbpStrategyAddress,
      mockPositionManager,
      mockPoolManager,
      saleDeployer,
      saleDeployerAddress,
    };
  }

  async function computeTestLbpSalePlan(
    ctx: Awaited<ReturnType<typeof setup>>,
    nonceOverride?: number,
  ): Promise<TestLbpSalePlan> {
    const config = await buildConfig({
      saleDeployer: ctx.saleDeployerAddress,
      safe: ctx.safeAddress,
      bondingRegistry: ctx.bondingRegistryAddress,
    });
    config.auction.fundsRecipient = ctx.lbpStrategyAddress;

    const factoryNonce =
      nonceOverride ??
      (await ethers.provider.getTransactionCount(ctx.saleDeployerAddress));
    const predictedFold = ethers.getCreateAddress({
      from: ctx.saleDeployerAddress,
      nonce: BigInt(factoryNonce),
    });
    const auctionValues = [
      ethers.ZeroAddress,
      config.auction.tokensRecipient,
      config.auction.fundsRecipient,
      BigInt(config.auction.startBlock),
      BigInt(config.auction.endBlock),
      BigInt(config.auction.claimBlock),
      BigInt(config.auction.tickSpacing),
      config.auction.validationHook,
      BigInt(config.auction.floorPrice),
      BigInt(config.auction.requiredCurrencyRaised),
      config.auction.auctionStepsData,
    ] as const;
    const ccaConfigData = ethers.AbiCoder.defaultAbiCoder().encode(
      [AUCTION_PARAMETERS_TUPLE],
      [auctionValues],
    );

    const positionDefinitions = ethers.AbiCoder.defaultAbiCoder().encode(
      [
        "tuple(int24 offsetLower,int24 offsetUpper,uint24 weight,address overridePositionRecipient)[]",
      ],
      [[]],
    );
    const lpAllocationSchedule = ethers.AbiCoder.defaultAbiCoder().encode(
      ["tuple(uint128 lowerThreshold,uint24 rate)[]"],
      [[[0n, 5_000_000n]]],
    );
    const migratorParams = [
      predictedFold,
      ethers.ZeroAddress,
      BigInt(config.auction.endBlock) + 10n,
      LP_RESERVE,
      ctx.safeAddress,
      ctx.safeAddress,
      [3000n, 60n, ethers.ZeroAddress],
      positionDefinitions,
      lpAllocationSchedule,
    ] as const;
    const launcherSalt = ethers.keccak256(
      ethers.AbiCoder.defaultAbiCoder().encode(
        ["address", "bytes32"],
        [ctx.saleDeployerAddress, config.ccaSalt],
      ),
    );
    const initializerSalt = ethers.keccak256(
      ethers.AbiCoder.defaultAbiCoder().encode(
        ["bytes32", MIGRATOR_PARAMETERS_TUPLE],
        [launcherSalt, migratorParams],
      ),
    );
    const predictedAuction = await (ctx.ccaFactory as any)[
      "getAddress(address,uint256,bytes,bytes32,address)"
    ](
      predictedFold,
      SALE_AMOUNT,
      ccaConfigData,
      initializerSalt,
      ctx.lbpStrategyAddress,
    );
    const noMoreLocks = BigInt(config.fold.ccaEnd) + FORTY_DAYS + FOUR_YEARS;
    const foldInitCode = ethers.concat([
      InterfoldTokenFactory.bytecode,
      ethers.AbiCoder.defaultAbiCoder().encode(
        ["address", "uint64", "uint64", "uint64", "address", "address"],
        [
          config.saleDeployer,
          BigInt(config.fold.ccaStart),
          BigInt(config.fold.ccaEnd),
          noMoreLocks,
          predictedAuction,
          config.fold.bondingRegistry,
        ],
      ),
    ]);
    const lbpConfigData = ethers.AbiCoder.defaultAbiCoder().encode(
      [MIGRATOR_PARAMETERS_TUPLE, "bytes"],
      [migratorParams, ccaConfigData],
    );
    return {
      predictedFold,
      predictedAuction,
      foldInitCode,
      lbpSaleConfig: {
        liquidityLauncher: ctx.launcherAddress,
        lbpStrategy: ctx.lbpStrategyAddress,
        expectedAuction: predictedAuction,
        auctionAmount: SALE_AMOUNT,
        reservedTokenAmountForLP: LP_RESERVE,
        distributionSalt: config.ccaSalt,
        lbpConfigData,
        saleLabel: ethers.encodeBytes32String(config.saleLabel),
        foldInitCodeHash: ethers.keccak256(foldInitCode),
      },
    };
  }

  it("captures the Safe as protocolAdmin (no hardcoded address)", async function () {
    const ctx = await setup();
    expect(await ctx.saleDeployer.protocolAdmin()).to.equal(ctx.safeAddress);
  });

  it("deploys FOLD + CCA through LiquidityLauncher/LBPStrategy", async function () {
    const ctx = await setup();
    const salePlan = await computeTestLbpSalePlan(ctx);
    const digest = await ctx.saleDeployer.hashLbpConfig(salePlan.lbpSaleConfig);

    await expect(
      ctx.saleDeployer
        .connect(ctx.operator)
        .deploySaleWithLiquidityLauncher(
          salePlan.lbpSaleConfig,
          salePlan.foldInitCode,
        ),
    )
      .to.emit(ctx.saleDeployer, "LbpSaleDeployed")
      .withArgs(
        digest,
        salePlan.predictedFold,
        salePlan.predictedAuction,
        ctx.launcherAddress,
        ctx.lbpStrategyAddress,
        SALE_AMOUNT,
        LP_RESERVE,
        await ctx.operator.getAddress(),
      );

    const fold = InterfoldTokenFactory.connect(
      salePlan.predictedFold,
      ctx.operator,
    );
    expect(await fold.CLAIM_SOURCE()).to.equal(salePlan.predictedAuction);
    expect(await fold.balanceOf(salePlan.predictedAuction)).to.equal(
      SALE_AMOUNT,
    );
    expect(await fold.balanceOf(ctx.lbpStrategyAddress)).to.equal(LP_RESERVE);
    expect(await fold.transferWhitelist(ctx.launcherAddress)).to.equal(true);
    expect(await fold.transferWhitelist(ctx.lbpStrategyAddress)).to.equal(true);
    expect(await fold.transferWhitelist(ctx.mockPositionManager)).to.equal(
      true,
    );

    const auction = auctionAt(salePlan.predictedAuction, ctx.operator);
    expect(await auction.token()).to.equal(salePlan.predictedFold);
    expect(await auction.totalSupply()).to.equal(SALE_AMOUNT);
    expect(await auction.fundsRecipient()).to.equal(ctx.lbpStrategyAddress);
    expect(await auction.tokensReceived()).to.equal(true);

    const initializer = await ctx.lbpStrategy.initializers(
      salePlan.predictedAuction,
    );
    expect(initializer.token).to.equal(salePlan.predictedFold);
    expect(initializer.reservedTokenAmountForLP).to.equal(LP_RESERVE);
  });

  it("hands FOLD ownership to the Safe (pending until acceptOwnership)", async function () {
    const ctx = await setup();
    const salePlan = await computeTestLbpSalePlan(ctx);
    await ctx.saleDeployer
      .connect(ctx.operator)
      .deploySaleWithLiquidityLauncher(
        salePlan.lbpSaleConfig,
        salePlan.foldInitCode,
      );

    const fold = InterfoldTokenFactory.connect(
      salePlan.predictedFold,
      ctx.operator,
    );

    expect(await fold.owner()).to.equal(ctx.saleDeployerAddress);
    expect(await fold.pendingOwner()).to.equal(ctx.safeAddress);

    await (await fold.connect(ctx.safeAdmin).acceptOwnership()).wait();

    expect(await fold.owner()).to.equal(ctx.safeAddress);
    const DEFAULT_ADMIN_ROLE = ethers.ZeroHash;
    expect(await fold.hasRole(DEFAULT_ADMIN_ROLE, ctx.safeAddress)).to.equal(
      true,
    );
    expect(
      await fold.hasRole(DEFAULT_ADMIN_ROLE, ctx.saleDeployerAddress),
    ).to.equal(false);
  });

  it("reverts when the sale amount does not match the FOLD init-code claim-source plan", async function () {
    const ctx = await setup();
    const salePlan = await computeTestLbpSalePlan(ctx);

    const tampered = {
      ...salePlan.lbpSaleConfig,
      auctionAmount: SALE_AMOUNT + 1n,
    };
    await expect(
      ctx.saleDeployer
        .connect(ctx.operator)
        .deploySaleWithLiquidityLauncher(tampered, salePlan.foldInitCode),
    ).to.be.revertedWithCustomError(ctx.saleDeployer, "AuctionMismatch");
  });

  it("reverts when FOLD init code does not match its hash", async function () {
    const ctx = await setup();
    const salePlan = await computeTestLbpSalePlan(ctx);

    const lastByte = salePlan.foldInitCode.slice(-2);
    const flipped = lastByte === "00" ? "01" : "00";
    const badInitCode = salePlan.foldInitCode.slice(0, -2) + flipped;
    await expect(
      ctx.saleDeployer
        .connect(ctx.operator)
        .deploySaleWithLiquidityLauncher(salePlan.lbpSaleConfig, badInitCode),
    ).to.be.revertedWithCustomError(ctx.saleDeployer, "FoldInitCodeMismatch");
  });

  it("prevents replaying the same approved config twice", async function () {
    const ctx = await setup();
    const salePlan = await computeTestLbpSalePlan(ctx);

    await ctx.saleDeployer
      .connect(ctx.operator)
      .deploySaleWithLiquidityLauncher(
        salePlan.lbpSaleConfig,
        salePlan.foldInitCode,
      );

    await expect(
      ctx.saleDeployer
        .connect(ctx.operator)
        .deploySaleWithLiquidityLauncher(
          salePlan.lbpSaleConfig,
          salePlan.foldInitCode,
        ),
    ).to.be.revertedWithCustomError(ctx.saleDeployer, "ConfigAlreadyUsed");
  });

  it("reverts (AuctionMismatch) when the predicted nonce is wrong", async function () {
    const ctx = await setup();
    // Build a plan assuming the wrong factory nonce -> wrong predicted FOLD ->
    // wrong predicted auction baked as claimSource -> on-chain mismatch.
    const liveNonce = await ethers.provider.getTransactionCount(
      ctx.saleDeployerAddress,
    );
    const salePlan = await computeTestLbpSalePlan(ctx, liveNonce + 5);

    await expect(
      ctx.saleDeployer
        .connect(ctx.operator)
        .deploySaleWithLiquidityLauncher(
          salePlan.lbpSaleConfig,
          salePlan.foldInitCode,
        ),
    ).to.be.revertedWithCustomError(ctx.saleDeployer, "AuctionMismatch");
  });
});
