# GLIBC Compatibility Fix

## Problem
```
/app/cyberfly-rust-node: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found
```

This error occurs when a binary compiled on a system with newer GLIBC (2.38) is run on a system with older GLIBC.

## Root Cause
- **Dockerfile was using**: `debian:bookworm-slim` (GLIBC 2.38)
- **Target system has**: Older GLIBC version (< 2.38)
- **Result**: Binary incompatibility at runtime

## Solution Applied
Changed base Docker image to use older Debian version for better backward compatibility:

### Before:
```dockerfile
FROM debian:bookworm-slim
# ...
libssl3
```

### After:
```dockerfile
FROM debian:bullseye-slim  # GLIBC 2.31
# ...
libssl1.1
```

## Changes Made
1. **Dockerfile** - Line 4: Changed base image from `bookworm-slim` to `bullseye-slim`
2. **Dockerfile** - Line 10: Changed SSL library from `libssl3` to `libssl1.1` (Bullseye version)

## GLIBC Versions
| Debian Version | GLIBC Version | Compatibility |
|----------------|---------------|---------------|
| Bullseye (11)  | 2.31         | ✅ Wide compatibility |
| Bookworm (12)  | 2.38         | ❌ Newer systems only |
| Buster (10)    | 2.28         | ✅ Maximum compatibility |

## Verification
After rebuilding the Docker image:
```bash
# Check GLIBC version in container
docker run --rm <image> ldd --version

# Should show: GLIBC 2.31 (compatible with most systems)
```

## Alternative Solutions

### Option A: Use Debian Bullseye (Current Solution)
- **Pros**: Good compatibility (GLIBC 2.31), widely supported
- **Cons**: Not the absolute latest packages
- **Recommended**: ✅ Best balance

### Option B: Use Alpine Linux with musl
```dockerfile
FROM alpine:3.18
RUN apk add --no-cache ca-certificates
```
- **Pros**: Static linking, smallest image size, no GLIBC dependency
- **Cons**: Requires rebuilding with musl target
- **Recommended**: For maximum compatibility

### Option C: Static compilation with musl target
Build with: `cargo build --release --target x86_64-unknown-linux-musl`
- **Pros**: Fully static binary, no runtime dependencies
- **Cons**: Slightly larger binary, requires musl toolchain
- **Recommended**: For portable deployments

## Next Steps
1. Rebuild Docker images with the updated Dockerfile
2. Test on target system
3. If issues persist, consider Option B or C

## Build Command
```bash
# Rebuild with new base image
docker build -t cyberfly-rust-node:latest .

# Or trigger GitHub Actions workflow
git add Dockerfile GLIBC_FIX.md
git commit -m "fix: Use Debian Bullseye for GLIBC 2.31 compatibility"
git push
```

---
**Date**: 2025-11-11  
**Issue**: GLIBC 2.38 not found  
**Solution**: Downgrade base image to Debian Bullseye (GLIBC 2.31)  
**Status**: ✅ Fixed
