# 05 — UI 测试与回归基线

## 当前回归基线（2026-11-23，Windows 11 x64，HEAD `f94e0db`）

```text
npm test                          → 25 files / 225 tests passed
npm run build                     → 成功（tsc -b && vite build）
npm run lint                      → 0 errors / 3 warnings（均在 check-ui-reuse.mjs，历史）
npm run check:ui-reuse:regression → exit 0，regressions=0（基线 ui-reuse-baseline-r1-20261123.json）
```

前端依赖曾因 Linux 原生 binding 在 Windows 上无法启动，已通过 `npm install` 修复（package-lock 未变化）。

## 约束

- 任何 UI 2.0 改动必须保持 `npm test` ≥225 全绿、build 成功、lint 无新增警告。
- 回归门禁只拦「指标退化」，不要求一次性 8/8 全绿（当前基线 2/8 通过）。
