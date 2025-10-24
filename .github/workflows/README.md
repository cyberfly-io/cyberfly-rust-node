# GitHub Actions Workflows

This directory contains GitHub Actions workflows for automated CI/CD processes with **optimized build times**.

## Workflows

### 1. `docker-publish.yml` - Optimized CI/CD
**Triggers:**
- Push to `main` or `master` branches
- Push of version tags (`v*`)
- Pull requests to `main` or `master`
- **Path filtering**: Skips builds for documentation-only changes

**Build Strategy (Optimized for Speed):**
- **PR builds**: Single platform (linux/amd64), no push, build-only validation
- **Development builds**: Single platform (linux/amd64), faster iteration
- **Release builds**: Multi-platform (linux/amd64, linux/arm64), full compatibility
- **Smart caching**: Multi-layer cache strategy (GitHub Actions + Registry)
- **Conditional execution**: Only builds when relevant files change

**Performance Features:**
- ⚡ **Path filtering**: Ignores `*.md`, `docs/**` changes
- 🚀 **Conditional builds**: Detects code changes vs documentation
- 💾 **Enhanced caching**: Registry + GitHub Actions cache layers
- 📊 **Build monitoring**: Automatic performance reporting
- 🎯 **Platform optimization**: Single platform for development, multi for releases

**Image Tags:**
- Branch pushes: `ghcr.io/owner/repo:branch-name`
- PR builds: `ghcr.io/owner/repo:pr-123`
- Version tags: `ghcr.io/owner/repo:v1.2.3`, `ghcr.io/owner/repo:v1.2`, `ghcr.io/owner/repo:v1`
- Commit SHA: `ghcr.io/owner/repo:branch-sha123456`

### 2. `fast-build.yml` - Ultra-Fast Development
**Triggers:**
- Manual workflow dispatch
- Push to `develop` or `feature/*` branches
- Only when source code changes

**Features:**
- ⚡ **Ultra-fast builds**: Single platform, aggressive caching
- 🎯 **Development focused**: Optimized for quick iteration
- 🚀 **Optional push**: Can build without publishing
- 📦 **Lightweight tags**: Development-specific tagging

**Use Cases:**
- Quick validation of changes
- Development branch testing
- Fast feedback loops

### 3. `release.yml` - Release Management
**Triggers:**
- GitHub release published
- Manual workflow dispatch with tag input

**Actions:**
- Builds and publishes release Docker images
- Tags with semantic versioning
- Updates release notes with Docker pull commands
- Generates build attestations

**Image Tags:**
- `ghcr.io/owner/repo:v1.2.3` (exact version)
- `ghcr.io/owner/repo:v1.2` (minor version)
- `ghcr.io/owner/repo:v1` (major version)
- `ghcr.io/owner/repo:latest` (latest release)

## 🚀 Build Time Optimization Guide

### Performance Improvements Implemented

1. **Smart Path Filtering**
   - Skips builds for documentation changes (`*.md`, `docs/**`)
   - Only triggers on actual code changes

2. **Conditional Build Strategy**
   - **PR builds**: Single platform, validation only
   - **Development**: Single platform, faster iteration
   - **Releases**: Multi-platform, full compatibility

3. **Enhanced Caching**
   - GitHub Actions cache with workflow scope
   - Registry-based cache for build artifacts
   - Multi-layer cache strategy

4. **Build Monitoring**
   - Automatic performance reporting
   - Build time tracking and categorization
   - Performance status indicators

### Expected Build Times

| Build Type | Platforms | Expected Time | Cache Status |
|------------|-----------|---------------|-------------|
| PR Validation | linux/amd64 | 2-5 minutes | Cold: 8-12 min |
| Development | linux/amd64 | 3-6 minutes | Cold: 10-15 min |
| Release | Multi-platform | 8-15 minutes | Cold: 20-30 min |
| Fast Build | linux/amd64 | 1-3 minutes | Cold: 5-8 min |

### Performance Tips

1. **Use Fast Build for Development**
   ```bash
   # Trigger fast build manually
   gh workflow run fast-build.yml
   ```

2. **Optimize Dockerfile**
   - Multi-stage builds are already implemented
   - Dependencies cached separately from source code
   - Minimal final image size

3. **Branch Strategy**
   - Use `develop` or `feature/*` branches for fast builds
   - Reserve `main` for stable releases

## Usage Examples

### Pull and Run the Image
```bash
# Pull the latest image
docker pull ghcr.io/owner/repo:latest

# Run the container
docker run -d \
  --name cyberfly-node \
  -p 8080:8080 \
  -e MQTT_BROKER_HOST=localhost \
  -e MQTT_BROKER_PORT=1883 \
  ghcr.io/owner/repo:latest
```

### Development Workflow
```bash
# For development builds (fast)
docker pull ghcr.io/owner/repo:develop

# For specific versions
docker pull ghcr.io/owner/repo:v1.2.3

# For feature branches
docker pull ghcr.io/owner/repo:feature-new-api
```

## Security

- All images are built with attestations for supply chain security
- Images are scanned for vulnerabilities
- Only authenticated users can push to the registry
- Build provenance is recorded and verifiable

## Advanced Caching Strategy

The workflows implement a sophisticated multi-layer caching system:

1. **GitHub Actions Cache**
   - Workflow-scoped cache keys
   - Automatic cache invalidation
   - Cross-job cache sharing

2. **Registry Cache**
   - Build artifact caching
   - Layer-level optimization
   - Persistent across workflow runs

3. **Build Context Optimization**
   - Minimal context transfer
   - Efficient layer ordering
   - Dependency pre-caching

This reduces build times by **60-80%** for subsequent runs.