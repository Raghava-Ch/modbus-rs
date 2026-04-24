# Building Server Applications

Entry point for choosing the right server development path.

This page is intentionally brief. Use the dedicated sync and async guides for full implementation details.

---

## Choose Your Runtime

| Runtime | Best for | Primary API | Full Guide |
|---|---|---|---|
| Sync (poll-driven) | Tight loop control, predictable execution, simple deployment | `ServerServices` | [Sync Server Applications](sync.md) |
| Async (Tokio) | Concurrent TCP clients, async side effects, task-based systems | `AsyncTcpServer`, `AsyncRtuServer`, `AsyncAsciiServer` | [Async Server Applications](async.md) |

---

## Minimal Building Notes

### Sync server shape

1. Build a transport + `ModbusConfig`
2. Implement server handlers directly or via `#[modbus_app]`
3. Create `ServerServices`
4. `connect()` and drive with `poll()`

See full details: [Sync Server Applications](sync.md).

### Async server shape

1. Use Tokio runtime and async server API
2. Implement app logic with `#[async_modbus_app]` or `AsyncAppHandler`
3. Run `AsyncTcpServer::serve(...)` or `serve_shared(...)`
4. For serial async, construct RTU/ASCII async server and run loop

See full details: [Async Server Applications](async.md).

---

## Data Model And Hook Guidance

- Sync macro and handler details: [Sync Server Applications](sync.md#data-models)
- Async macro and hook behavior: [Async Server Applications](async.md#async-write-hooks)

---

## See Also

- [Quick Start](quick_start.md)
- [Examples](examples.md)
- [Function Codes](function_codes.md)
- [Write Hooks](write_hooks.md)
- [Policies](policies.md)
- [Architecture](architecture.md)
