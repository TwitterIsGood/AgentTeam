# Parallel Development

## 基本规则

- 每个需求映射为唯一 `workitem/<id>`
- 每个 WorkItem 绑定独立 branch 或 worktree
- WorkItem 状态必须落盘，不能只存在于对话中
- 合并前需要工件对齐：Decision、Architecture、ChangeSet、TestResult

## 并行推进原则

- WorkItem 之间通过公共工件共享，而不是通过隐式上下文共享
- ADR、schema、README、CLAUDE.md 这类公共文件变更必须显式 review
- 对公共资产的修改应指定 owner，避免并发覆盖

## 多对话约束

- 每个对话应明确绑定当前 WorkItem
- 切换 WorkItem 前先更新当前 `summary.md` 与最新 checkpoint
- 新对话恢复上下文时，优先读 `CLAUDE.md` 与 `workitems/<id>/`
