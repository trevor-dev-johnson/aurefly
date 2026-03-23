const fs = require("fs");
const { Keypair } = require("@solana/web3.js");

const secret = JSON.parse(
  fs.readFileSync("C:\\Users\\Trevor\\.config\\solana\\id.json", "utf-8"),
);

const keypair = Keypair.fromSecretKey(Uint8Array.from(secret));

console.log("Public Address:", keypair.publicKey.toBase58());
