<div align="center">

![DUELPVP — Bet. Play. Win.](./DUELPVP%20BANNER.png)

# DUELPVP

### Real SOL duels. Real stakes. Real competition.

**The fully on-chain 1v1 gaming protocol where the blockchain decides the winner — not us.**

[![Network](https://img.shields.io/badge/Solana-Mainnet-9945FF?style=for-the-badge&logo=solana&logoColor=white)](https://solana.com)
[![Randomness](https://img.shields.io/badge/Randomness-ORAO_VRF-FFB300?style=for-the-badge)](https://orao.network)
[![House Fee](https://img.shields.io/badge/House_Fee-1%25-2ecc71?style=for-the-badge)](#-fees)
[![Site](https://img.shields.io/badge/duelpvp.fun-000000?style=for-the-badge)](https://duelpvp.fun)

</div>

---

## ⚔️ What is DUELPVP?

DUELPVP is a head-to-head **PvP gaming protocol** on Solana. Two players each
stake SOL, the **smart contract** decides the winner using verifiable
randomness, and the winner takes the pot. Simple, fast, and provably fair.

The live games are a **dice duel** and a **coin flip** — and DUELPVP is built as
a platform. Both run on the same trustless escrow + randomness engine, and new
PvP games inherit the same provably-fair guarantees.

> **The golden rule:** the outcome is decided **on-chain** by cryptographic
> randomness nobody can predict or rig. The website only *animates* a result the
> blockchain has already locked in. Not us, not you, not the other player —
> **no one** can change who wins.

---

## ⚡ How a duel works

| Step | What happens |
|:----:|:-------------|
| **1. Create** | You pick your bet, the game rules, and whether it's public or invite-only. Your SOL goes into a unique vault for *that match only*. |
| **2. Join** | An opponent matches your bet. In the **same transaction**, fresh randomness is requested from ORAO VRF — so nobody can see the result before committing. |
| **3. Play & Settle** | A few seconds later the randomness lands. Anyone can trigger `settle` — the contract resolves the match from the verified randomness and **instantly pays the winner**. |
| **4. Win** | The winner receives the full pot minus a **1% house fee**. A tie refunds both players in full, no fee. |

If nobody joins your match, you can **cancel anytime for a full refund** — your
SOL never leaves your own game vault, and it never touches the house.

---

## 🛡️ Why it's provably fair

- **The chain decides, period.** Outcomes come straight from [ORAO VRF](https://orao.network)
  — verifiable randomness produced by a network of oracle nodes and signed
  cryptographically. It's impossible to predict or grind.
- **No early peeking.** The randomness seed is fresh entropy supplied by the
  *joiner* at join time. Until that transaction lands, the result doesn't exist —
  so neither player can know the outcome before putting money down.
- **The site is just animation.** Whatever plays out on screen, the result is
  *already final on-chain*. The animation is pure theater over a settled fact.
- **No hidden edge.** Every outcome is drawn from the raw verified randomness with
  unbiased sampling — no statistical thumb on the scale.

This is the same on-chain-VRF trust model that powers the biggest Solana degen
games — built here as a true 1v1 PvP platform.

---

## 💰 Your money is always safe

Every duel gets its **own dedicated vault** (a Program Derived Address). Funds can
only ever move in three ways, all enforced by code:

```
  ┌─────────────┐     win       ┌──────────┐
  │  Game Vault │ ────────────► │  Winner  │  (pot − 1% fee)
  │  (per duel) │     tie        └──────────┘
  │             │ ────────────► both players refunded
  │  holds the  │     no join    ┌──────────┐
  │  staked SOL │ ────────────► │ Creator  │  (full refund)
  └─────────────┘                └──────────┘
```

- **Refunds always come from your own game's vault** — never from the treasury,
  never from another game. Games can't cross-pay.
- **The winner is always paid from the staked pot** held in escrow. The payout is
  pure on-chain math (no external call), so it can never "run out of gas" or fail
  to pay.
- **The treasury only ever collects the 1% fee.** It cannot be drained into a
  refund or a payout — there is no code path for that.

---

## 🚀 Built to scale

DUELPVP is engineered for **thousands of simultaneous duels**:

- **Every game is independent.** Each duel uses its own accounts, so the network
  processes them **in parallel** — no global queue.
- **Settlements never bottleneck.** The 1% fee is parked in each game's vault and
  swept to the treasury later, so paying out winners never competes for a shared
  lock. Thousands of games can settle at the same time.
- **No griefing.** Because randomness is requested per-game with the joiner's own
  entropy, nobody can clog or front-run the system.

---

## 💸 Fees

| Event | Fee | Goes to |
|:------|:----|:--------|
| Win | **1%** of the total pot | Treasury |
| Tie | **0%** — both players fully refunded | — |
| No opponent | **0%** — creator fully refunded | — |

The fee is collected by the program's **treasury account** and can only be
withdrawn by the admin (the project's deployer key). House fees are the project's
revenue.

---

## 🌐 Deployment

| Item | Value |
|:-----|:------|
| **Network** | Solana Mainnet-Beta |
| **Program ID** | `8NkYNEeX6eUiNrK89cHfNmZoigaUCdi5NLGKgRFJ77oZ` |
| **Treasury (fee vault)** | `6HH6su5MAjcNvVUtpeLkWzijnmzBJDn6GxCovHRRMGWY` |
| **Randomness** | ORAO VRF (`VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y`) |

> 🚀 DUELPVP runs on **Solana Mainnet**. The program carries an on-chain
> [`security.txt`](https://github.com/neodyme-labs/solana-security-txt) so explorers
> show the verified project identity. See [Verify it yourself](#-verify-it-yourself)
> to confirm the deployed bytecode matches this source.

---

## 🧱 Project structure

```
duelpvp/
├── programs/duelpvp/src/
│   ├── lib.rs        # the on-chain program: create, join, settle, close
│   ├── state.rs      # account layouts (Duel, Treasury)
│   └── errors.rs     # custom error messages
├── app/
│   ├── idl/duelpvp.json       # the interface the frontend imports
│   ├── src/anchor-client.ts   # TypeScript client (call this from the frontend)
│   └── DiceDuel.html          # standalone animated demo of the full flow
├── scripts/init-treasury.ts   # one-time setup, run once after deploy
└── tests/duelpvp.ts           # automated tests
```

---

## 🕹️ The instruction set

| Instruction | Who calls it | What it does |
|:------------|:-------------|:-------------|
| `create_duel` | Creator | Open a duel (dice or coin flip), lock in the bet. |
| `join_duel` | Opponent | Match the bet + request randomness. |
| `settle_duel` | Anyone | Resolve the match from VRF, pay the winner. |
| `close_duel` | Creator / anyone | Cancel-refund an unmatched duel (creator only) or settle-sweep / stuck-game refund. |
| `initialize_treasury` | Admin | One-time setup of the fee vault. |
| `set_paused` / `set_max_bet` | Admin | Safety switches. |
| `withdraw_treasury` | Admin | Collect accumulated fees. |

> **For frontend devs:** import the IDL at `app/idl/duelpvp.json`. The helpers in
> `app/src/anchor-client.ts` build every instruction for you. `create_duel` takes
> a `kind` (`{ dice: {} }` or `{ coinFlip: {} }`) and a `creatorSide`
> (`{ heads: {} }` / `{ tails: {} }`, ignored for dice). Settlement is a single
> `settle_duel` for both games; dice emits `DuelSettled`, coin flip emits
> `CoinFlipSettled`.

---

## � Verify it yourself

Don't trust — verify. The deployed program is built from this exact source. You
can confirm the on-chain bytecode matches the code in this repo:

```bash
# Hash of the program currently live on mainnet
solana-verify get-program-hash -u mainnet-beta \
  8NkYNEeX6eUiNrK89cHfNmZoigaUCdi5NLGKgRFJ77oZ

# Hash of the program built from this repo
solana-verify get-executable-hash target/deploy/duelpvp.so
```

Both commands return the **same** SHA-256 — proof the live program is exactly
what you see here, with no hidden logic. The program also embeds an on-chain
`security.txt` (project name, site, contact) that block explorers display.

---

## �🛠️ Build & run (for developers)

This program builds with the modern Solana toolchain. **Use `cargo build-sbf`,
not `anchor build`.**

```bash
# 1. Build the on-chain program (targets the SBPF version Solana accepts)
cargo build-sbf --arch v3

# 2. Deploy (point --url at mainnet-beta for production, or devnet for staging)
solana program deploy target/deploy/duelpvp.so \
  --program-id target/deploy/duelpvp-keypair.json --url mainnet-beta

# 3. One-time: initialize the fee treasury (run by the deployer)
ANCHOR_PROVIDER_URL=https://api.mainnet-beta.solana.com \
ANCHOR_WALLET=~/.config/solana/id.json \
  npx ts-node scripts/init-treasury.ts

# 4. Run the tests against a local validator (clones ORAO from a live cluster)
solana-test-validator -r \
  --clone VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y \
  --clone 5ER1oENnV4srxYdAynUfRzWeQCPQaqMiAp4VqyMbSqnK \
  --url https://api.devnet.solana.com \
  --upgradeable-program <YOUR_PROGRAM_ID> \
    target/deploy/duelpvp.so $(solana address) &

ANCHOR_PROVIDER_URL=http://127.0.0.1:8899 \
ANCHOR_WALLET=~/.config/solana/id.json \
  npx ts-mocha -p ./tsconfig.json -t 1000000 tests/duelpvp.ts
```

**Tech:** Anchor `0.29.0` · `orao-solana-vrf 0.4.0` · `@coral-xyz/anchor ^0.29` ·
Solana CLI 4.x (Agave).

> 💡 Run a small **settle bot** in production that calls `settle_duel` the moment
> randomness lands — players get instant payouts and the game stays snappy.

---

## 🗺️ Roadmap to Mainnet

- [x] Core duel logic (create / join / settle / refund)
- [x] ORAO VRF randomness integration
- [x] Parallel-settlement scaling
- [x] Full test suite + on-chain validation
- [x] Dice duel + coin flip game modes
- [x] **Mainnet launch**
- [ ] **Professional third-party security audit**
- [ ] Multisig-secured treasury & upgrade authority
- [ ] Reproducible verified-build badge

---

## ⚠️ Disclaimer

Always do your own research before playing with real funds. Gambling may be
regulated or restricted in your jurisdiction — play responsibly and know your
local laws. DUELPVP's outcomes are decided entirely on-chain by verifiable
randomness; the operator cannot influence any result.

<div align="center">

---

**[duelpvp.fun](https://duelpvp.fun)** · Bet. Play. Win.

</div>
