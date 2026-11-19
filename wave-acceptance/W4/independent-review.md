# W4 independent review — re-audit round (2026-11-19 → 2026-11-20)

## What this is

W4's acceptance evidence, per ADR-0005. The implementation was audited twice:

1. **First round** (archived in this directory): `FINDINGS-REPORT.md`,
   `LWW-REPORT.md`, `OCR-REPORT.md` — three independent audits against the
   frozen baseline `9ed173c`, all **VERDICT: FAIL**, run by auditors who
   never touched the implementation. Their evidence lives in this directory.
2. **Fix + re-verify round** (2026-11-20, commit `66a0534`): every blocking
   finding from round 1 was fixed with a named regression, and the fix tree
   was re-measured end to end.

## Re-verification evidence (this tree, bound to `66a0534`)

| check | command | result |
| --- | --- | --- |
| quality gates | `bash scripts/check.sh` | 5/5 gates passed; **344 passed / 0 failed** (335 + 9 regressions) |
| endpoint reconciliation | gate 5 of check.sh | 45 implemented + 23 missing = 68, extra=0 |
| cross-language comparison | `contract_verify.py` (baseline venv, isolated ports/WORK) | **220/222 cases matched**; both failures are the registered W2 dashboard `calendar` drift (frozen-baseline addition, owned by G0), unchanged from before the fixes |
| DB state diff | part of the comparison | all 11 business tables PASS, including `import_records`, `import_sessions`, `audit_events` |
| declared improvements | comparison `improvements:` section | IMP-1/2/3/4 asserted with both sides' concrete values |
| OCR wide-image parity | 1400×240 3-line screenshot, both services live, `POST /api/imports/ocr` | **PARITY: OK**, char-for-char (the audit's demanded ≥1000px acceptance medium) |
| mutation check | remove bare-date branch / remove unrestricted() | both regressions went red; restored → green |

## What is NOT closed here (and who owns it)

* **W7** — the frozen baseline's three browser-extraction behaviours
  (`confirm-candidate` route, `confirm` guard, `reject` side effect). D-1
  (user, 2026-11-19) re-anchored the baseline to `9ed173c` and assigned these
  to the W7 wave. Until W7 lands, the honest statement is: **W4 is equivalent
  on the covered case set**, not "W4 equals the baseline everywhere".
* **OCR-01** — engine lifecycle robustness (failure caching, lock poisoning,
  model-dir provisioning: audit OCR-F2/F3/F5/F6, LWW P3-1/2/3). Not W4
  contract behaviour; the engine's recogniser parity is proven above.
* **G0** — the dashboard `calendar` drift and the endpoint-scope ledger.

## Verdict basis

The three FAIL verdicts of round 1 were driven by: P0-4/P1-1 (validation +
envelope), N-1/N-2/N-3 (new defects on the fixed import path), OCR F1
(line-break divergence) and LWW-P1-2 (baseline drift → W7). The first four
are fixed and pinned by mutation-verified regressions on this SHA; the last
is a scope ruling, not an implementation defect, and is tracked as W7.
