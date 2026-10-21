#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { ContractFactory, JsonRpcProvider, Wallet, getBytes } from "ethers";
import {
  readSecretFile,
  requiredEnvironment,
  SEPOLIA_CHAIN_ID,
  validatedAddress,
  writePublicJson,
} from "./lib.mjs";

const packageDirectory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifactPath = path.join(
  packageDirectory,
  "artifacts",
  "src",
  "PayrailBridgeVault.sol",
  "PayrailBridgeVault.json",
);

function sha256Hex(value) {
  return createHash("sha256").update(value).digest("hex");
}

function sha256HexBytes(value) {
  return sha256Hex(getBytes(value));
}

async function main() {
  const rpcUrl = requiredEnvironment("PAYRAIL_SEPOLIA_RPC_URL");
  const owner = validatedAddress(
    "PAYRAIL_SEPOLIA_OWNER_ADDRESS",
    requiredEnvironment("PAYRAIL_SEPOLIA_OWNER_ADDRESS"),
  );
  const releaseAuthority = validatedAddress(
    "PAYRAIL_SEPOLIA_RELEASE_AUTHORITY_ADDRESS",
    requiredEnvironment("PAYRAIL_SEPOLIA_RELEASE_AUTHORITY_ADDRESS"),
  );
  if (owner === releaseAuthority) throw new Error("owner and release authority must differ");

  const privateKey = await readSecretFile("PAYRAIL_SEPOLIA_DEPLOYER_KEY_FILE");
  const provider = new JsonRpcProvider(rpcUrl, Number(SEPOLIA_CHAIN_ID), { staticNetwork: true });
  const network = await provider.getNetwork();
  if (network.chainId !== SEPOLIA_CHAIN_ID) {
    throw new Error(`RPC chain ID ${network.chainId} is not Sepolia (${SEPOLIA_CHAIN_ID})`);
  }

  const artifact = JSON.parse(await readFile(artifactPath, "utf8"));
  if (!Array.isArray(artifact.abi) || typeof artifact.bytecode !== "string") {
    throw new Error(`invalid Hardhat artifact at ${artifactPath}`);
  }
  const buildInfoPath = path.join(
    packageDirectory,
    "artifacts",
    "build-info",
    `${artifact.buildInfoId}.json`,
  );
  const buildInfo = JSON.parse(await readFile(buildInfoPath, "utf8"));
  if (buildInfo.solcVersion !== "0.8.24") {
    throw new Error(`unexpected Solidity compiler ${buildInfo.solcVersion}`);
  }
  const deployer = new Wallet(privateKey, provider);
  if ((await provider.getBalance(deployer.address)) === 0n) {
    throw new Error(`deployer ${deployer.address} has no Sepolia ETH`);
  }

  const contract = await new ContractFactory(artifact.abi, artifact.bytecode, deployer).deploy(
    owner,
    releaseAuthority,
  );
  const transaction = contract.deploymentTransaction();
  if (!transaction) throw new Error("deployment transaction was not created");
  const receipt = await transaction.wait(2);
  if (!receipt || receipt.status !== 1) throw new Error("deployment transaction failed");
  const address = await contract.getAddress();
  const code = await provider.getCode(address, receipt.blockNumber);
  if (code === "0x") throw new Error("deployed address has no contract code");

  const outputPath = process.env.PAYRAIL_SEPOLIA_DEPLOYMENT_FILE?.trim()
    || path.join(packageDirectory, "deployments", "sepolia.local.json");
  const writtenPath = await writePublicJson(outputPath, {
    schemaVersion: 1,
    network: "sepolia",
    chainId: SEPOLIA_CHAIN_ID.toString(),
    contract: "PayrailBridgeVault",
    address,
    deploymentTransaction: receipt.hash,
    deploymentBlock: receipt.blockNumber.toString(),
    deploymentBlockHash: receipt.blockHash,
    deployer: deployer.address,
    owner,
    releaseAuthority,
    confirmations: 2,
    artifact: {
      compilerVersion: buildInfo.solcVersion,
      abiSha256: sha256Hex(JSON.stringify(artifact.abi)),
      creationBytecodeSha256: sha256HexBytes(artifact.bytecode),
      runtimeBytecodeSha256: sha256HexBytes(code),
    },
  });
  process.stdout.write(`${JSON.stringify({ address, deploymentTransaction: receipt.hash, deploymentFile: writtenPath })}\n`);
}

main().catch((error) => {
  process.stderr.write(`Sepolia deployment failed: ${error.message}\n`);
  process.exitCode = 1;
});
