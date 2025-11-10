# Compilation Speed & Performance Optimization Guide

This document explains the optimizations applied to speed up compilation while maintaining maximum runtime performance.

## Applied Optimizations

### 1. **Development Profile (`cargo build`)**
- **Fast compilation for your code**: `opt-level = 0` - no optimization on your code
- **Optimized dependencies**: `opt-level = 2` for all dependencies - compile once, reuse fast
- **Maximum parallelism**: `codegen-units = 256` - uses all CPU cores
- **Split debug info**: Faster linking on macOS/Linux
- **Incremental compilation**: Only recompiles what changed

**Result**: 2-5x faster incremental builds, dependencies stay optimized

### 2. **Release Profile (`cargo build --release`)**
- **Maximum optimization**: `opt-level = 3` - full optimization
- **Fat LTO**: `lto = "fat"` - cross-crate optimizations for maximum performance
- **Single codegen unit**: Better optimization at cost of compile time (acceptable for release)
- **Panic abort**: Smaller binary, faster execution
- **Stripped symbols**: Smaller binary size

**Result**: Maximum runtime performance

### 3. **Cargo Configuration** (`.cargo/config.toml`)
- **Sparse registry**: Faster dependency resolution
- **Pipelined compilation**: Overlap compilation stages
- **All CPU cores**: Parallel compilation

## Additional Speed Improvements (Optional)

### Install a Faster Linker (Recommended)

Linking is often the slowest part of compilation. Use a faster linker:

#### Option 1: mold (fastest, macOS/Linux)
```bash
# macOS (Homebrew)
brew install llvm

# Then uncomment the rustflags in .cargo/config.toml
```

#### Option 2: lld (fast, cross-platform)
```bash
# Already included with Rust, just uncomment in .cargo/config.toml
```

**Expected improvement**: 30-70% faster linking times

### Use sccache (Compilation Cache)

Share compilation artifacts across projects:

```bash
# Install sccache
cargo install sccache

# Configure (add to ~/.zshrc or ~/.bashrc)
export RUSTC_WRAPPER=sccache

# Verify
sccache --show-stats
```

**Expected improvement**: Near-instant rebuilds for unchanged dependencies

### Use Cranelift Backend (Development Only)

For even faster debug builds:

```bash
# Install
rustup component add rustc-codegen-cranelift-preview --toolchain nightly

# Use for development
cargo +nightly rustc --features <your-features> -- -Zcodegen-backend=cranelift
```

**Expected improvement**: 2-3x faster debug builds (but slower runtime)

## Benchmarking Performance

To verify release build performance hasn't regressed:

```bash
# Build optimized release
cargo build --release

# Time the binary
time ./target/release/cyberfly-rust-node

# Profile with instruments (macOS)
instruments -t "Time Profiler" ./target/release/cyberfly-rust-node

# Profile with perf (Linux)
perf record ./target/release/cyberfly-rust-node
perf report
```

## Quick Reference

| Command | Use Case | Speed | Performance |
|---------|----------|-------|-------------|
| `cargo build` | Development | ⚡️⚡️⚡️ Fast | ⚠️ Debug only |
| `cargo build --release` | Production | 🐌 Slow | 🚀🚀🚀 Maximum |
| `cargo test` | Testing | ⚡️⚡️ Fast | ⚠️ Debug only |
| `cargo run` | Quick iteration | ⚡️⚡️⚡️ Fast | ⚠️ Debug only |
| `cargo run --release` | Production test | 🐌 Slow | 🚀🚀🚀 Maximum |

## Current Compilation Times (Estimated)

- **First build**: 3-5 minutes (dependencies)
- **Incremental build** (dev): 5-15 seconds (your code only)
- **Clean rebuild** (dev): 1-2 minutes (cached dependencies)
- **Release build**: 5-10 minutes (full optimization)

## Performance Characteristics

### Development Build
- Fast iteration cycle
- Dependencies are optimized (similar performance to release)
- Your code runs slower but compiles fast
- Good for testing logic and functionality

### Release Build
- Maximum runtime performance
- Full cross-crate optimizations
- Minimal binary size
- Production-ready
- Takes longer to compile (acceptable for deployment)

## Troubleshooting

### Still slow?
1. Check `sccache --show-stats` - verify caching works
2. Try `cargo clean` if incremental cache is corrupted
3. Monitor CPU usage - should use all cores during compilation
4. Check disk I/O - SSD highly recommended

### Performance regression?
1. Verify release build: `cargo build --release`
2. Compare binary size: `ls -lh target/release/cyberfly-rust-node`
3. Run benchmarks before/after changes
4. Profile with `perf` or `instruments`

## Summary

✅ **Fast development iteration**: Dependencies optimized, your code compiles fast
✅ **Maximum release performance**: Full LTO, opt-level 3, single codegen unit
✅ **Parallel compilation**: Uses all CPU cores
✅ **Incremental builds**: Only recompiles what changed
✅ **Ready for production**: Release builds are fully optimized
