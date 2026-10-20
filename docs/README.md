# athena documentation

Index for the `athena` repository. The root [`README.md`](../README.md) is the entry point; [`AGENTS.md`](../AGENTS.md) holds the agent constraints.

| Document | Responsibility |
| --- | --- |
| [goal.md](goal.md) | What the project is for, what it is not, and the method |
| [architecture.md](architecture.md) | Parts, ownership boundaries, dependency direction, simulator scope |
| [simulator-interface.md](simulator-interface.md) | The simulator entry point and trace format consumers rely on |
| [placement.md](placement.md) | Where the project lives (decided), and what is still open |
| [capture-convention.md](capture-convention.md) | Probe layout, `capture.json` shape, result rules |
| [procedure.zh-CN.md](procedure.zh-CN.md) | Chinese: how to run a probe, expected values, verdict conditions |
| [findings.md](findings.md) | Behavior currently believed, each tied to the probe that supports it |

Per-probe status (runs, result, observed values) is not documented in prose. It lives in `probes/<id>/capture.json`.
