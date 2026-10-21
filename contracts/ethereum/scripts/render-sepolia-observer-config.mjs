#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import process from "node:process";
import { JsonRpcProvider, getAddress } from "ethers";
import {
  requiredEnvironment,
  SEPOLIA_CHAIN_ID,
  writeExclusivePrivateFile,
} from "./lib.mjs";

function identifier(domain, value) {
  return `0x${createHash("sha256").update(domain).update(value).digest("hex")}`;
}

async function main() {
  const deploymentFile = requiredEnvironment("PAYRAIL_SEPOLIA_DEPLOYMENT_FILE");
  const outputFile = requiredEnvironment("PAYRAIL_SEPOLIA_OBSERVER_ENV_FILE");
  const rpcUrls = requiredEnvironment("PAYRAIL_SEPOLIA_RPC_URLS")
    .split(",")
    .map((value) => value.trim());
  if (rpcUrls.length < 2 || rpcUrls.length > 8 || rpcUrls.some((url) => !url.startsWith("https://"))) {
    throw new Error("PAYRAIL_SEPOLIA_RPC_URLS requires 2..=8 comma-separated HTTPS endpoints");
  }
  if (new Set(rpcUrls).size !== rpcUrls.length) {
    throw new Error("PAYRAIL_SEPOLIA_RPC_URLS contains a duplicate endpoint");
  }

  const deployment = JSON.parse(await readFile(deploymentFile, "utf8"));
  if (BigInt(deployment.chainId) !== SEPOLIA_CHAIN_ID) {
    throw new Error("deployment file is not for Sepolia");
  }
  const bridgeAddress = getAddress(deployment.address).toLowerCase();
  const deploymentBlock = Number(deployment.deploymentBlock);
  if (!Number.isSafeInteger(deploymentBlock) || deploymentBlock <= 0) {
    throw new Error("deployment file has an invalid deployment block");
  }

  const observations = await Promise.all(rpcUrls.map(async (rpcUrl) => {
    const provider = new JsonRpcProvider(rpcUrl, Number(SEPOLIA_CHAIN_ID), { staticNetwork: true });
    const network = await provider.getNetwork();
    if (network.chainId !== SEPOLIA_CHAIN_ID) throw new Error("an RPC endpoint is not Sepolia");
    const block = await provider.getBlock(deploymentBlock);
    if (!block) throw new Error("an RPC endpoint does not have the deployment block");
    return { hash: block.hash, parentHash: block.parentHash };
  }));
  const expectedHash = deployment.deploymentBlockHash.toLowerCase();
  if (observations.some(({ hash }) => hash?.toLowerCase() !== expectedHash)) {
    throw new Error("RPC providers disagree with the recorded deployment block hash");
  }
  const parentHash = observations[0].parentHash.toLowerCase();
  if (observations.some((observation) => observation.parentHash.toLowerCase() !== parentHash)) {
    throw new Error("RPC providers disagree on the deployment parent block");
  }

  const providerEntries = rpcUrls.map((url) =>
    `${identifier("payrail:ethereum-rpc-provider:v1:", url)}=${url}`,
  );
  const tokenAddress = deployment.token?.address
    ? getAddress(deployment.token.address).toLowerCase()
    : null;
  const payrailAssetId = tokenAddress
    ? identifier("payrail:external-asset:v1:", `${SEPOLIA_CHAIN_ID}:erc20:${tokenAddress}`)
    : identifier("payrail:external-asset:v1:", `${SEPOLIA_CHAIN_ID}:native:${bridgeAddress}`);
  const assetBinding = tokenAddress
    ? `${tokenAddress}=${payrailAssetId}`
    : `native=${payrailAssetId}`;
  const defaultDatabase = tokenAddress
    ? "/var/lib/payrail-ethereum-usdt-observer/journal"
    : "/var/lib/payrail-ethereum-observer/journal";
  const database = process.env.PAYRAIL_SEPOLIA_OBSERVER_DB?.trim() || defaultDatabase;
  const environment = [
    `PAYRAIL_ETH_CHAIN_ID=${SEPOLIA_CHAIN_ID}`,
    `PAYRAIL_ETH_BRIDGE_ADDRESS=${bridgeAddress}`,
    `PAYRAIL_ETH_FIRST_BLOCK=${deploymentBlock}`,
    `PAYRAIL_ETH_FIRST_PARENT_HASH=${parentHash}`,
    `PAYRAIL_ETH_RPC_PROVIDERS=${providerEntries.join(",")}`,
    `PAYRAIL_ETH_QUORUM=${rpcUrls.length}`,
    `PAYRAIL_ETH_ASSETS=${assetBinding}`,
    `PAYRAIL_ETH_OBSERVER_DB=${database}`,
    "PAYRAIL_ETH_OBSERVER_MAP_SIZE=67108864",
    "PAYRAIL_ETH_POLL_MILLIS=3000",
  ].join("\n");
  const writtenPath = await writeExclusivePrivateFile(outputFile, environment);
  process.stdout.write(`${JSON.stringify({
    bridgeAddress,
    deploymentBlock: deploymentBlock.toString(),
    firstParentHash: parentHash,
    tokenAddress,
    payrailAssetId,
    providerCount: rpcUrls.length,
    observerEnvironmentFile: writtenPath,
  })}\n`);
}

main().catch((error) => {
  process.stderr.write(`observer configuration failed: ${error.message}\n`);
  process.exitCode = 1;
});
