/**
 * One-time setup after deploy: create the fee treasury PDA.
 * MUST be run by the program's upgrade authority (i.e. the wallet that deployed
 * the program). That wallet becomes the treasury admin.
 *
 *   npx ts-node scripts/init-treasury.ts
 */
import * as anchor from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";

const BPF_LOADER_UPGRADEABLE = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111"
);

(async () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Duelpvp as anchor.Program;

  const [treasury] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury")],
    program.programId
  );
  const [programData] = PublicKey.findProgramAddressSync(
    [program.programId.toBuffer()],
    BPF_LOADER_UPGRADEABLE
  );

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
      programData,
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
