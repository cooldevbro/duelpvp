/**
 * DUELPVP client — VRF (ORAO) edition.
 *
 * Flow: createDuel -> joinDuel (deposits + requests randomness) -> settleDuel
 * (consumes randomness, pays winner). The front end animates the settled
 * on-chain result; it never decides the outcome.
 */
import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram } from "@solana/web3.js";
import { sha256 } from "@noble/hashes/sha256";
import {
  Orao,
  networkStateAccountAddress,
  randomnessAccountAddress,
} from "@orao-network/solana-vrf";
import type { Duelpvp } from "../target/types/duelpvp";

export type DuelProgram = Program<Duelpvp>;

export const TREASURY_SEED = "treasury";
export const DUEL_SEED = "duel";

// ---------------------------------------------------------------------------
// PDA derivation
// ---------------------------------------------------------------------------

export function deriveTreasuryPda(programId: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(TREASURY_SEED)],
    programId
  );
}

export function deriveDuelPda(
  gameId: BN,
  creator: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(DUEL_SEED), gameId.toArrayLike(Buffer, "le", 8), creator.toBuffer()],
    programId
  );
}

/** Random u64 game id (off-chain; PDA existence is the collision guard). */
export function generateGameId(): BN {
  const buf = anchor.web3.Keypair.generate().publicKey.toBuffer().subarray(0, 8);
  return new BN(buf, "le");
}

/**
 * VRF force = sha256(duelPda || opponent || createdAt_le_i64).
 * Must match the on-chain derivation exactly. `createdAt` is read from the
 * fetched duel account.
 */
export function computeForce(
  duelPda: PublicKey,
  opponent: PublicKey,
  createdAt: BN
): Uint8Array {
  const buf = new Uint8Array(32 + 32 + 8);
  buf.set(duelPda.toBytes(), 0);
  buf.set(opponent.toBytes(), 32);
  buf.set(createdAt.toArrayLike(Buffer, "le", 8), 64);
  return sha256(buf);
}

/** Derive the two d6 per player from the 64-byte VRF output (for UI preview/verification). */
export function deriveDice(randomness: Uint8Array): {
  creator: [number, number];
  opponent: [number, number];
} {
  return {
    creator: [(randomness[0] % 6) + 1, (randomness[1] % 6) + 1],
    opponent: [(randomness[2] % 6) + 1, (randomness[3] % 6) + 1],
  };
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

export type WinCondition = { higherWins: {} } | { lowerWins: {} };
export const HIGHER_WINS: WinCondition = { higherWins: {} };
export const LOWER_WINS: WinCondition = { lowerWins: {} };

export async function initializeTreasury(program: DuelProgram, admin: PublicKey) {
  const [treasury] = deriveTreasuryPda(program.programId);
  return program.methods
    .initializeTreasury()
    .accounts({ admin, treasury, systemProgram: SystemProgram.programId })
    .rpc();
}

export async function createDuel(
  program: DuelProgram,
  creator: PublicKey,
  gameId: BN,
  betLamports: BN,
  winCondition: WinCondition,
  requiredOpponent: PublicKey | null
) {
  const [duel] = deriveDuelPda(gameId, creator, program.programId);
  await program.methods
    .createDuel(gameId, betLamports, winCondition, requiredOpponent)
    .accounts({ creator, duel, systemProgram: SystemProgram.programId })
    .rpc();
  return { duel, gameId };
}

/**
 * Opponent joins: deposits the matching bet and fires the VRF request in one
 * transaction. Returns the randomness account so the UI can await fulfillment.
 */
export async function joinDuel(
  program: DuelProgram,
  connection: Connection,
  opponent: PublicKey,
  creator: PublicKey,
  gameId: BN
) {
  const vrf = new Orao(program.provider as anchor.AnchorProvider);
  const [duel] = deriveDuelPda(gameId, creator, program.programId);

  const duelAcc = await program.account.duel.fetch(duel);
  const force = computeForce(duel, opponent, duelAcc.createdAt as BN);

  const random = randomnessAccountAddress(force);
  const vrfConfig = networkStateAccountAddress();
  const ns = await vrf.getNetworkState();
  const vrfTreasury = ns.config.treasury;

  await program.methods
    .joinDuel(gameId, Array.from(force))
    .accounts({
      opponent,
      creator,
      duel,
      vrfConfig,
      vrfTreasury,
      random,
      vrf: vrf.programId,
      systemProgram: SystemProgram.programId,
    })
    .rpc();

  return { duel, random, force };
}

/** Wait until ORAO has fulfilled the request (the "rolling" window, ~1-3s). */
export async function waitForRandomness(
  program: DuelProgram,
  random: PublicKey
): Promise<Uint8Array> {
  const vrf = new Orao(program.provider as anchor.AnchorProvider);
  const r = await vrf.waitFulfilled(random.toBuffer());
  return Uint8Array.from(r.randomness);
}

/** Permissionless settle. Consumes randomness and pays the winner. */
export async function settleDuel(
  program: DuelProgram,
  caller: PublicKey,
  creator: PublicKey,
  opponent: PublicKey,
  gameId: BN
) {
  const [duel] = deriveDuelPda(gameId, creator, program.programId);
  const [treasury] = deriveTreasuryPda(program.programId);
  const duelAcc = await program.account.duel.fetch(duel);
  return program.methods
    .settleDuel(gameId)
    .accounts({
      caller,
      creator,
      opponent,
      treasury,
      duel,
      random: duelAcc.randomness as PublicKey,
    })
    .rpc();
}

/**
 * Close / refund.
 *  - Waiting: creator cancels (any time) or anyone after 10 min -> refund creator.
 *  - Rolling (stuck VRF) after 24h expiry: pass the real opponent -> refund both.
 *  - Settled: rent cleanup.
 */
export async function closeDuel(
  program: DuelProgram,
  caller: PublicKey,
  creator: PublicKey,
  gameId: BN,
  opponent?: PublicKey
) {
  const [duel] = deriveDuelPda(gameId, creator, program.programId);
  return program.methods
    .closeDuel(gameId)
    .accounts({ caller, creator, opponent: opponent ?? creator, duel })
    .rpc();
}

/** Creator cancels their own unmatched duel and gets refunded. */
export async function cancelUnmatchedDuel(
  program: DuelProgram,
  creator: PublicKey,
  gameId: BN
) {
  return closeDuel(program, creator, creator, gameId);
}

// ---------------------------------------------------------------------------
// Reads + realtime
// ---------------------------------------------------------------------------

export async function fetchDuel(program: DuelProgram, duel: PublicKey) {
  return program.account.duel.fetch(duel);
}

export function subscribeDuelEvents(
  program: DuelProgram,
  handlers: {
    onCreated?: (e: any) => void;
    onJoined?: (e: any) => void;
    onSettled?: (e: any) => void;
    onClosed?: (e: any) => void;
  }
): number[] {
  const ids: number[] = [];
  const add = (name: string, cb?: (e: any) => void) => {
    if (cb) ids.push(program.addEventListener(name as any, cb));
  };
  add("duelCreated", handlers.onCreated);
  add("duelJoined", handlers.onJoined);
  add("duelSettled", handlers.onSettled);
  add("duelClosed", handlers.onClosed);
  return ids;
}
