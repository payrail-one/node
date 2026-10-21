#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import process from "node:process";
import { Contract, JsonRpcProvider, Wallet, getAddress } from "ethers";
import { readSecretFile, requiredEnvironment, SEPOLIA_CHAIN_ID } from "./lib.mjs";

const DEPOSIT_ABI = [
  "function depositNative(bytes32 payrailRecipient) payable",
  "event Deposited(bytes32 indexed payrailRecipient, address indexed token, uint256 amount, uint256 depositNonce)",
];

async function main() {
  const rpcUrl = requiredEnvironment("PAYRAIL_SEPOLIA_RPC_URL");
  const deploymentFile = requiredEnvironment("PAYRAIL_SEPOLIA_DEPLOYMENT_FILE");
  const recipient = requiredEnvironment("PAYRAIL_SEPOLIA_PAYRAIL_RECIPIENT");
  if (!/^0x[0-9a-fA-F]{64}$/.test(recipient) || /^0x0{64}$/i.test(recipient)) {
    throw new Error("PAYRAIL_SEPOLIA_PAYRAIL_RECIPIENT must be a nonzero bytes32 value");
  }
  const amount = BigInt(requiredEnvironment("PAYRAIL_SEPOLIA_DEPOSIT_WEI"));
  if (amount <= 0n) throw new Error("PAYRAIL_SEPOLIA_DEPOSIT_WEI must be greater than zero");

  const deployment = JSON.parse(await readFile(deploymentFile, "utf8"));
  if (BigInt(deployment.chainId) !== SEPOLIA_CHAIN_ID) {
    throw new Error("deployment file is not for Sepolia");
  }
  const provider = new JsonRpcProvider(rpcUrl, Number(SEPOLIA_CHAIN_ID), { staticNetwork: true });
  const network = await provider.getNetwork();
  if (network.chainId !== SEPOLIA_CHAIN_ID) {
    throw new Error(`RPC chain ID ${network.chainId} is not Sepolia (${SEPOLIA_CHAIN_ID})`);
  }
  const signer = new Wallet(await readSecretFile("PAYRAIL_SEPOLIA_DEPOSITOR_KEY_FILE"), provider);
  const vault = new Contract(getAddress(deployment.address), DEPOSIT_ABI, signer);
  const transaction = await vault.depositNative(recipient, { value: amount });
  const receipt = await transaction.wait(2);
  if (!receipt || receipt.status !== 1) throw new Error("deposit transaction failed");
  const deposit = receipt.logs
    .map((log) => {
      try { return vault.interface.parseLog(log); } catch { return null; }
    })
    .find((entry) => entry?.name === "Deposited");
  if (!deposit) throw new Error("confirmed transaction did not emit Deposited");
  if (deposit.args.payrailRecipient.toLowerCase() !== recipient.toLowerCase() || deposit.args.amount !== amount) {
    throw new Error("Deposited event does not match the requested deposit");
  }
  process.stdout.write(`${JSON.stringify({
    transaction: receipt.hash,
    blockNumber: receipt.blockNumber.toString(),
    payrailRecipient: deposit.args.payrailRecipient,
    amountWei: deposit.args.amount.toString(),
    depositNonce: deposit.args.depositNonce.toString(),
  })}\n`);
}

main().catch((error) => {
  process.stderr.write(`Sepolia deposit failed: ${error.message}\n`);
  process.exitCode = 1;
});
