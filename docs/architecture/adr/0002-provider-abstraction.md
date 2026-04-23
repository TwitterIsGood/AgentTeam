# ADR 0002: Keep provider details behind runtime abstraction

- Status: Accepted
- Date: 2026-04-23

## Context

ForgeFlow 需要支持 fake runtime、Claude runtime，以及未来的第二 provider。若 provider 细节直接泄漏到领域模型或 orchestrator，后续扩展、回放、策略控制和测试都会变得脆弱。

## Decision

将 provider 细节收敛到 `forgeflow-runtime`，通过统一 trait 输出结构化结果。领域层、workflow 层和 orchestrator 只依赖统一 runtime 能力，不依赖具体 provider SDK 或协议。

## Consequences

### Positive

- domain / orchestrator 与 provider 解耦
- fake runtime 能直接用于 dry-run 与测试
- cost / latency / retry / fallback 可以集中治理

### Negative

- 需要额外的映射层
- 某些 provider 专有能力需要延后抽象

## Notes

V0 先实现 fake runtime，并为 Claude runtime 预留结构。
