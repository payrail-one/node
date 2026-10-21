#!/usr/bin/env node

import process from "node:process";
import { Wallet } from "ethers";
import { writeExclusivePrivateFile } from "./lib.mjs";

async function main() {
  const outputPath = process.argv[2];
  if (!outputPath) throw new Error("usage: npm run wallet:testnet -- <private-key-file>");
  const wallet = Wallet.createRandom();
  const writtenPath = await writeExclusivePrivateFile(outputPath, wallet.privateKey);
  process.stdout.write(`${JSON.stringify({ address: wallet.address, privateKeyFile: writtenPath })}\n`);
}

main().catch((error) => {
  process.stderr.write(`wallet creation failed: ${error.message}\n`);
  process.exitCode = 1;
});
