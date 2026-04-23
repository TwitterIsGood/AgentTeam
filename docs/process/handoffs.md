# Handoffs

当一个 WorkItem 被中断、转交或跨会话继续时，必须留下标准 handoff。

## 必填字段

- objective
- current state
- next step
- verification

## 落盘位置

- `workitems/<id>/summary.md`
- `workitems/<id>/checkpoints/<timestamp>.json`

## 原则

- handoff 以恢复执行为目标，不追求完整聊天复述
- 需要说明当前阶段和阻塞点
- 必须能够让新对话在最少上下文下恢复工作
