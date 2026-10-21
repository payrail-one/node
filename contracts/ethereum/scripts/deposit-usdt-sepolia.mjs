#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import process from "node:process";
import { Contract, JsonRpcProvider, Wallet, getAddress } from "ethers";
import { readSecretFile, requiredEnvironment, SEPOLIA_CHAIN_ID } from "./lib.mjs";

const TOKEN_ABI = [
  "function approve(address spender,uint256 amount)",
  "function balanceOf(address account) view returns (uint256)",
];
const VAULT_ABI = [
  "function deposit(bytes32 payrailRecipient,uint256 amount)",
  "function token() view returns (address)",
  "event Deposited(bytes32 indexed payrailRecipient,address indexed token,uint256 amount,uint256 depositNonce)",
];

async function main() {
  const rpcUrl = requiredEnvironment("PAYRAIL_SEPOLIA_RPC_URL");
  const deploymentFile = requiredEnvironment("PAYRAIL_SEPOLIA_DEPLOYMENT_FILE");
  const recipient = requiredEnvironment("PAYRAIL_SEPOLIA_PAYRAIL_RECIPIENT");
  if (!/^0x[0-9a-fA-F]{64}$/.test(recipient) || /^0x0{64}$/i.test(recipient)) {
    throw new Error("PAYRAIL_SEPOLIA_PAYRAIL_RECIPIENT must be a nonzero bytes32 value");
  }
  const amount = BigInt(requiredEnvironment("PAYRAIL_SEPOLIA_DEPOSIT_UNITS"));
  if (amount <= 0n) throw new Error("PAYRAIL_SEPOLIA_DEPOSIT_UNITS must be greater than zero");

  const deployment = JSON.parse(await readFile(deploymentFile, "utf8"));
  if (BigInt(deployment.chainId) !== SEPOLIA_CHAIN_ID || !deployment.token?.address) {
    throw new Error("deployment file is not a Sepolia token-vault deployment");
  }
  const provider = new JsonRpcProvider(rpcUrl, Number(SEPOLIA_CHAIN_ID), { staticNetwork: true });
  if ((await provider.getNetwork()).chainId !== SEPOLIA_CHAIN_ID) {
    throw new Error("RPC endpoint is not Sepolia");
  }
  const signer = new Wallet(await readSecretFile("PAYRAIL_SEPOLIA_DEPOSITOR_KEY_FILE"), provider);
  const vaultAddress = getAddress(deployment.address);
  const tokenAddress = getAddress(deployment.token.address);
  const vault = new Contract(vaultAddress, VAULT_ABI, signer);
  const token = new Contract(tokenAddress, TOKEN_ABI, signer);
  if ((await vault.token()).toLowerCase() !== tokenAddress.toLowerCase()) {
    throw new Error("vault immutable token differs from deployment evidence");
  }
  const senderBefore = await token.balanceOf(signer.address);
  const vaultBefore = await token.balanceOf(vaultAddress);
  if (senderBefore < amount) throw new Error("depositor has insufficient test USDT");

  const approval = await token.approve(vaultAddress, amount);
  const approvalReceipt = await approval.wait(2);
  if (!approvalReceipt || approvalReceipt.status !== 1) throw new Error("approval failed");
  const transaction = await vault.deposit(recipient, amount);
  const receipt = await transaction.wait(2);
  if (!receipt || receipt.status !== 1) throw new Error("deposit transaction failed");
  const deposit = receipt.logs
    .map((log) => {
      try { return vault.interface.parseLog(log); } catch { return null; }
    })
    .find((entry) => entry?.name === "Deposited");
  if (
    !deposit
    || deposit.args.payrailRecipient.toLowerCase() !== recipient.toLowerCase()
    || deposit.args.token.toLowerCase() !== tokenAddress.toLowerCase()
    || deposit.args.amount !== amount
  ) {
    throw new Error("confirmed transaction did not emit the requested token deposit");
  }
  const senderAfter = await token.balanceOf(signer.address);
  const vaultAfter = await token.balanceOf(vaultAddress);
  if (senderBefore - senderAfter !== amount || vaultAfter - vaultBefore !== amount) {
    throw new Error("confirmed token balances do not match the requested exact deposit");
  }
  process.stdout.write(`${JSON.stringify({
    transaction: receipt.hash,
    blockNumber: receipt.blockNumber.toString(),
    token: tokenAddress,
    payrailRecipient: deposit.args.payrailRecipient,
    amountAtomic: amount.toString(),
    depositNonce: deposit.args.depositNonce.toString(),
  })}\n`);
}

main().catch((error) => {
  process.stderr.write(`Sepolia USDT deposit failed: ${error.message}\n`);
  process.exitCode = 1;
});
