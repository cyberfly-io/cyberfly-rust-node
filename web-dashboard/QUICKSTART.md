# CyberFly Web Dashboard - Quick Start

## Prerequisites

1. **Start the Rust backend** (in the main project directory):
   ```bash
   cargo run --release
   ```
   The backend should be running on `http://localhost:3000`

2. **Verify backend is running**:
   ```bash
   curl http://localhost:3000/graphql
   ```

## Starting the Dashboard

```bash
cd web-dashboard
npm run dev
```

The dashboard will open at `http://localhost:5173`

## Features to Test

### 1. Dashboard (/)
- View real-time connected peers
- Monitor network stats (messages/sec, bandwidth)
- Check storage statistics (keys, cache hit rate)
- See node information (Peer ID, version, uptime)

### 2. Submit Data (/submit)
Test all data types:
- **String**: Simple key-value
- **Hash**: Nested key-value pairs
- **List**: Ordered collections
- **Set**: Unique items
- **SortedSet**: Scored data for rankings
- **JSON**: Arbitrary JSON documents
- **Stream**: Event streams
- **TimeSeries**: Time-stamped metrics
- **Geo**: Location data

**Note**: You need to generate Ed25519 signatures. Use the client SDK:
```typescript
import { CyberFlyClient } from 'cyberfly-client-sdk';
const client = new CyberFlyClient('http://localhost:3000');
const { signature, publicKey } = client.signData({ key: 'value' });
```

### 3. Query Data (/query)
- Get all data with optional store type filter
- Filter by key pattern (e.g., `sensor_*`)
- Set limits and pagination
- Expand entries to see full data and metadata

### 4. Blob Storage (/blobs)
- **Upload**: Select any file and upload to Iroh
- **Download**: Enter a blob hash to download
- Content-addressed storage with automatic deduplication
- P2P distribution across connected nodes

## Troubleshooting

### Backend Not Connected
- Error: "Failed to fetch" or "Network Error"
- Solution: Make sure Rust backend is running on port 3000

### CORS Errors
- The Rust backend should have CORS enabled
- Check `src/graphql.rs` for CORS configuration

### No Peers Showing
- Make sure at least one other node is connected
- Check network discovery is working: `cargo run -- --help`
- Connect to bootstrap peers if configured

### GraphQL Errors
- Check the schema matches: compare `schema.graphql` with API client
- View browser console for detailed error messages
- Use GraphQL playground at `http://localhost:3000/graphql`

## Development

### Hot Reload
The dashboard uses Vite with HMR (Hot Module Replacement). Changes are reflected instantly.

### Adding New Features
1. Create component in `src/components/`
2. Add API methods in `src/api/client.ts`
3. Add navigation item in `App.tsx`

### Building for Production
```bash
npm run build
npm run preview
```

## Architecture

```
web-dashboard/
├── src/
│   ├── api/
│   │   └── client.ts          # API client (GraphQL + REST)
│   ├── components/
│   │   ├── Dashboard.tsx      # Main dashboard with peer list
│   │   ├── DataSubmit.tsx     # Data submission forms
│   │   ├── DataQuery.tsx      # Query interface
│   │   └── BlobManager.tsx    # Blob upload/download
│   ├── App.tsx                # Main app with routing
│   └── main.tsx               # Entry point
├── .env                       # API configuration
└── package.json
```

## Next Steps

1. **Add Authentication**: Implement Ed25519 key management in the UI
2. **Real-time Updates**: Add WebSocket support for live data streams
3. **Metrics Visualization**: Add charts for Prometheus metrics
4. **Peer Management**: Add manual peer connection/disconnection
5. **Data Import/Export**: Bulk data operations

## API Endpoints Reference

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/graphql` | POST | GraphQL API for all queries/mutations |
| `/blobs/upload` | POST | Upload blob (multipart/form-data) |
| `/blobs/:hash` | GET | Download blob by hash |
| `/metrics` | GET | Prometheus metrics |

## Environment Variables

Create `.env.local` to override:

```env
VITE_API_URL=http://localhost:3000
```
