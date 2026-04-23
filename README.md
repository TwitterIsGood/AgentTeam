# ForgeFlow

ForgeFlow 是一个以 Rust 为基础的 Multi-Agent 软件交付操作系统。

它的目标不是先拼一个“会聊天的多 Agent 系统”，而是先建立可商用、可回放、可审计、可恢复、可并行开发的仓库操作系统：围绕 WorkItem、结构化工件、状态机编排、长期记忆和并行协作规范，逐步沉淀软件交付能力。

## 当前阶段

当前仓库处于 V0 起步阶段，重点是：

- Rust workspace 与 crate 边界
- 文件持久化的 WorkItem / checkpoint / event 模型
- CLI-first 的最小可运行骨架
- fake runtime 驱动的 dry-run workflow
- 文档、ADR、schema、协作规则先行

## 仓库结构

- `crates/`：ForgeFlow workspace crates
- `docs/architecture/`：架构说明与 ADR
- `docs/process/`：流程、协作、handoff、记忆规范
- `schemas/`：结构化文件 schema
- `workitems/`：运行时 WorkItem 数据目录，由 CLI 生成
- `.claude/settings.json`：Claude Code 运行权限与本地行为配置

## 最小命令

```bash
cargo run -p forgeflow-cli -- doctor
cargo run -p forgeflow-cli -- workitem create --id wi-demo --title "Bootstrap ForgeFlow"
cargo run -p forgeflow-cli -- workitem status --id wi-demo
cargo run -p forgeflow-cli -- memory checkpoint --id wi-demo --stage Intake --summary "Scaffold created"
cargo run -p forgeflow-cli -- workflow run --dry-run --id wi-demo
```

## 验证

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
