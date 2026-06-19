/**
 * Devnet smoke test: create a real duel against the deployed program, then
 * cancel it for a refund. Proves the on-chain program + IDL + wiring all work.
 *
 *   ANCHOR_PROVIDER_URL=https://api.devnet.solana.com \
 *   ANCHOR_WALLET=~/.config/solana/id.json \
 *   npx ts-node scripts/devnet-smoke.ts
 */
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { PublicKey, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { randomBytes } from "@noble/hashes/utils";

const PROGRAM_ID = new PublicKey("FpVpkZzyW9tdbXxH9ZUMSe9sghnroDNUkw7uiEgPJ89q");

(async () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const idl = require("../target/idl/duelpvp.json");
  const program = new anchor.Program(idl, PROGRAM_ID, provider);

  const creator = provider.wallet.publicKey;
  const gameId = new BN(randomBytes(8), "le");

  const [treasury] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury")],
    PROGRAM_ID
  );
  const [duel] = PublicKey.findProgramAddressSync(
    [Buffer.from("duel"), gameId.toArrayLike(Buffer, "le", 8), creator.toBuffer()],
    PROGRAM_ID
  );

  console.log("wallet  :", creator.toBase58());
  console.log("gameId  :", gameId.toString());
  console.log("duel PDA:", duel.toBase58());

  const bet = new BN(0.05 * LAMPORTS_PER_SOL);

  console.log("\n→ create_duel ...");
  const sig1 = await program.methods
    .createDuel(gameId, bet, { higherWins: {} }, null, null)
    .accounts({ creator, duel, treasury, systemProgram: SystemProgram.programId })
    .rpc();
  console.log("  ✅ tx:", sig1);
  console.log("  solscan: https://solscan.io/tx/" + sig1 + "?cluster=devnet");

  const acc: any = await program.account.duel.fetch(duel);
  console.log("  on-chain bet:", acc.betLamports.toString(), "status:", Object.keys(acc.status)[0]);

  console.log("\n→ close_duel (creator cancel, full refund) ...");
  const sig2 = await program.methods
    .closeDuel(gameId)
    .accounts({
      caller: creator,
      creator,
      opponent: creator,
      treasury,
      random: SystemProgram.programId, // never joined -> randomness is default
      duel,
    })
    .rpc();
  console.log("  ✅ tx:", sig2);
  console.log("  solscan: https://solscan.io/tx/" + sig2 + "?cluster=devnet");

  console.log("\n🎉 Smoke test passed — contract is live and working on devnet.");
})().catch((e) => {
  console.error("\n❌ FAILED:", e);
  process.exit(1);
});
