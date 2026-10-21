import { defineConfig } from "hardhat/config";

export default defineConfig({
  solidity: {
    version: "0.8.24",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200,
      },
    },
  },
  paths: {
    sources: "./src",
    tests: {
      solidity: "./test",
    },
  },
  test: {
    solidity: {
      profiles: {
        default: {
          fuzz: { runs: 256 },
        },
        ci: {
          fuzz: { runs: 2_000 },
        },
      },
    },
  },
});
