# ADR 0001: Use a Rust workspace as the ForgeFlow foundation

- Status: Accepted
- Date: 2026-04-23

## Context

ForgeFlow 需要成为一个可商用、可审计、可恢复、可并行开发的多 Agent 软件交付系统。项目从绿地开始，需要尽早固定边界、模块职责和演进方式。

## Decision

采用 Rust workspace 作为项目基础，并按职责拆分 crate，而不是在单一二进制或单一库中堆积所有能力。

## Consequences

### Positive

- 可以在早期就固定领域边界与依赖方向
- 有利于测试、替换、演进和并行开发
- 类型系统和编译器约束适合状态机、schema、事件模型

### Negative

- 初期样板代码更多
- 需要明确控制 crate 间依赖，避免回流

## Notes

当前阶段优先建设最小可编译骨架，而非完整业务实现。
