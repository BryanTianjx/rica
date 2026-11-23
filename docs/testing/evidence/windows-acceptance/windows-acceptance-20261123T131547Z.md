# Windows real-machine acceptance - preflight evidence

> PREFLIGHT ONLY. Not an acceptance pass, not signed. Captured 2026-11-23T13:15:48.4945395Z.

- Host: Microsoft Windows 11 家庭版 中文版 10.0.26200 (build 26200, 64 位)
- git HEAD: d0077052de9feabd642ccd0c1981bb15ef490c3c (baseline/windows-real-machine-acceptance)
- Installer: not supplied
- Bundle resources placeholder: True

| ID | Scenario | Status | Detail |
|---|---|---|---|
| W1 | Windows clean install | BLOCKED | No installer supplied; nothing was installed. |
| W2 | First launch / health | MANUAL | Launch the installed app on this host; record window appearance, health probe output and crash dialogs. |
| W3 | SQLite schema auto-initialization | MANUAL | Inspect the data dir DDL against crates/ica-infra/schema/ica-baseline.sql (32 tables / 121 indexes). |
| W4 | In-place upgrade keeps data | MANUAL | Install previous version, write business rows, install current version, compare row counts. |
| W5 | Migration idempotence | MANUAL | Check schema_migrations and row snapshots before/after upgrade. |
| W6 | Backup/restore via product path | MANUAL | Mechanism-level drill already exists (TEST-ICA-DRILL-001); product UI/NSIS path still needs real evidence. |
| W7 | Restart after forced termination | MANUAL | taskkill /F the app, restart, verify probe data, backup list and audit trail persisted. |
| W8 | Uninstall keeps user data | MANUAL | Compare registry keys and data dir tree before/after uninstall; native messaging keys must disappear. |
| W9 | Version / build traceability | BLOCKED | Installer version traceability cannot be checked without an installer. |

Machine-readable evidence: windows-acceptance-20261123T131547Z.json

