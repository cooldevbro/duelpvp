<div align="center">

![DUELPVP — Bet. Play. Win.](./DUELPVP%20BANNER.png)

# DUELPVP

### Real SOL duels. Real stakes. Real competition.

**The fully on-chain 1v1 dice game where the blockchain rolls the dice — not us.**

[![Network](https://img.shields.io/badge/Solana-Devnet-9945FF?style=for-the-badge&logo=solana&logoColor=white)](https://solana.com)
[![Randomness](https://img.shields.io/badge/Randomness-ORAO_VRF-FFB300?style=for-the-badge)](https://orao.network)
[![House Fee](https://img.shields.io/badge/House_Fee-1%25-2ecc71?style=for-the-badge)](#-fees)
[![Site](https://img.shields.io/badge/duelpvp.gg-000000?style=for-the-badge)](https://duelpvp.gg)

</div>

---

## 🎲 What is DUELPVP?

DUELPVP is a head-to-head dice duel on Solana. Two players each stake SOL, the
**smart contract** rolls two dice for each side using verifiable randomness, and
the winner takes the pot. Simple, fast, and provably fair.

> **The golden rule:** the outcome is decided **on-chain** by a cryptographic
> coin-flip nobody can predict or rig. The website only *animates* a result that
> the blockchain has already locked in. Not us, not you, not the other player —
> **no one** can change who wins.

---

## ⚡ How a duel works

| Step | What happens |
|:----:|:-------------|
| **1. Create** | You pick your bet, your win-condition (**higher** or **lower** total wins), and whether it's public or invite-only. Your SOL goes into a unique vault for *that game only*. |
| **2. Join** | An opponent matches your bet. In the **same transaction**, fresh randomness is requested from ORAO VRF — so nobody can see the result before committing. |
| **3. Roll & Settle** | A few seconds later the randomness lands. Anyone can trigger `settle` — the contract rolls 2 dice per player, compares totals, and **instantly pays the winner**. |
| **4. Win** | The winner receives the full pot minus a **1% house fee**. A tie refunds both players in full, no fee. |

If nobody joins your duel, you get a **full refund** — your SOL never leaves your
own game vault, and it never touches the house.

---

## 🛡️ Why it's provably fair

- **The chain rolls the dice, period.** Dice come straight from [ORAO VRF](https://orao.network)
  — verifiable randomness produced by a network of oracle nodes and signed
  cryptographically. It's impossible to predict or grind.
- **No early peeking.** The randomness seed is fresh entropy supplied by the
  *joiner* at join time. Until that transaction lands, the result doesn't exist —
  so neither player can know the outcome before putting money down.
- **The site is just animation.** When the dice tumble on screen, the result is
  *already final on-chain*. The animation is pure theater over a settled fact.
- **Unbiased dice.** Each face (1–6) is exactly equally likely — we use rejection
  sampling so there's no statistical edge hiding in the math.

This is the same on-chain-VRF trust model that powers the biggest Solana degen
games — built here for true 1v1 PvP.

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

## 🌐 Live deployment (Devnet)

| Item | Value |
|:-----|:------|
| **Program ID** | `FpVpkZzyW9tdbXxH9ZUMSe9sghnroDNUkw7uiEgPJ89q` |
| **Treasury (fee vault)** | `Afy9n47HACod2VNhAzGYSWquTs8iZ1g3P5NpWFCEYbH5` |
| **Admin** | `77LXpmiHMcKiX9ACmZgpDTii2EkGtqGHkqz9DPkfeg6j` |
| **Randomness** | ORAO VRF (`VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y`) |

> ⚠️ Currently on **Devnet** for testing. A fresh program and multisig-secured
> treasury will be deployed for Mainnet (see [Roadmap to Mainnet](#-roadmap-to-mainnet)).

---

## 🧱 Project structure

```
duelpvp/
├── programs/duelpvp/src/
│   ├── lib.rs        # the on-chain program: create, join, settle, close
│   ├── state.rs      # account layouts (Duel, Treasury)
│   └── errors.rs     # custom error messages
├── app/
│   ├── src/anchor-client.ts   # TypeScript client (call this from the frontend)
│   └── DiceDuel.html          # standalone animated demo of the full flow
├── scripts/init-treasury.ts   # one-time setup, run once after deploy
├── tests/duelpvp.ts           # automated tests
└── target/idl/duelpvp.json    # the interface the frontend imports
```

---

## 🕹️ The instruction set

| Instruction | Who calls it | What it does |
|:------------|:-------------|:-------------|
| `create_duel` | Creator | Open a duel, lock in the bet. |
| `join_duel` | Opponent | Match the bet + request randomness. |
| `settle_duel` | Anyone | Roll dice, pay the winner. |
| `close_duel` | Anyone | Refund (no-join / tie / stuck game) and sweep fees. |
| `initialize_treasury` | Admin | One-time setup of the fee vault. |
| `set_paused` / `set_max_bet` | Admin | Safety switches. |
| `withdraw_treasury` | Admin | Collect accumulated fees. |

> **For frontend devs:** import the IDL at `target/idl/duelpvp.json` (or fetch it
> on-chain with `anchor idl fetch <program id>`). The helpers in
> `app/src/anchor-client.ts` build every instruction for you.

---

## 🛠️ Build & run (for developers)

This program builds with the modern Solana toolchain. **Use `cargo build-sbf`,
not `anchor build`.**

```bash
# 1. Build the on-chain program (targets the SBPF version Devnet accepts)
cargo build-sbf --arch v3

# 2. Deploy to devnet
solana program deploy target/deploy/duelpvp.so \
  --program-id target/deploy/duelpvp-keypair.json --url devnet

# 3. One-time: initialize the fee treasury (run by the deployer)
ANCHOR_PROVIDER_URL=https://api.devnet.solana.com \
ANCHOR_WALLET=~/.config/solana/id.json \
  npx ts-node scripts/init-treasury.ts

# 4. Run the tests against a local validator (clones ORAO from devnet)
solana-test-validator -r \
  --clone VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y \
  --clone 5ER1oENnV4srxYdAynUfRzWeQCPQaqMiAp4VqyMbSqnK \
  --upgradeable-program FpVpkZzyW9tdbXxH9ZUMSe9sghnroDNUkw7uiEgPJ89q \
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
- [x] Devnet deployment + automated tests
- [ ] **Professional third-party security audit** *(required before real funds)*
- [ ] Multisig-secured treasury & upgrade authority
- [ ] Mainnet launch + token

---

## ⚠️ Disclaimer

DUELPVP is currently deployed on **Devnet** and has **not yet been independently
audited**. Do not use real funds until a professional security audit is complete
and the project has launched on Mainnet. Gambling may be regulated or restricted
in your jurisdiction — play responsibly and know your local laws.

<div align="center">

---

**[duelpvp.gg](https://duelpvp.gg)** · Bet. Play. Win.

</div>
