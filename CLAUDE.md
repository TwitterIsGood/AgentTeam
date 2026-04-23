# ForgeFlow Collaboration Guide

## 1. 项目使命

ForgeFlow 是一个 Multi-Agent 软件交付操作系统。

它关注的是：

- 围绕 WorkItem 的长期交付状态
- 可审计的结构化工件与事件
- 可恢复的 checkpoint / resume 机制
- 多 WorkItem 并行推进的仓库协作方式
- provider、runtime、policy、repo 集成的分层演进

ForgeFlow 现在不做的事情：

- 不把自由聊天当作主执行模型
- 不把 provider 细节泄漏到领域层或编排层
- 不依赖隐式对话上下文来承载工作状态
- 不在 V0 阶段优先建设 UI 或数据库

## 2. 分层架构

- **L4 Runtime**：模型与外部执行环境抽象，统一 execute / stream / health / cost / capability
- **L3 Coordination**：状态机编排、roundtable、gate、retry、resume
- **L2 Harness**：角色注册、结构化输入输出、workflow 模板、策略装配
- **L1 ProjectOps**：repo、issue、pr、ci、release、handoff、协作流程
- **L0 Storage**：文件持久化、schema、事件、快照、长期记忆索引

## 3. Workspace 地图

- `forgeflow-core`：基础类型、时间、错误、ID、通用帮助函数
- `forgeflow-domain`：WorkItem、Artifact、ExecutionEvent、Checkpoint 等领域模型
- `forgeflow-config`：仓库目录与配置解析
- `forgeflow-memory`：WorkItem 数据目录、checkpoint、事件、summary 持久化
- `forgeflow-runtime`：统一 runtime trait 与 fake runtime
- `forgeflow-agents`：角色目录与结构化 roundtable 角色集合
- `forgeflow-orchestrator`：状态机与阶段迁移
- `forgeflow-workflows`：标准 workflow 模板
- `forgeflow-repo`：repo 对接抽象边界
- `forgeflow-policy`：审批与风险边界策略
- `forgeflow-observability`：日志、指标、回放摘要边界
- `forgeflow-testkit`：测试 fixture 与 workflow 校验帮助
- `forgeflow-cli`：`forgeflow` 命令入口

## 4. WorkItem 生命周期

默认阶段：

1. `Intake`
2. `Roundtable`
3. `Architecture`
4. `Implement`
5. `Test`
6. `Review`
7. `PR`
8. `Release`

每个阶段必须具备：

- 进入条件
- 输入工件
- 退出条件
- 失败回退路径
- 可审计事件记录

## 5. 工件规范

ForgeFlow 的核心工件包括：

- `Position`
- `Critique`
- `Decision`
- `Architecture`
- `ChangeSet`
- `TestReport`
- `Review`

每个工件都必须：

- 绑定 `artifact_id`
- 绑定 `related_workitem`
- 记录 producer、version、path、depends_on
- 可以被后续阶段直接引用

## 6. 长期记忆规则

- 长期记忆只记录未来仍有价值的规则、事实、决策
- 当前执行状态不写入长期记忆，写入 WorkItem checkpoint
- 所有长期状态以 `WorkItem ID` 或 repo 级作用域组织
- resume 时优先读取当前 WorkItem 的 summary、checkpoint、artifacts、events

## 7. 并行开发规则

- 一项工作对应一个 `WorkItem`
- 一个 `WorkItem` 对应一个 branch 或 worktree
- WorkItem 间共享信息必须通过公共工件或 ADR，不能依赖隐式上下文
- 公共文件变更需要显式 owner 与 review

## 8. Handoff 规则

中断时必须留下：

- objective
- current state
- next step
- verification

这些内容应写入当前 WorkItem 的 `summary.md` 与最新 checkpoint。

## 9. 测试与完成定义

一个功能算完成，至少要求：

- 文档同步更新
- schema 同步更新
- 状态流转保持一致
- checkpoint / resume 能恢复上下文
- CLI 或 workflow 行为有测试覆盖

## 10. 禁止事项

禁止：

- 跳过 gate 直接推进阶段
- 在同一改动中混改多个 WorkItem
- 仅留在对话里、不落盘关键记忆或决策
- 未记录 Decision / Architecture 就直接编码关键实现
- 把 repo 操作塞进 agent 角色内部
- 把 policy 写成散落的 if / else
