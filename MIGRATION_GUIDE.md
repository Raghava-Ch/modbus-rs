# Modbus-rs Migration Guides Index

This index redirects you to the specific versioned migration guides for `modbus-rs` releases. Use these guides sequentially when upgrading your applications or integrations.

---

## 1. Versioned Migration Guides

### [v0.14.0 Migration Guide (from v0.13.0)](./documentation/migrations/v0.13.0-to-v0.14.0.md)
- **Target Platform**: Node.js Bindings
- **Topic**: Upgrading to the **Transport + Client Factory** API.
- **Key Changes**:
  - Separation of physical socket/port management (`AsyncTcpTransport` / `AsyncRtuTransport`) from logical client endpoints.
  - Relocation of connection lifecycles (`close`, `reconnect`, timeouts) to the transport.
  - Native support for Multi-drop configurations.
---

## 2. General Upgrade Guidelines

1. **Confirm Platform Alignment**: Multi-language bindings (Python, Go, .NET, WASM, C/C++ FFI) follow the Rust Core paradigm but may align on different release intervals. Check individual migration guides for exact version compatibility details.
2. **Apply Sequentially**: If upgrading across multiple breaking releases, apply each migration guide step-by-step and run the respective validation checklists at every stage.
