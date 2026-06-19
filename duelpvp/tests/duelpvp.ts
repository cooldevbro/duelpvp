/**
 * DUELPVP tests — VRF edition.
 *
 * Two groups:
 *   1. Pure-escrow paths (no VRF): create, private-duel intruder rejection,
 *      creator self-cancel + refund. These run on any localnet.
 *   2. Full roll paths (VRF): create -> join (requests randomness) -> fulfil
 *      via a TEST ORAO instance -> settle -> assert payout.
 *
 * For (2) you must run a LOCAL ORAO you control so you can fulfil randomness
 * deterministically. Follow ORAO's russian-roulette example harness:
 *   https://github.com/orao-network/solana-vrf/tree/master/rust/examples/cpi
 * It uses InitBuilder (to set a test fulfilment authority) and FulfillBuilder
 * + tweetnacl to sign the randomness. The clone in Anchor.toml gives you the
 * program binary; InitBuilder gives you an instance whose authority you hold.
 */
import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { assert } from "chai";
import { sha256 } from "@noble/hashes/sha256";
import {
  Orao,
  networkStateAccountAddress,
  randomnessAccountAddress,
  InitBuilder,
  FulfillBuilder,
} from "@orao-network/solana-vrf";
import nacl from "tweetnacl";
import { Duelpvp } from "../target/types/duelpvp";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);
const program = anchor.workspace.Duelpvp as Program<Duelpvp>;
const connection = provider.connection;

const airdrop = async (kp: Keypair, sol = 5) => {
  const sig = await connection.requestAirdrop(kp.publicKey, sol * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(sig);
};

const treasuryPda = () =>
  PublicKey.findProgramAddressSync([Buffer.from("treasury")], program.programId)[0];
const duelPda = (gameId: BN, creator: PublicKey) =>
  PublicKey.findProgramAddressSync(
    [Buffer.from("duel"), gameId.toArrayLike(Buffer, "le", 8), creator.toBuffer()],
    program.programId
  )[0];
const randomGameId = () =>
  new BN(Keypair.generate().publicKey.toBuffer().subarray(0, 8), "le");

const computeForce = (duel: PublicKey, opponent: PublicKey, createdAt: BN) => {
  const b = new Uint8Array(72);
  b.set(duel.toBytes(), 0);
  b.set(opponent.toBytes(), 32);
  b.set(createdAt.toArrayLike(Buffer, "le", 8), 64);
  return sha256(b);
};

describe("duelpvp", () => {
  const admin = Keypair.generate();
  const creator = Keypair.generate();
  const opponent = Keypair.generate();

  before(async () => {
    await Promise.all([airdrop(admin), airdrop(creator), airdrop(opponent)]);
    await program.methods
      .initializeTreasury()
      .accounts({ admin: admin.publicKey, treasury: treasuryPda(), systemProgram: SystemProgram.programId })
      .signers([admin])
      .rpc();
  });

  // ----------------------------------------------------------------- group 1
  it("creates and lets the creator cancel for a full refund", async () => {
    const gameId = randomGameId();
    const duel = duelPda(gameId, creator.publicKey);
    const before = await connection.getBalance(creator.publicKey);

    await program.methods
      .createDuel(gameId, new BN(0.1 * LAMPORTS_PER_SOL), { higherWins: {} }, null)
      .accounts({ creator: creator.publicKey, duel, systemProgram: SystemProgram.programId })
      .signers([creator])
      .rpc();

    await program.methods
      .closeDuel(gameId)
      .accounts({ caller: creator.publicKey, creator: creator.publicKey, opponent: creator.publicKey, duel })
      .signers([creator])
      .rpc();

    const after = await connection.getBalance(creator.publicKey);
    assert.isTrue(after > before - 0.01 * LAMPORTS_PER_SOL, "bet + rent refunded (minus fees)");
  });

  it("rejects an uninvited opponent on a private duel", async () => {
    const invited = Keypair.generate();
    const gameId = randomGameId();
    const duel = duelPda(gameId, creator.publicKey);
    await program.methods
      .createDuel(gameId, new BN(0.1 * LAMPORTS_PER_SOL), { higherWins: {} }, invited.publicKey)
      .accounts({ creator: creator.publicKey, duel, systemProgram: SystemProgram.programId })
      .signers([creator])
      .rpc();

    const duelAcc = await program.account.duel.fetch(duel);
    const force = computeForce(duel, opponent.publicKey, duelAcc.createdAt as BN);
    try {
      await joinRaw(gameId, duel, opponent, force);
      assert.fail("uninvited opponent should be rejected");
    } catch (e: any) {
      assert.match(e.toString(), /NotInvitedOpponent/);
    }
  });

  // ----------------------------------------------------------------- group 2
  // Requires a local ORAO instance whose fulfilment authority you control.
  // See the ORAO russian-roulette harness; sketch shown here.
  it.skip("full roll: create -> join -> fulfil -> settle -> winner paid", async () => {
    const vrf = new Orao(provider);
    const fulfilmentAuthority = Keypair.generate();

    // 1) stand up a test ORAO instance with our authority (one-time)
    await new InitBuilder(
      vrf,
      Keypair.generate().publicKey, // treasury
      [fulfilmentAuthority.publicKey],
      new BN(2) // fee (lamports) — small for tests
    ).rpc();

    // 2) create + join (join CPIs RequestV2)
    const gameId = randomGameId();
    const duel = duelPda(gameId, creator.publicKey);
    await program.methods
      .createDuel(gameId, new BN(0.1 * LAMPORTS_PER_SOL), { higherWins: {} }, null)
      .accounts({ creator: creator.publicKey, duel, systemProgram: SystemProgram.programId })
      .signers([creator])
      .rpc();

    const duelAcc = await program.account.duel.fetch(duel);
    const force = computeForce(duel, opponent.publicKey, duelAcc.createdAt as BN);
    await joinRaw(gameId, duel, opponent, force);

    // 3) fulfil the randomness with our test authority (deterministic)
    const random = randomnessAccountAddress(Buffer.from(force));
    const signature = nacl.sign.detached(Buffer.from(force), fulfilmentAuthority.secretKey);
    await new FulfillBuilder(vrf, Buffer.from(force))
      .build(fulfilmentAuthority.publicKey, signature)
      .rpc();

    // 4) settle (permissionless) and assert the winner got the pot - fee
    const treasuryBefore = await connection.getBalance(treasuryPda());
    await program.methods
      .settleDuel(gameId)
      .accounts({
        caller: opponent.publicKey,
        creator: creator.publicKey,
        opponent: opponent.publicKey,
        treasury: treasuryPda(),
        duel,
        random,
      })
      .signers([opponent])
      .rpc();

    const settled = await program.account.duel.fetch(duel);
    assert.deepEqual(Object.keys(settled.status)[0], "settled");
    const treasuryAfter = await connection.getBalance(treasuryPda());
    assert.isTrue(treasuryAfter > treasuryBefore, "house fee collected");
  });

  // helper: build the join_duel call with ORAO accounts
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
