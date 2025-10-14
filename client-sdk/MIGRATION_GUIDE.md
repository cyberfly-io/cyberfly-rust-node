# SDK Migration Guide: Submit → Store

## Overview

The CyberFly Client SDK has updated its method naming from "submit" to "store" to better reflect database operations. This change makes the API more intuitive for developers working with a database system.

## What Changed?

All data submission methods have been renamed:

| Old Method (Deprecated) | New Method |
|------------------------|-----------|
| `submitString()` | `storeString()` |
| `submitHash()` | `storeHash()` |
| `submitList()` | `storeList()` |
| `submitSet()` | `storeSet()` |
| `submitSortedSet()` | `storeSortedSet()` |
| `submitJSON()` | `storeJSON()` |
| `submitStream()` | `storeStream()` |
| `submitTimeSeries()` | `storeTimeSeries()` |
| `submitGeo()` | `storeGeo()` |
| `submitData()` | `storeData()` |

## Backward Compatibility

**Good news!** All old `submit*` methods still work as deprecated aliases. Your existing code will continue to function without any changes.

```typescript
// Old code - still works but deprecated
await client.submitString('user:123', 'Alice');

// New code - recommended
await client.storeString('user:123', 'Alice');
```

## Migration Steps

### Option 1: Gradual Migration (Recommended)

Keep using your existing code. TypeScript/IDE will show deprecation warnings, allowing you to update methods gradually:

```typescript
// You'll see: @deprecated Use storeString instead
await client.submitString('user:123', 'Alice');
```

### Option 2: Quick Migration

Use find-and-replace in your codebase:

```bash
# Find all usages (example for Unix-like systems)
grep -r "\.submit" src/

# Replace using your editor or:
sed -i 's/\.submitString(/\.storeString(/g' **/*.ts
sed -i 's/\.submitHash(/\.storeHash(/g' **/*.ts
sed -i 's/\.submitJSON(/\.storeJSON(/g' **/*.ts
# ... etc for other methods
```

## Why This Change?

### Better Semantics
- **"Store"** clearly indicates data persistence in a database
- **"Submit"** implies transient form submission or request sending
- More intuitive for developers familiar with database operations

### Industry Standard
```typescript
// Now aligns with database terminology
await client.storeString('key', 'value');    // Like Redis SET
await client.storeHash('key', 'field', 'val'); // Like Redis HSET
await client.storeJSON('key', data);          // Like MongoDB insert
```

## Examples

### Before (Deprecated)
```typescript
import { CyberFlyClient, CryptoUtils } from '@cyberfly/client-sdk';

const keyPair = await CryptoUtils.generateKeyPair();
const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/graphql',
  keyPair,
  defaultDbName: 'mydb',
});

// Old submit methods (still work)
await client.submitString('user:123', 'Alice');
await client.submitHash('user:123', 'name', 'Alice');
await client.submitJSON('profile:123', { name: 'Alice', age: 30 });
```

### After (Recommended)
```typescript
import { CyberFlyClient, CryptoUtils } from '@cyberfly/client-sdk';

const keyPair = await CryptoUtils.generateKeyPair();
const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/graphql',
  keyPair,
  defaultDbName: 'mydb',
});

// New store methods
await client.storeString('user:123', 'Alice');
await client.storeHash('user:123', 'name', 'Alice');
await client.storeJSON('profile:123', { name: 'Alice', age: 30 });
```

**Note:** Query methods remain unchanged (`queryString`, `queryHash`, etc.)

## Timeline

- **Current Version**: Both APIs work (submit methods deprecated)
- **Next Major Version (2.0)**: Submit methods may be removed
- **Recommended Action**: Start using store methods in new code

## Questions?

See the updated documentation:
- [README.md](./README.md) - Complete API reference
- [QUICKSTART.md](./QUICKSTART.md) - Quick start guide
- [examples/](./examples/) - Working examples

All examples have been updated to use the new store methods.
