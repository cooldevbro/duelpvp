/**
 * DUELPVP tests — hardened VRF edition.
 *
 * Group 1 (no VRF) runs on any localnet: create, pause/max-bet gating,
 * private-duel rejection, creator self-cancel + refund.
 * Group 2 (VRF) needs a local ORAO instance whose fulfilment authority you
 * control — see ORAO's russian-roulette harness (rust/examples/cpi).
 *
 * Note: initialize_treasury is gated to the program's upgrade authority, which
 * on `anchor test` is the provider wallet. So admin = provider.wallet here.
 */
import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { assert } from "chai";
import { randomBytes } from "@noble/hashes/utils";
import {
  Orao,
  networkStateAccountAddress,
  randomnessAccountAddress,
} from "@orao-network/solana-vrf";
import { Duelpvp } from "../target/types/duelpvp";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);
const program = anchor.workspace.Duelpvp as Program<Duelpvp>;
const connection = provider.connection;

const BPF_LOADER_UPGRADEABLE = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111"
);

const airdrop = async (pk: PublicKey, sol = 5) => {
  const sig = await connection.requestAirdrop(pk, sol * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(sig);
};

const treasuryPda = () =>
  PublicKey.findProgramAddressSync([Buffer.from("treasury")], program.programId)[0];
const programDataPda = () =>
  PublicKey.findProgramAddressSync([program.programId.toBuffer()], BPF_LOADER_UPGRADEABLE)[0];
const duelPda = (gameId: BN, creator: PublicKey) =>
  PublicKey.findProgramAddressSync(
    [Buffer.from("duel"), gameId.toArrayLike(Buffer, "le", 8), creator.toBuffer()],
    program.programId
  )[0];
const randomGameId = () => new BN(randomBytes(8), "le");

describe("duelpvp", () => {
  const admin = (provider.wallet as anchor.Wallet).payer; // upgrade authority
  const creator = Keypair.generate();
  const opponent = Keypair.generate();

  before(async () => {
    await Promise.all([airdrop(creator.publicKey), airdrop(opponent.publicKey)]);
    await program.methods
      .initializeTreasury()
      .accounts({
        admin: admin.publicKey,
        treasury: treasuryPda(),
        programData: programDataPda(),
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  });

  // --------------------------------------------------------------- group 1
  it("creates and lets the creator cancel for a full refund", async () => {
    const gameId = randomGameId();
    const duel = duelPda(gameId, creator.publicKey);
    const before = await connection.getBalance(creator.publicKey);

    await program.methods
      .createDuel(gameId, new BN(0.1 * LAMPORTS_PER_SOL), { higherWins: {} }, null, null)
      .accounts({ creator: creator.publicKey, duel, treasury: treasuryPda(), systemProgram: SystemProgram.programId })
      .signers([creator])
      .rpc();

    await program.methods
      .closeDuel(gameId)
      .accounts({
        caller: creator.publicKey,
        creator: creator.publicKey,
        opponent: creator.publicKey,
        treasury: treasuryPda(),
        // Waiting duel never joined -> randomness is the system program (default).
        random: SystemProgram.programId,
        duel,
      })
      .signers([creator])
      .rpc();

    const after = await connection.getBalance(creator.publicKey);
    assert.isTrue(after > before - 0.01 * LAMPORTS_PER_SOL, "bet + rent refunded minus fees");
  });

  it("blocks creation when paused, allows again when unpaused", async () => {
    await program.methods.setPaused(true).accounts({ admin: admin.publicKey, treasury: treasuryPda() }).rpc();
    const gameId = randomGameId();
    const duel = duelPda(gameId, creator.publicKey);
    try {
      await program.methods
        .createDuel(gameId, new BN(0.1 * LAMPORTS_PER_SOL), { higherWins: {} }, null, null)
        .accounts({ creator: creator.publicKey, duel, treasury: treasuryPda(), systemProgram: SystemProgram.programId })
        .signers([creator])
        .rpc();
      assert.fail("paused should block creation");
    } catch (e: any) {
      assert.match(e.toString(), /Paused/);
    }
    await program.methods.setPaused(false).accounts({ admin: admin.publicKey, treasury: treasuryPda() }).rpc();
  });

  it("enforces max bet", async () => {
    await program.methods.setMaxBet(new BN(0.05 * LAMPORTS_PER_SOL)).accounts({ admin: admin.publicKey, treasury: treasuryPda() }).rpc();
    const gameId = randomGameId();
    const duel = duelPda(gameId, creator.publicKey);
    try {
      await program.methods
        .createDuel(gameId, new BN(0.1 * LAMPORTS_PER_SOL), { higherWins: {} }, null, null)
        .accounts({ creator: creator.publicKey, duel, treasury: treasuryPda(), systemProgram: SystemProgram.programId })
        .signers([creator])
        .rpc();
      assert.fail("over-max bet should be rejected");
    } catch (e: any) {
      assert.match(e.toString(), /BetTooLarge/);
    }
    await program.methods.setMaxBet(new BN(0)).accounts({ admin: admin.publicKey, treasury: treasuryPda() }).rpc();
  });

  it("rejects an uninvited opponent on a private duel", async () => {
    const invited = Keypair.generate();
    const gameId = randomGameId();
    const duel = duelPda(gameId, creator.publicKey);
    await program.methods
      .createDuel(gameId, new BN(0.1 * LAMPORTS_PER_SOL), { higherWins: {} }, invited.publicKey, null)
      .accounts({ creator: creator.publicKey, duel, treasury: treasuryPda(), systemProgram: SystemProgram.programId })
      .signers([creator])
      .rpc();

    const force = randomBytes(32);
    try {
      await joinRaw(gameId, duel, opponent, force);
      assert.fail("uninvited opponent should be rejected");
    } catch (e: any) {
      assert.match(e.toString(), /NotInvitedOpponent/);
    }
  });

  // --------------------------------------------------------------- group 2
  // Requires a local ORAO instance you can fulfil. See the ORAO CPI example.
  it.skip("full roll: create -> join -> fulfil -> settle -> winner paid", async () => {
    // 1) stand up test ORAO (InitBuilder) with your fulfilment authority
    // 2) create + join (join CPIs request_v2 with a random force)
    // 3) fulfil via FulfillBuilder + tweetnacl signature
    // 4) settle_duel (permissionless), assert winner paid + house fee collected
    // Full reference: github.com/orao-network/solana-vrf rust/examples/cpi
  });

  async function joinRaw(gameId: BN, duel: PublicKey, opp: Keypair, force: Uint8Array) {
    const vrf = new Orao(provider);
    const random = randomnessAccountAddress(Buffer.from(force));
    const ns = await vrf.getNetworkState();
    return program.methods
      .joinDuel(gameId, Array.from(force))
      .accounts({
        opponent: opp.publicKey,
        creator: creator.publicKey,
        duel,
        vrfConfig: networkStateAccountAddress(),
        vrfTreasury: ns.config.treasury,
        random,
        vrf: vrf.programId,
        systemProgram: SystemProgram.programId,
      })
      .signers([opp])
      .rpc();
  }
});
