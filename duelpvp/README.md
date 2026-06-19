# DUELPVP — trustless PvP dice duel (Solana / Anchor + ORAO VRF)

A two-player SOL dice duel where the **smart contract** decides the roll and the
front end only animates the result. No house wallet ever holds funds; nobody —
not even you, the operator — can change an outcome.

## The flow

1. **Create** — a player creates a public or private duel and deposits their bet.
   PDA escrow at `["duel", game_id, creator]` holds it.
2. **10-min refund** — if nobody joins, the creator can cancel any time, and
   after 10 minutes anyone (your client/relayer) can trigger the refund. Money
   goes back to the creator, no fee.
3. **Join** — the opponent deposits the matching bet and, in the same
   transaction, requests verifiable randomness from ORAO VRF. The game is now
   fully on-chain; no more player signatures are needed.
4. **Roll + settle** — ORAO fulfils in ~1–3s (your 5-second countdown covers
   it). `settle_duel` (permissionless) reads the randomness, rolls the dice,
   pays the winner the pot minus a 1% fee, and emits `DuelSettled`. A tie
   refunds both bets.
5. **Animate** — the front end catches `DuelSettled`, plays the dice animation
   landing on the on-chain dice, highlights the winner, and links the payout tx
   to Solscan.

## Why it can't cheat

The dice come from `hash`ing ORAO's VRF output — a value produced by a Byzantine
quorum of oracle nodes using EDDSA, cryptographically verifiable, and impossible
for either player (or you) to predict or grind. The VRF seed is derived
deterministically from the duel + opponent, so no one can pick a favorable seed.
The front-end animation is pure theater over a result that is already final
on-chain. This is the same trust model DegenCoinFlip uses (on-chain VRF), adapted
to PvP.

## Files

- `programs/duelpvp/src/lib.rs` — program: treasury, create, join (VRF CPI),
  settle, close/refund.
- `programs/duelpvp/src/state.rs` / `errors.rs` — accounts + errors.
- `app/src/anchor-client.ts` — TS client (PDA derivation, force, all builders,
  `waitForRandomness`, events).
- `app/DiceDuel.html` — self-contained front-end flow demo (open in a browser).
  Port its markup/CSS into your React component; replace the simulated stage
  transitions with the event hooks marked `[HOOK A–D]`.
- `tests/duelpvp.ts` — escrow-path tests + a skipped full-VRF-roll test that
  follows ORAO's harness.

## Build, test, deploy

```bash
# toolchain: Solana CLI 1.18+, Anchor 0.30.1, Node 18+, Yarn
avm use 0.30.1
yarn add @coral-xyz/anchor @solana/web3.js @noble/hashes @orao-network/solana-vrf
yarn add -D tweetnacl chai @types/chai ts-mocha typescript

anchor keys sync          # writes your real program id into lib.rs + Anchor.toml
anchor build
anchor test               # clones ORAO from devnet (see Anchor.toml)

# devnet (ORAO is live there — real fulfillment):
solana config set --url devnet
anchor deploy
# then, once, initialize the fee treasury:
#   ts-node scripts/init-treasury.ts   (calls initializeTreasury)
```

## Migration from your house-wallet flow

Deploy + `initialize_treasury`, then run both systems in parallel. Point "create
duel" at `create_duel` and "accept" at `join_duel` instead of sending SOL to the
house wallet. Flip Supabase from source-of-truth to an index: a small listener
mirrors `DuelCreated/Joined/Settled/Closed` events into your existing `games`
table, so profiles and leaderboard keep working while the **program** decides
outcomes. Drain the old house wallet once legacy duels settle.

## Honest status — read before mainnet

- **Not compiled in the authoring environment.** Run `anchor build && anchor test`
  yourself. The most likely spot to need a tweak is the ORAO integration, since
  its exact account/method surface is version-sensitive.
- **Verify two ORAO specifics against the installed `orao-solana-vrf 0.4.x`:**
  (1) the in-program `RandomnessAccountData::fulfilled()` accessor in
  `read_fulfilled_randomness`, and (2) the `RequestV2` CPI account names. Both
  are cross-checked against ORAO's russian-roulette example — keep that example
  open while building.
- **Version alignment:** pinned to Anchor 0.30.1 to match ORAO 0.4.0. If you move
  to Anchor 1.x, confirm ORAO publishes a compatible crate first.
- **Audit before real funds.** This holds money. Get a professional Solana audit
  and run on devnet under load first. The escrow/payout math, the
  permissionless-settle guard, and the refund paths are the areas to scrutinize.
- **ORAO is an external dependency.** It removes griefing entirely, but the
  24h `close_duel` expiry path exists as a liveness safety net to refund both
  players in the unlikely event a request is never fulfilled.
