# ForgeFlow Overview

ForgeFlow 是一个围绕 WorkItem 运转的软件交付操作系统。它通过结构化工件、事件日志、checkpoint 和状态机编排，把多 Agent 协作从“聊天过程”提升为“可恢复、可审计、可回放的工程流程”。

## 核心原则

1. **WorkItem-first**：所有长期状态围绕 WorkItem ID 组织。
2. **Artifacts over chat**：阶段输出必须落为结构化工件。
3. **State machine over freeform orchestration**：流程由显式状态驱动。
4. **File-first persistence**：先用文件、schema、event + checkpoint 验证架构。
5. **Provider abstraction**：provider 细节不泄漏到 domain / orchestrator。
6. **Parallel delivery**：每个 WorkItem 独立 branch/worktree，并通过公共工件共享信息。

## 分层

- **L4 Runtime**：模型 provider 与执行抽象
- **L3 Coordination**：orchestrator、state machine、gate、resume
- **L2 Harness**：agents、workflow 模板、结构化协议
- **L1 ProjectOps**：repo、process、handoff、review、release
- **L0 Storage**：schemas、events、checkpoints、memory records

## 最小运行路径

V0 以 CLI-first 方式验证：

1. 初始化仓库
2. 创建 WorkItem 目录
3. 写入 checkpoint
4. 通过 fake runtime 跑 dry-run workflow
5. 从 summary / checkpoint / events 恢复上下文
