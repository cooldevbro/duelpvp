/**
 * One-time setup after deploy: create the fee treasury PDA.
 * The wallet that runs this becomes the treasury admin (can withdraw fees).
 *
 *   anchor run init-treasury            (uses Anchor.toml provider/cluster)
 *   # or:
 *   npx ts-node scripts/init-treasury.ts
 */
import * as anchor from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";

(async () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Duelpvp as anchor.Program;
  const [treasury] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury")],
    program.programId
  );

  // Skip if it already exists.
  const existing = await provider.connection.getAccountInfo(treasury);
  if (existing) {
    console.log("Treasury already initialized:", treasury.toBase58());
    return;
  }

  const sig = await program.methods
    .initializeTreasury()
    .accounts({
      admin: provider.wallet.publicKey,
      treasury,
      systemProgram: SystemProgram.programId,
    })
    .rpc();

  console.log("Treasury:", treasury.toBase58());
  console.log("Admin:   ", provider.wallet.publicKey.toBase58());
  console.log("Tx:      ", sig);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
