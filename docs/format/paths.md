# Paths: `paths/<id>.toml`

Part of [format v0](v0.md).

```toml
#:schema ../../../client/docs/format/schema/v0/path.schema.json
points = [[0, 0], [32, 0, 50], [32, 32]]   # [x, y] or [x, y, speed factor]; the factor defaults to 100
closed = false                             # the last point connects to the first
smooth = false                             # curved path: not supported by the sim yet (a warning)
precision = 4                              # smoothing precision
```

At least two points. The path id is the file stem; params name a path by index (the loader orders paths by file path).
