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
const ONE_MONTH = 30n * DAY;
const LOCK_SUNSET_DELAY = FOUR_YEARS + ONE_MONTH;
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

/** Read a deployed mock auction's shared views. */
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
  saleDeployer: string;
  safe: string;
  ccaStart: bigint;
  ccaEnd: bigint;
  noMoreLocks: bigint;
  bondingRegistry: string;
  auction: {
    currency: string;
    tokensRecipient: string;
    fundsRecipient: string;
    startBlock: bigint;
    endBlock: bigint;
    claimBlock: bigint;
    tickSpacing: bigint;
    validationHook: string;
    floorPrice: bigint;
    requiredCurrencyRaised: bigint;
    auctionStepsData: string;
  };
}

interface DeployedSale {
  foldAddress: string;
  auctionAddress: string;
}

describe("InterfoldTokenSaleDeployer", function () {
  async function buildConfig(opts: {
    saleDeployer: string;
    safe: string;
    bondingRegistry: string;
    lbpStrategy: string;
  }): Promise<TestConfig> {
    const now = BigInt(await time.latest());
    const ccaStart = now + 10n * DAY;
    const ccaEnd = ccaStart + 7n * DAY;
    const currentBlock = BigInt(await ethers.provider.getBlockNumber());

    return {
      saleDeployer: opts.saleDeployer,
      safe: opts.safe,
      ccaStart,
      ccaEnd,
      noMoreLocks: ccaEnd + FORTY_DAYS + LOCK_SUNSET_DELAY,
      bondingRegistry: opts.bondingRegistry,
      auction: {
        currency: ethers.ZeroAddress,
        tokensRecipient: opts.safe,
        fundsRecipient: opts.lbpStrategy,
        startBlock: currentBlock + 100n,
        endBlock: currentBlock + 200n,
        claimBlock: currentBlock + 210n,
        tickSpacing: 1_000_000_000_000n,
        validationHook: ethers.ZeroAddress,
        floorPrice: 1_000_000_000_000n,
        requiredCurrencyRaised: 0n,
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
      launcherAddress,
      lbpStrategy,
      lbpStrategyAddress,
      mockPositionManager,
      mockPoolManager,
      saleDeployer,
      saleDeployerAddress,
    };
  }

  function auctionConfigData(config: TestConfig): string {
    const auctionValues = [
      config.auction.currency,
      config.auction.tokensRecipient,
      config.auction.fundsRecipient,
      config.auction.startBlock,
      config.auction.endBlock,
      config.auction.claimBlock,
      config.auction.tickSpacing,
      config.auction.validationHook,
      config.auction.floorPrice,
      config.auction.requiredCurrencyRaised,
      config.auction.auctionStepsData,
    ] as const;
    return ethers.AbiCoder.defaultAbiCoder().encode(
      [AUCTION_PARAMETERS_TUPLE],
      [auctionValues],
    );
  }

  async function buildLbpSaleConfig(ctx: Awaited<ReturnType<typeof setup>>) {
    const config = await buildConfig({
      saleDeployer: ctx.saleDeployerAddress,
      safe: ctx.safeAddress,
      bondingRegistry: ctx.bondingRegistryAddress,
      lbpStrategy: ctx.lbpStrategyAddress,
    });
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
    const foldInitCode = ethers.concat([
      InterfoldTokenFactory.bytecode,
      ethers.AbiCoder.defaultAbiCoder().encode(
        ["address", "uint64", "uint64", "uint64", "address"],
        [
          config.saleDeployer,
          config.ccaStart,
          config.ccaEnd,
          config.noMoreLocks,
          config.bondingRegistry,
        ],
      ),
    ]);

    return {
      config,
      foldInitCode,
      lbpSaleConfig: {
        liquidityLauncher: ctx.launcherAddress,
        lbpStrategy: ctx.lbpStrategyAddress,
        ccaStart: config.ccaStart,
        ccaEnd: config.ccaEnd,
        noMoreLocks: config.noMoreLocks,
        bondingRegistry: config.bondingRegistry,
        auctionAmount: SALE_AMOUNT,
        reservedTokenAmountForLP: LP_RESERVE,
        distributionSalt: ethers.ZeroHash,
        currency: config.auction.currency,
        migrationBlock: config.auction.endBlock + 10n,
        recipient: ctx.safeAddress,
        positionRecipient: ctx.safeAddress,
        poolParameters: {
          fee: 3000n,
          tickSpacing: 60n,
          hook: ethers.ZeroAddress,
        },
        positionDefinitions,
        lpAllocationSchedule,
        auctionConfigData: auctionConfigData(config),
        saleLabel: ethers.encodeBytes32String("cca-sale"),
        foldInitCodeHash: ethers.keccak256(foldInitCode),
      },
    };
  }

  async function deploySale(
    ctx: Awaited<ReturnType<typeof setup>>,
    plan: Awaited<ReturnType<typeof buildLbpSaleConfig>>,
  ): Promise<DeployedSale> {
    const tx = await ctx.saleDeployer
      .connect(ctx.operator)
      .deploySaleWithLiquidityLauncher(plan.lbpSaleConfig, plan.foldInitCode);
    const receipt = await tx.wait();
    if (!receipt) throw new Error("missing receipt");

    const saleEvent = receipt.logs
      .map((log) => {
        try {
          return ctx.saleDeployer.interface.parseLog(log);
        } catch {
          return null;
        }
      })
      .find((event) => event?.name === "SaleDeployed");
    const lbpEvent = receipt.logs
      .map((log) => {
        try {
          return ctx.lbpStrategy.interface.parseLog(log);
        } catch {
          return null;
        }
      })
      .find((event) => event?.name === "InitializerCreated");

    return {
      foldAddress: saleEvent?.args.fold as string,
      auctionAddress: lbpEvent?.args.initializer as string,
    };
  }

  it("captures the Safe as protocolAdmin (no hardcoded address)", async function () {
    const ctx = await setup();
    expect(await ctx.saleDeployer.protocolAdmin()).to.equal(ctx.safeAddress);
  });

  it("deploys FOLD + CCA through LiquidityLauncher/LBPStrategy without a predicted auction", async function () {
    const ctx = await setup();
    const plan = await buildLbpSaleConfig(ctx);
    const digest = await ctx.saleDeployer.hashLbpConfig(plan.lbpSaleConfig);
    const { foldAddress, auctionAddress } = await deploySale(ctx, plan);

    const fold = InterfoldTokenFactory.connect(foldAddress, ctx.operator);
    expect(await ctx.saleDeployer.usedConfigHashes(digest)).to.equal(true);
    expect(await fold.CLAIM_SOURCE()).to.equal(ethers.ZeroAddress);
    expect(await fold.balanceOf(auctionAddress)).to.equal(SALE_AMOUNT);
    expect(await fold.balanceOf(ctx.lbpStrategyAddress)).to.equal(LP_RESERVE);
    expect(await fold.transferWhitelist(ctx.launcherAddress)).to.equal(true);
    expect(await fold.transferWhitelist(ctx.lbpStrategyAddress)).to.equal(true);
    expect(await fold.transferWhitelist(ctx.mockPositionManager)).to.equal(
      true,
    );

    const auction = auctionAt(auctionAddress, ctx.operator);
    expect(await auction.token()).to.equal(foldAddress);
    expect(await auction.totalSupply()).to.equal(SALE_AMOUNT);
    expect(await auction.fundsRecipient()).to.equal(ctx.lbpStrategyAddress);
    expect(await auction.tokensReceived()).to.equal(true);

    const initializer = await ctx.lbpStrategy.initializers(auctionAddress);
    expect(initializer.token).to.equal(foldAddress);
    expect(initializer.reservedTokenAmountForLP).to.equal(LP_RESERVE);
  });

  it("hands FOLD ownership to the Safe, which sets CLAIM_SOURCE once", async function () {
    const ctx = await setup();
    const plan = await buildLbpSaleConfig(ctx);
    const { foldAddress, auctionAddress } = await deploySale(ctx, plan);
    const fold = InterfoldTokenFactory.connect(foldAddress, ctx.operator);

    expect(await fold.owner()).to.equal(ctx.saleDeployerAddress);
    expect(await fold.pendingOwner()).to.equal(ctx.safeAddress);

    await (await fold.connect(ctx.safeAdmin).acceptOwnership()).wait();
    await (
      await fold.connect(ctx.safeAdmin).setClaimSource(auctionAddress)
    ).wait();

    expect(await fold.owner()).to.equal(ctx.safeAddress);
    expect(await fold.CLAIM_SOURCE()).to.equal(auctionAddress);
    await expect(
      fold.connect(ctx.safeAdmin).setClaimSource(ctx.lbpStrategyAddress),
    ).to.be.revertedWithCustomError(fold, "ClaimSourceAlreadySet");

    const DEFAULT_ADMIN_ROLE = ethers.ZeroHash;
    expect(await fold.hasRole(DEFAULT_ADMIN_ROLE, ctx.safeAddress)).to.equal(
      true,
    );
    expect(
      await fold.hasRole(DEFAULT_ADMIN_ROLE, ctx.saleDeployerAddress),
    ).to.equal(false);
    expect(
      await fold.hasRole(DEFAULT_ADMIN_ROLE, await ctx.operator.getAddress()),
    ).to.equal(false);
  });

  it("prevents replaying the same config twice", async function () {
    const ctx = await setup();
    const plan = await buildLbpSaleConfig(ctx);

    await deploySale(ctx, plan);

    await expect(
      ctx.saleDeployer
        .connect(ctx.operator)
        .deploySaleWithLiquidityLauncher(plan.lbpSaleConfig, plan.foldInitCode),
    ).to.be.revertedWithCustomError(ctx.saleDeployer, "ConfigAlreadyUsed");
  });

  it("reverts when the sale amount exceeds uint128", async function () {
    const ctx = await setup();
    const plan = await buildLbpSaleConfig(ctx);
    const tampered = {
      ...plan.lbpSaleConfig,
      auctionAmount: (1n << 128n) + 1n,
    };

    await expect(
      ctx.saleDeployer
        .connect(ctx.operator)
        .deploySaleWithLiquidityLauncher(tampered, plan.foldInitCode),
    ).to.be.revertedWithCustomError(ctx.saleDeployer, "SaleAmountTooLarge");
  });
});
