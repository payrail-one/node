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
const TEST_USDT_SUPPLY = 1_000_000n * 1_000_000n;

function sha256Hex(value) {
  return createHash("sha256").update(value).digest("hex");
}

function sha256HexBytes(value) {
  return sha256Hex(getBytes(value));
}

async function loadArtifact(relativePath, contractName) {
  const artifactPath = path.join(packageDirectory, "artifacts", relativePath, `${contractName}.json`);
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
  return { artifact, compilerVersion: buildInfo.solcVersion };
}

async function deploy(factory, constructorArguments, confirmations) {
  const contract = await factory.deploy(...constructorArguments);
  const transaction = contract.deploymentTransaction();
  if (!transaction) throw new Error("deployment transaction was not created");
  const receipt = await transaction.wait(confirmations);
  if (!receipt || receipt.status !== 1) throw new Error("deployment transaction failed");
  return { contract, receipt, address: await contract.getAddress() };
}

async function artifactEvidence(provider, loaded, address, blockNumber) {
  const code = await provider.getCode(address, blockNumber);
  if (code === "0x") throw new Error(`deployed address ${address} has no contract code`);
  return {
    compilerVersion: loaded.compilerVersion,
    abiSha256: sha256Hex(JSON.stringify(loaded.artifact.abi)),
    creationBytecodeSha256: sha256HexBytes(loaded.artifact.bytecode),
    runtimeBytecodeSha256: sha256HexBytes(code),
  };
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

  const provider = new JsonRpcProvider(rpcUrl, Number(SEPOLIA_CHAIN_ID), { staticNetwork: true });
  const network = await provider.getNetwork();
  if (network.chainId !== SEPOLIA_CHAIN_ID) {
    throw new Error(`RPC chain ID ${network.chainId} is not Sepolia (${SEPOLIA_CHAIN_ID})`);
  }
  const signer = new Wallet(await readSecretFile("PAYRAIL_SEPOLIA_DEPLOYER_KEY_FILE"), provider);
  if ((await provider.getBalance(signer.address)) === 0n) {
    throw new Error(`deployer ${signer.address} has no Sepolia ETH`);
  }

  const tokenArtifact = await loadArtifact(
    path.join("src", "testnet", "PayrailTestUsdt.sol"),
    "PayrailTestUsdt",
  );
  const vaultArtifact = await loadArtifact(
    path.join("src", "PayrailTokenVault.sol"),
    "PayrailTokenVault",
  );
  const tokenDeployment = await deploy(
    new ContractFactory(tokenArtifact.artifact.abi, tokenArtifact.artifact.bytecode, signer),
    [signer.address, TEST_USDT_SUPPLY],
    2,
  );
  const vaultDeployment = await deploy(
    new ContractFactory(vaultArtifact.artifact.abi, vaultArtifact.artifact.bytecode, signer),
    [tokenDeployment.address, owner, releaseAuthority],
    2,
  );

  const vault = vaultDeployment.contract;
  const token = tokenDeployment.contract;
  if (
    (await vault.token()).toLowerCase() !== tokenDeployment.address.toLowerCase()
    || (await vault.owner()).toLowerCase() !== owner.toLowerCase()
    || (await vault.releaseAuthority()).toLowerCase() !== releaseAuthority.toLowerCase()
    || await vault.paused()
  ) {
    throw new Error("deployed vault configuration does not match requested roles and token");
  }
  if (
    await token.symbol() !== "tUSDT"
    || await token.decimals() !== 6n
    || await token.totalSupply() !== TEST_USDT_SUPPLY
    || await token.balanceOf(signer.address) !== TEST_USDT_SUPPLY
  ) {
    throw new Error("deployed test token metadata or allocation is invalid");
  }

  const deploymentBlock = await provider.getBlock(vaultDeployment.receipt.blockNumber);
  if (!deploymentBlock || deploymentBlock.hash !== vaultDeployment.receipt.blockHash) {
    throw new Error("could not bind deployment evidence to the canonical block");
  }
  const outputPath = process.env.PAYRAIL_SEPOLIA_DEPLOYMENT_FILE?.trim()
    || path.join(packageDirectory, "deployments", "sepolia-usdt.local.json");
  const writtenPath = await writePublicJson(outputPath, {
    schemaVersion: 1,
    network: "sepolia",
    chainId: SEPOLIA_CHAIN_ID.toString(),
    assetPurpose: "testnet-usdt-integration-only",
    productionOrigin: {
      network: "ethereum-mainnet",
      chainId: "1",
      token: "0xdAC17F958D2ee523a2206206994597C13D831ec7",
    },
    contract: "PayrailTokenVault",
    address: vaultDeployment.address,
    deploymentTransaction: vaultDeployment.receipt.hash,
    deploymentBlock: vaultDeployment.receipt.blockNumber.toString(),
    deploymentBlockHash: vaultDeployment.receipt.blockHash,
    deploymentBlockParentHash: deploymentBlock.parentHash,
    deployer: signer.address,
    owner,
    releaseAuthority,
    confirmations: 2,
    token: {
      contract: "PayrailTestUsdt",
      address: tokenDeployment.address,
      symbol: "tUSDT",
      decimals: 6,
      totalSupplyAtomic: TEST_USDT_SUPPLY.toString(),
      deploymentTransaction: tokenDeployment.receipt.hash,
      deploymentBlock: tokenDeployment.receipt.blockNumber.toString(),
      deploymentBlockHash: tokenDeployment.receipt.blockHash,
      artifact: await artifactEvidence(
        provider,
        tokenArtifact,
        tokenDeployment.address,
        tokenDeployment.receipt.blockNumber,
      ),
    },
    artifact: await artifactEvidence(
      provider,
      vaultArtifact,
      vaultDeployment.address,
      vaultDeployment.receipt.blockNumber,
    ),
  });
  process.stdout.write(`${JSON.stringify({
    vault: vaultDeployment.address,
    token: tokenDeployment.address,
    deploymentTransaction: vaultDeployment.receipt.hash,
    deploymentFile: writtenPath,
  })}\n`);
}

main().catch((error) => {
  process.stderr.write(`Sepolia USDT deployment failed: ${error.message}\n`);
  process.exitCode = 1;
});
