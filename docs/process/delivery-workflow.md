# Delivery Workflow

ForgeFlow 默认交付流程：

1. Intake
2. Roundtable
3. Architecture
4. Implement
5. Test
6. Review
6. PR
8. Release

## 阶段要求

每个阶段都必须定义：

- 输入工件
- 输出工件
- 进入条件
- 退出条件
- 失败回退路径
- 可审计事件

## V0 范围

V0 只要求跑通以下最小路径：

- 创建 WorkItem
- 记录 summary
- 写入 checkpoint
- 生成 dry-run workflow events
- 输出阶段状态

## 完成定义

一个 WorkItem 至少要有：

- `summary.md`
- `checkpoints/`
- `events/`
- `artifacts/`
- 对应 schema 可校验的结构化记录
