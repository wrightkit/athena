# 探针操作与判定

本文是各探针判定条件和操作步骤的出处。有 `probes/<id>/expect.json` 的探针（P0 到 P4、P6、P7），其预期日志行以该文件为唯一出处；P5 的统计判定在其 `expect.json` 的 `verdict` 字段。下文表中的预期列只作说明；`athena ingest` 按 `expect.json` 判定（见 [capture-convention.md](capture-convention.md#ingesting-a-log)）。结果状态（`runs`、`result`）以各探针目录下的 `capture.json` 为准，这里不记录。运行流程见 workspace skill `wrightkit-run-workshop-probe`；`capture.json` 的格式见 [capture-convention.md](capture-convention.md)。

## 探针清单

| 探针 | 要验证的假设 | 能区分的结果 |
|---|---|---|
| P0 | `Total Time Elapsed` 按 tick 更新，8 次 `Wait(0.016)` 每次间隔约 0.016 | 间隔恒为 0.016，说明计时器分辨率是 1 tick；否则其他探针的读数要换算 |
| P1 | Wait 向上取整到 tick 的整数倍：0.001 到 0.016，0.017 到 0.032，0.03 到 0.032，0.1 到 0.112，0.5 到 0.512 | 实测和取整结果一致或不一致，逐项记录 |
| P2 | Ongoing 是边沿触发，Wait 期间同一规则不会被再次激活，Wait 结束时条件仍为 True 也不会立刻重跑 | 总执行次数：2 = 边沿触发且 Wait 期间的边沿被丢弃；3 = Wait 期间的边沿被排队，Wait 结束后补跑；≥4 = 电平触发（结束时仍为 True 就再跑）。以 start 时间戳为准 |
| P3 | 同一 tick 条件成立的规则，按文件顺序从上到下执行；第二批的执行顺序是 D1、D3、D2 | 日志顺序；同一 tick 的时间戳是否相同 |
| P3 driver-last | 与 P3 相同，只把 driver 移到所有接收规则之后 | 比较第一批是否晚一 tick，以及 A、B、C 的顺序 |
| P4 | `Wait Until(True, 0)` 经过 0 tick，`Wait(0)` 经过 1 tick，`Wait Until(False, 0.03)` 超时取整到 0.032 | 四个 elapsed 的值 |
| P5 | `Position Of` 约 12.5Hz 更新，`Update Every Frame` 版本约 62.5Hz | 188 条日志里 pos 和 everyframe 各有多少个不同的值：pos 远少于 188 个 = match；pos 也接近 188 个 = mismatch（两列相同也如此，`Update Every Frame` 可能只对持续重算的元素生效，本探针测不到）；pos 恒定或无效 = inconclusive |
| P6 | 三种 Wait Behavior 在条件变假、再变真时的处理 | 用各规则启动次数、完成次数及完成时间区分忽略、中止、重新开始 |
| P7 | heroes 设置作用于上报的玩家状态：health% 放大 Max Health，damageReceived%/healingReceived% 缩放脚本 Damage/Heal，abilityCooldown% 作用于技能冷却 | 四行读数与预期是否一致；`ability1_cooldown` 为 20 说明 `Set Ability Cooldown` 是绝对写入，为 10 说明它也被 50% 缩放 |

P5 用 `Start Forcing Throttle` 强制主机玩家向前移动，无需手动操作。脚本会一直等到主机玩家选好英雄并出生（最长 10 分钟），所以开局后正常选英雄即可；不要在观战状态下跑。前方要有约 3 秒的跑道。

P7 把英雄池限制为 Ana（`enabledHeroes`），开局后正常选英雄出生即可，无需其他操作；读数在 `Wait(3)` 后记录，玩家是哪队、第几个槽位都一样。若 `max_health` 等行显示 0 或缺行，说明读取时玩家未出生，重跑一次。

## 操作与判定

预期值均为假设（tick = 0.016 s）。实测的 Inspector 格式：log 行形如 `[00:00:03] 文本`，时间戳只到秒，不能用来量时间；`Custom String` 里的数字只显示 2 位小数；末尾的变量块显示 3 位小数。所以 P0 到 P4 的时间一律以毫秒打印（差值乘以 1000，字段名带 `_ms`）。每个探针至少跑 3 次，3 次一致才记 `match`。

### 通用步骤

1. 新建自定义游戏，选 Workshop 模式，打开 Workshop Inspector 和 Inspector 记录；确认没有开 Slow Motion。
2. 粘贴一个 `.zh-CN.ow`（中文客户端；`probe.workshop` 是可读源码，不用于粘贴），一次只跑一个探针，开始游戏，不要操作角色。每个探针开头都有 `Wait(3)`，无需手动操作（P5 需要主机玩家出生，见上方探针清单）。
3. 约 3 秒后日志开始出现；见到该探针最后一行（见下表）后，复制整个 Inspector 内容（变量块加 log，不要只复制 log），存成该探针目录下的 `capture.txt`。变量块是复制时的末态，用来核对循环是否跑完、`runs` 等计数，不用来判断时序。
4. 重复两次，每次一份独立日志，依次存成 `capture-2.txt`、`capture-3.txt`。
5. 按 [capture-convention.md](capture-convention.md) 写 `capture.json`，`result` 只能填 `match`、`mismatch`、`inconclusive` 之一。

### 每个探针应得到什么

| 探针 | 日志最后一行 | 预期（假设） | 判定 |
|---|---|---|---|
| P0 | `P0 i=7 ...` | 8 行，`t_ms` 依次为 16, 32, 48, 64, 80, 96, 112, 128；变量块 `i=8` | 步长恒为 16.00 → match；步长约 16.67（60 Hz，依次 16.67, 33.33, 50.00, 66.67）或不恒定 → mismatch，其他探针的读数要换算 |
| P1 | `P1 requested_ms=500 ...` | requested_ms→elapsed_ms：1→16，16→16，17→32，30→32，100→112，500→512 | 全部一致 → 向上取整到 tick；100→96 说明是四舍五入；17→16 说明是向下取整；逐项记录 |
| P2 | `P2 total runs=N` | 看总数 N 和各次 start 的 `t_ms` | N=2，start 约 0 与 2624，end 约 1008 与 3632 → 边沿触发且 Wait 期间丢弃边沿，match；N=3，start 约 0、1008、2624 → Wait 期间的边沿被排队，mismatch；N≥4，相邻 start 约间隔 1008 → 电平触发，mismatch |
| P3 | `P3a done` | 第一批 A、B、C 三行，`t_ms` 相同（约 0）；第二批 D1 → D3 → D2，`t_ms` 约 1008、1024、1040 | A、B、C 顺序与时间戳一致 → 同 tick 按文件顺序，match；顺序不同或 `t_ms` 不同则记下实际值。D 批只验证 Wait 时长，不验证规则顺序 |
| P4 | `P4d ...` | P4a `elapsed_ms`=0，P4b=16，P4c=32，P4d=112 | 四项逐一比对；P4a 为 16 或 P4b 为 0 说明零 tick 假设错 |
| P5 | `P5 i=187 ...` | 开头一行 `P5 start spawned=真 pos=(x, y, z)`（`spawned` 为假或 `pos` 为 0 说明主机玩家没有出生，本次作废）；然后 188 行。`pos` 约 38 个不同值，`everyframe` 约 188 个不同值 | pos 不同值个数接近 38 → match；pos 也接近 188 → mismatch，以实测为准；pos 恒定或为 0 → 主机玩家没出生，本次作废 |
| P7 | `P7 done` | `max_health`=375，`health_after_damage`=275，`health_after_heal`=355，`ability1_cooldown`=20 | 四行与预期逐一比对，全部一致为 match；任一行不符记下实际值（cooldown 为 10 说明 `abilityCooldown%` 也缩放脚本写入） |

### P3 的 driver 末尾变体

文件在 `probes/p3-same-tick-rule-order-driver-last/`。只移动整个 driver 规则，不改动作、条件或日志；独立保存抓取，不混入原 P3 的三次计数。

操作与 P3 相同，等待 `P3b done`（两个 P3 探针的结束行不同：原探针是 `P3a done`），应有 7 条日志。假设按文件顺序扫描时，前面的接收规则要在下一 tick 才看到末尾 driver 写入的条件，因此 A、B、C 依次出现在 16 ms，D1、D3、D2 依次出现在 1024、1040、1056 ms。全部一致记 `match`；若仍为原 P3 的 0、1008、1024、1040 ms，或出现其他完整但不同的时序，也记 `mismatch` 并保留实际值；缺行或未完成记 `inconclusive`。这项假设尚未用客户端验证。

### P6：Wait Behavior

文件在 `probes/p6-wait-behavior/`。只粘贴这个探针，重启自定义游戏后不操作角色，等待 `P6 done`（起始等待 3 秒，随后约 2624 ms）。同时导出全部日志和全部全局变量；不需要观察期间手动改变条件。

driver 把 `f` 依次设为真、假、真、假，计划时刻为 0、208、416、624 ms，并打印每次切换。三个目标规则均以 `f == True` 为条件、等待 1 秒，分别使用 Ignore Condition、Abort When False、Restart When True。每次进入动作列表增加 `nIgnore`、`nAbort` 或 `nRestart`，完成等待才增加相应的 `end*` 并打印完成时间。driver 在 `P6 done` 前另打印一行 `P6 counts`，依次列出 `nIgnore`、`nAbort`、`nRestart`、`endIgnore`、`endRestart`；`endAbort` 不在其中，仍要靠变量导出核对它保持为 0。

以下均为待验证假设：

| 行为 | 预期变量 | 预期完成日志 |
|---|---|---|
| Ignore Condition | `nIgnore=1, endIgnore=1` | `P6 Ignore end t_ms=1008` |
| Abort When False | `nAbort=2, endAbort=0` | 无 Abort end；两次都在等待期间遇到假 |
| Restart When True | `nRestart=2, endRestart=1` | `P6 Restart end t_ms=1424`，在 416 ms 的真边沿重新执行动作列表 |

预期共 8 条日志：4 次 flag、Ignore end、Restart end、`P6 counts nIgnore=1 nAbort=2 nRestart=2 endIgnore=1 endRestart=1`、done；`athena ingest` 按 `expect.json` 逐行判定。计数、完成时间或完成次数不同记 `mismatch` 并逐项记录；尚未见到 done 记 `inconclusive`。如果 Restart 在条件连续为真时反复开始，启动次数会大于 2，完成时间也会不同，这正是本探针要区分的行为，不能删改这些观测。没有任何抓取时 `runs=0`、`result=inconclusive`。

P2 的边沿时刻是 `Wait(0.2)` 取整后的值：0、208、416、2416、2624 ms，不是 200 的整数倍。

### 出现意外时

- 没有任何日志：检查 Inspector 记录是否打开，以及是否只粘了一个探针。
- 日志被截断：P5 有 188 行，先确认能完整复制，否则拆成两次或缩短循环。
- P0 就不一致：先停，不要往下跑，把 P0 日志发过来。
