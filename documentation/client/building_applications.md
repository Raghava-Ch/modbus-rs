# Building Client Applications

Entry point for choosing the right client development path.

This page is intentionally brief. Use the dedicated sync and async guides for full implementation details.

---

## Choose Your Runtime

| Runtime | Best for | Primary API | Full Guide |
|---|---|---|---|
| Sync (poll-driven) | Deterministic loops, embedded-style control, thread-owned state | `ClientServices` | [Sync Development](sync.md) |
| Async (Tokio) | Concurrent I/O, task-based orchestration, await-first flows | `AsyncTcpClient`, `AsyncSerialClient` | [Async Development](async.md) |

---

## Minimal Building Notes

### Sync client shape

1. Build a `ModbusConfig` + transport (`StdTcpTransport`, `StdRtuTransport`, `StdAsciiTransport`)
2. Implement callback traits on your app (`RequestErrorNotifier`, response traits, `TimeKeeper`)
3. Create `ClientServices`
4. `connect()` and drive a poll loop with `poll()`

See full details: [Sync Development](sync.md).

### Queue depth (`ClientServices::<_, _, N>`) sizing

`N` is the maximum number of in-flight requests the sync client will track.

- Lower `N` reduces memory usage and is better for constrained MCUs.
- Higher `N` increases tolerated burst/concurrency before backpressure.
- A practical starting point is `N=2..4` on embedded targets and `N=8+` on host-class targets.

Tune using your real poll cadence and request burst profile: if calls are frequently waiting
for free slots, increase `N`; if memory is tight and slots stay mostly idle, reduce `N`.

### Async client shape

1. Enable `async` feature and use Tokio runtime
2. Construct `AsyncTcpClient` or `AsyncSerialClient`
3. `connect().await?`
4. Issue requests directly with `.await`

See full details: [Async Development](async.md).

---

## Transport Decision

- For sync transport configuration and retries: [Sync Development](sync.md#transport-configuration)
- For async TCP and serial construction patterns: [Async Development](async.md#async-tcp-client)

---

## See Also

- [Quick Start](quick_start.md)
- [Examples](examples.md)
- [Feature Flags](feature_flags.md)
- [Policies](policies.md)
- [Architecture](architecture.md)
