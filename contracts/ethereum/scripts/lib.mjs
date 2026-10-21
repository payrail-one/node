import { constants, lstat, mkdir, open, readFile, realpath, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

import { getAddress, isAddress } from "ethers";

export const SEPOLIA_CHAIN_ID = 11_155_111n;
export const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";

export function requiredEnvironment(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

export function validatedAddress(name, value) {
  if (!isAddress(value)) throw new Error(`${name} must be a valid Ethereum address`);
  const address = getAddress(value);
  if (address === ZERO_ADDRESS) throw new Error(`${name} must not be the zero address`);
  return address;
}

export async function readSecretFile(environmentName) {
  const secretPath = path.resolve(requiredEnvironment(environmentName));
  const metadata = await lstat(secretPath);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${environmentName} must refer to a regular file, not a symlink`);
  }
  if ((metadata.mode & 0o077) !== 0) {
    throw new Error(`${environmentName} must not be accessible by group or other users`);
  }
  const value = (await readFile(secretPath, "utf8")).trim();
  if (!/^0x[0-9a-fA-F]{64}$/.test(value)) {
    throw new Error(`${environmentName} does not contain an Ethereum private key`);
  }
  return value;
}

export async function writeExclusivePrivateFile(outputPath, contents) {
  const absolutePath = path.resolve(outputPath);
  await mkdir(path.dirname(absolutePath), { recursive: true, mode: 0o700 });
  const directory = await realpath(path.dirname(absolutePath));
  const target = path.join(directory, path.basename(absolutePath));
  const handle = await open(
    target,
    constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | constants.O_NOFOLLOW,
    0o600,
  );
  try {
    await handle.writeFile(`${contents}\n`, { encoding: "utf8" });
    await handle.sync();
  } finally {
    await handle.close();
  }
  return target;
}

export async function writePublicJson(outputPath, value) {
  const absolutePath = path.resolve(outputPath);
  await mkdir(path.dirname(absolutePath), { recursive: true });
  await writeFile(absolutePath, `${JSON.stringify(value, null, 2)}\n`, {
    encoding: "utf8",
    mode: 0o644,
    flag: "wx",
  });
  return absolutePath;
}
