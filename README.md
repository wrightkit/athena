# Workshop 运行时行为探针（v0）

目的：用真实客户端测出 Workshop 规则调度的实际行为，把结果存成测试 fixture，供之后的模拟器当断言。

和现有 census / live-capture 的区别：census 测的是"客户端接受并能导出这些内容"，这里测的是"规则运行起来的时序和顺序"。两者的结论不要混写。

## 使用方法

1. 在自定义游戏里新建 Workshop 模式，打开 Workshop Inspector 和 Inspector 记录。
2. 粘贴 `probes/<id>/probe.zh-CN.ow`（中文客户端；`probe.workshop` 是可读源码，不用于粘贴），一次只跑一个探针，开始游戏。
3. 每个探针开头都有 `Wait(3)`，等 3 秒后开始记录。所有探针都无需手动操作。
4. 把整个 Inspector 内容原样复制出来，存成同目录下的 `capture.txt`。
5. 按下面的 JSON 模板写同目录下的 `capture.json`，记录客户端版本、日期、locale。

## 探针清单

| 探针 | 要验证的假设 | 能区分的结果 |
|---|---|---|
| P0 | `Total Time Elapsed` 按 tick 更新，8 次 `Wait(0.016)` 每次间隔约 0.016 | 间隔恒为 0.016，说明计时器分辨率是 1 tick；否则其他探针的读数要换算 |
| P1 | Wait 向上取整到 tick 的整数倍：0.001 到 0.016，0.017 到 0.032，0.03 到 0.032，0.1 到 0.112，0.5 到 0.512 | 实测和取整结果一致或不一致，逐项记录 |
| P2 | Ongoing 是边沿触发，Wait 期间同一规则不会被再次激活，Wait 结束时条件仍为 True 也不会立刻重跑 | 总执行次数：2 = 边沿触发且 Wait 期间的边沿被丢弃；3 = Wait 期间的边沿被排队，Wait 结束后补跑；≥4 = 电平触发（结束时仍为 True 就再跑）。以 start 时间戳为准 |
| P3 | 同一 tick 条件成立的规则，按文件顺序从上到下执行；第二批的执行顺序是 D1、D3、D2 | 日志顺序；同一 tick 的时间戳是否相同 |
| P4 | `Wait Until(True, 0)` 经过 0 tick，`Wait(0)` 经过 1 tick，`Wait Until(False, 0.03)` 超时取整到 0.032 | 四个 elapsed 的值 |
| P5 | `Position Of` 约 12.5Hz 更新，`Update Every Frame` 版本约 62.5Hz | 188 条日志里 pos 和 everyframe 各有多少个不同的值：pos 远少于 188 个 = match；pos 也接近 188 个 = mismatch（两列相同也如此，`Update Every Frame` 可能只对持续重算的元素生效，本探针测不到）；pos 恒定或无效 = inconclusive |

P5 用 `Start Forcing Throttle` 强制主机玩家向前移动，无需手动操作。脚本会一直等到主机玩家选好英雄并出生（最长 10 分钟），所以开局后正常选英雄即可；不要在观战状态下跑。前方要有约 3 秒的跑道。

预期结果来自 workshop.codes wiki 和社区讨论。截至 2026-10-19，P0 到 P4 各跑过一次且与假设一致，P5 跑过一次且与假设不一致；每个探针还没有满足"3 次一致"，客户端版本也没有记录，所以都还不是定论。

## 操作与判定

预期值均为假设（tick = 0.016 s）。实测的 Inspector 格式：log 行形如 `[00:00:03] 文本`，时间戳只到秒，不能用来量时间；`Custom String` 里的数字只显示 2 位小数；末尾的变量块显示 3 位小数。所以 P0 到 P4 的时间一律以毫秒打印（差值乘以 1000，字段名带 `_ms`）。每个探针至少跑 3 次，3 次一致才记 `match`。

### 通用步骤

1. 新建自定义游戏，选 Workshop 模式，打开 Workshop Inspector 和 Inspector 记录；确认没有开 Slow Motion。
2. 粘贴一个 `.zh-CN.ow`，开始游戏，不要操作角色。
3. 约 3 秒后日志开始出现；见到该探针最后一行（见下表）后，复制整个 Inspector 内容（变量块加 log，不要只复制 log），存成该探针目录下的 `capture.txt`。变量块是复制时的末态，用来核对循环是否跑完、`runs` 等计数，不用来判断时序。
4. 重复两次，每次一份独立日志。
5. 按下方模板写 `capture.json`，`result` 只能填 `match`、`mismatch`、`inconclusive` 之一。

### 每个探针应得到什么

| 探针 | 日志最后一行 | 预期（假设） | 判定 |
|---|---|---|---|
| P0 | `P0 i=7 ...` | 8 行，`t_ms` 依次为 16, 32, 48, 64, 80, 96, 112, 128；变量块 `i=8` | 步长恒为 16.00 → match；步长约 16.67（60 Hz，依次 16.67, 33.33, 50.00, 66.67）或不恒定 → mismatch，其他探针的读数要换算 |
| P1 | `P1 requested_ms=500 ...` | requested_ms→elapsed_ms：1→16，16→16，17→32，30→32，100→112，500→512 | 全部一致 → 向上取整到 tick；100→96 说明是四舍五入；17→16 说明是向下取整；逐项记录 |
| P2 | `P2 total runs=N` | 看总数 N 和各次 start 的 `t_ms` | N=2，start 约 0 与 2624，end 约 1008 与 3632 → 边沿触发且 Wait 期间丢弃边沿，match；N=3，start 约 0、1008、2624 → Wait 期间的边沿被排队，mismatch；N≥4，相邻 start 约间隔 1008 → 电平触发，mismatch |
| P3 | `P3 done` | 第一批 A、B、C 三行，`t_ms` 相同（约 0）；第二批 D1 → D3 → D2，`t_ms` 约 1008、1024、1040 | A、B、C 顺序与时间戳一致 → 同 tick 按文件顺序，match；顺序不同或 `t_ms` 不同则记下实际值。D 批只验证 Wait 时长，不验证规则顺序 |
| P4 | `P4d ...` | P4a `elapsed_ms`=0，P4b=16，P4c=32，P4d=112 | 四项逐一比对；P4a 为 16 或 P4b 为 0 说明零 tick 假设错 |
| P5 | `P5 i=187 ...` | 开头一行 `P5 start spawned=真 pos=(x, y, z)`（`spawned` 为假或 `pos` 为 0 说明主机玩家没有出生，本次作废）；然后 188 行。`pos` 约 38 个不同值，`everyframe` 约 188 个不同值 | pos 不同值个数接近 38 → match；pos 也接近 188 → mismatch，以实测为准；pos 恒定或为 0 → 主机玩家没出生，本次作废 |

P2 的边沿时刻是 `Wait(0.2)` 取整后的值：0、208、416、2416、2624 ms，不是 200 的整数倍。

### 已有抓取

- P0 第一次抓取（旧版，2 位小数）：`t` 为 0.02, 0.03, 0.05, 0.06, 0.08, 0.10, 0.11, 0.13，`t0` 为 3.023。与 62.5 Hz 一致，排除 60 Hz，但分辨率不足，不记 `match`。用毫秒版重测。

- P5（出生检查版，第一次有效抓取）：188 次采样里 `pos` 和 `everyframe` 都是 188 个不同值，且每一行两者完全相同；前进步长约 0.088/tick（5.5 m/s × 0.016 s）。12.5 Hz 假设不成立，记 `mismatch`。这只说明在 action 里每 tick 读一次时的情况，`Update Every Frame` 在 HUD、特效等持续重算元素里的作用没有测。原始 188 行日志没有存盘。

### 出现意外时

- 没有任何日志：检查 Inspector 记录是否打开，以及是否只粘了一个探针。
- 日志被截断：P5 有 188 行，先确认能完整复制，否则拆成两次或缩短循环。
- P0 就不一致：先停，不要往下跑，把 P0 日志发过来。

## 捕获记录模板

```json
{
  "probe": "p1-wait-rounding",
  "client": { "version": "", "date": "", "locale": "zh-CN" },
  "runs": 1,
  "probe_sha256": "SHA-256 of the pasted probe.zh-CN.ow",
  "hypothesis": "wait rounds up to a whole number of ticks (tick = 0.016 s)",
  "observed": [
    { "requested": 0.001, "elapsed": null },
    { "requested": 0.016, "elapsed": null }
  ],
  "result": "match | mismatch | inconclusive",
  "notes": ""
}
```

`result` 为 `inconclusive` 的探针，对应的模拟器行为写成带说明的未断言用例，不要当成已确认。

## 仓库里怎么放

- 本目录是独立的 git 仓库，探针和捕获放在 `probes/<id>/`，每个目录有 `probe.workshop`、`probe.zh-CN.ow`、`capture.json`、`capture.txt`。
- 要并入 workshop-rs 时，整个 `probes/` 目录搬到 `tools/runtime-probes/`，目录结构不变。
- 导入、导出的 `LiveCapture` 仍留在 workshop-rs-cli，这里只借用它的来源记录字段，不新增 evidence 分级。
- 模拟器放在 workspace 的独立 crate，依赖 workshop-rs，不进语义 crate。

## 模拟器 v0 的范围

只做调度器，足够让 P0 到 P4 的 fixture 当断言：

- tick 循环，每个 tick 16 ms
- 规则实例、Ongoing 边沿触发、Wait 取整到 tick
- Wait Until 和三种 Wait Behavior
- 全局变量和数组
- 输出：按 tick 排序的规则触发 trace 和 `Log To Inspector` 输出

不在 v0 里的：玩家世界、伤害和命中、Position Of 等随世界变化的值。遇到未实现的 action 或 value，直接报 unsupported，不静默跳过。

验收方式：对每个 `result` 为 `match` 的 fixture，`sim(probe.workshop)` 的输出等于 `capture.json` 里的 `observed`。`mismatch` 的以实测为准，模拟器跟着改。

## 注意

- 这些探针是我凭记忆写的 Workshop 文本，没有在游戏或你们的 CLI 里验证过。请先用 workshop-rs-cli 解析一遍，再粘进游戏。我这边的环境 Rust 版本太旧，无法安装它。
- `Custom String` 的参数个数、`Wait Until` 的超时参数类型、`Is Button Held` 的写法，如果解析失败，优先检查这几处。
- Inspector 日志的格式和是否带时间戳，我没有确认。所有时间都靠探针里的 `Total Time Elapsed` 差值。

## Issue 草案（English）

Title: Add runtime-behavior probes and a scheduler simulator for Workshop rules

Body:

The census and live-capture work establishes that the client accepts and exports the declared Workshop surface. It does not say how rules behave at runtime. Today the only runtime references are community notes (workshop.codes wiki, forum posts, one speculative write-up).

Scope:
- Add `tools/runtime-probes/` with probes P0 to P4, each with a pasted-in Workshop program, a captured Inspector log, and a `capture.json` recording client version, date, locale, hypothesis, observed values, and result.
- Add a workspace crate (not the semantic crate) with a scheduler simulator that consumes WIR and reproduces the probes: tick loop, rule instances, edge-triggered Ongoing events, Wait rounding, Wait Until, wait behaviors, variables.
- Unimplemented actions and values return an explicit unsupported error.

Non-goals:
- Simulating game physics, hit detection, or hero abilities.
- Claiming real server cost; any load number is a relative count of evaluations.
- Adding an evidence-class model; probes are ordinary tests with recorded fixtures.

Acceptance:
- For every probe with result `match`, the simulator output equals the captured trace.
- For `mismatch`, the fixture wins and the simulator follows it.
- Probes with `inconclusive` results are recorded as unasserted cases with a note.
