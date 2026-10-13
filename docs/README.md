# Documentation

Developer documentation, API references, and operational runbooks for the
SimplyTokenized Soroban smart contracts.

## Contract Documentation

| Contract | Doc | Contents |
|---|---|---|
| Token | [contracts/token.md](contracts/token.md) | Spec + full API reference (mint, cap, minter role, whitelist, OZ surface) |
| Crowdsale | [contracts/crowdsale.md](contracts/crowdsale.md) | Spec + full API reference (sale config, pricing modes, SEP-40 oracle, buy flow) |
| Payout | [contracts/payout.md](contracts/payout.md) | Spec + full API reference (distributions, state machine, payout methods) |
| Payout (deep dive) | [PAYOUT_SPECIFICATION.md](PAYOUT_SPECIFICATION.md) | Extended design rationale, invariants, worked examples |

## Guides

- [ARCHITECTURE.md](ARCHITECTURE.md) — system overview, cross-contract flows,
  storage and access models
- [INTEGRATION.md](INTEGRATION.md) — contract IDs, CLI/SDK examples, event
  surface for indexers, pitfalls
- [DEPLOYMENT.md](DEPLOYMENT.md) — build → deploy → wire → verify runbook,
  upgrade procedure, state-archival handling
- [MONITORING.md](MONITORING.md) — alert matrix, OpenZeppelin Monitor +
  uptime-checker setup, incident response, 30-day uptime evidence

## Governance & Security

- [`../GOVERNANCE.md`](../GOVERNANCE.md) — roles, multisig procedures, change
  and incident processes (OpenZeppelin-aligned)
- [`../SECURITY.md`](../SECURITY.md) — security policy, per-function risk
  assessments, mainnet deployment checklist
- [coverage_report.md](coverage_report.md) — test coverage summary

## Monitoring Configs

Ready-to-run OpenZeppelin Monitor and Gatus (uptime) configuration lives in
[`../monitoring/`](../monitoring/README.md) — install instructions in its README.
