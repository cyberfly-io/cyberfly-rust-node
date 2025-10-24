# GitHub Actions Workflows

This directory contains GitHub Actions workflows for automated CI/CD processes.

## Workflows

### 1. `docker-publish.yml` - Continuous Integration
**Triggers:**
- Push to `main` or `master` branches
- Push of version tags (`v*`)
- Pull requests to `main` or `master`

**Actions:**
- Builds Docker image using multi-platform support (linux/amd64, linux/arm64)
- Publishes to GitHub Container Registry (`ghcr.io`)
- Uses GitHub Actions cache for faster builds
- Generates build attestations for security

**Image Tags:**
- Branch pushes: `ghcr.io/owner/repo:branch-name`
- PR builds: `ghcr.io/owner/repo:pr-123`
- Version tags: `ghcr.io/owner/repo:v1.2.3`, `ghcr.io/owner/repo:v1.2`, `ghcr.io/owner/repo:v1`
- Commit SHA: `ghcr.io/owner/repo:branch-sha123456`

### 2. `release.yml` - Release Management
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

## Usage

### Pulling Images

```bash
# Latest release
docker pull ghcr.io/owner/cyberfly-rust-node:latest

# Specific version
docker pull ghcr.io/owner/cyberfly-rust-node:v1.0.0

# Development branch
docker pull ghcr.io/owner/cyberfly-rust-node:main
```

### Running the Container

```bash
docker run -d --name cyberfly-node \
  -p 8080:8080 \
  -p 1883:1883 \
  -p 11204:11204 \
  -p 3478:3478 \
  -e MQTT_BROKER_HOST=localhost \
  -e API_HOST=0.0.0.0 \
  ghcr.io/owner/cyberfly-rust-node:latest
```

## Security

- Images are signed with build attestations
- Only repository collaborators can trigger builds
- Uses GitHub's built-in `GITHUB_TOKEN` for authentication
- Multi-platform builds ensure compatibility

## Cache Strategy

- Uses GitHub Actions cache (`type=gha`) for faster builds
- Cache is shared between workflow runs
- Automatically manages cache lifecycle