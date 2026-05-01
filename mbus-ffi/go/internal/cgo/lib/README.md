# Native library directory

This directory holds the platform-specific static archives that the
Go module links against:

```
internal/cgo/lib/
├── linux_amd64/libmbus_ffi.a
├── linux_arm64/libmbus_ffi.a
├── darwin_amd64/libmbus_ffi.a
├── darwin_arm64/libmbus_ffi.a
└── windows_amd64/mbus_ffi.lib
```

The archives are **not** checked into git (they are large binary
blobs). To populate them locally, run:

```
./scripts/build_native.sh           # build for the host platform
```

This invokes `cargo build --release -p mbus-ffi --features go,full`
and copies the resulting archive into the appropriate sub-directory.

For releases, the `.github/workflows/go-bindings.yml` CI workflow
builds all five archives and uploads them as workflow artefacts. Tag
publishing also bundles them into the Go module release commit.
