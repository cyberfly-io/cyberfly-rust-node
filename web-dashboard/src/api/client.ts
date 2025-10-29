import axios from 'axios';

// Get API URL from localStorage or env
function getApiBaseUrl(): string {
  const stored = localStorage.getItem('cyberfly_api_url');
  return stored || import.meta.env.VITE_API_URL || 'http://localhost:8080';
}

const API_BASE_URL = getApiBaseUrl();
const GRAPHQL_ENDPOINT = `${API_BASE_URL}/graphql`;

const axiosInstance = axios.create({
  baseURL: API_BASE_URL,
  timeout: 10000,
});

// GraphQL Query Helper
export async function graphqlQuery<T>(query: string, variables?: Record<string, any>): Promise<T> {
  const response = await axiosInstance.post(GRAPHQL_ENDPOINT, {
    query,
    variables,
  });
  
  if (response.data.errors) {
    throw new Error(response.data.errors[0].message);
  }
  
  return response.data.data;
}

// Node Info
export interface NodeInfo {
  nodeId: string;
  peerId: string;
  health: string;
  connectedPeers: number;
  discoveredPeers: number;
  uptimeSeconds: number;
  relayUrl?: string;
}

export async function getNodeInfo(): Promise<NodeInfo> {
  const query = `
    query {
      getNodeInfo {
        nodeId
        peerId
        health
        connectedPeers
        discoveredPeers
        uptimeSeconds
        relayUrl
      }
    }
  `;
  const data = await graphqlQuery<{ getNodeInfo: NodeInfo }>(query);
  return data.getNodeInfo;
}

// Connected Peers
export interface Peer {
  peerId: string;
  connectionStatus: string;
  lastSeen: string;
}

export async function getConnectedPeers(): Promise<Peer[]> {
  const query = `
    query {
      getConnectedPeers {
        peerId
        connectionStatus
        lastSeen
      }
    }
  `;
  const data = await graphqlQuery<{ getConnectedPeers: Peer[] }>(query);
  return data.getConnectedPeers;
}

// Discovered Peers
export async function getDiscoveredPeers(): Promise<Peer[]> {
  const query = `
    query {
      getDiscoveredPeers {
        peerId
        connectionStatus
        lastSeen
      }
    }
  `;
  const data = await graphqlQuery<{ getDiscoveredPeers: Peer[] }>(query);
  return data.getDiscoveredPeers;
}

// Submit Data
export interface DataSubmission {
  storeType: 'String' | 'Hash' | 'List' | 'Set' | 'SortedSet' | 'Json' | 'Stream' | 'TimeSeries' | 'Geo';
  key: string;
  value: any;
  publicKey: string;
  signature: string;
  timestamp?: number;
  dbName?: string;
  field?: string;
  score?: number;
  jsonPath?: string;
  streamFields?: string;
  longitude?: number;
  latitude?: number;
}

export async function submitData(data: DataSubmission): Promise<string> {
  // Generate dbName if not provided
  const dbName = data.dbName || `mydb-${data.publicKey}`;
  
  // Convert value to JSON string
  const valueStr = typeof data.value === 'string' ? data.value : JSON.stringify(data.value);
  
  const mutation = `
    mutation($input: SignedData!) {
      submitData(input: $input) {
        success
        message
      }
    }
  `;
  
  const input = {
    dbName,
    key: data.key,
    value: valueStr,
    publicKey: data.publicKey,
    signature: data.signature,
    storeType: data.storeType,
    ...(data.field && { field: data.field }),
    ...(data.score !== undefined && { score: data.score }),
    ...(data.jsonPath && { jsonPath: data.jsonPath }),
    ...(data.streamFields && { streamFields: data.streamFields }),
    ...(data.timestamp && { timestamp: data.timestamp.toString() }),
    ...(data.longitude !== undefined && { longitude: data.longitude }),
    ...(data.latitude !== undefined && { latitude: data.latitude }),
  };
  
  const result = await graphqlQuery<{ submitData: { success: boolean; message: string } }>(mutation, {
    input,
  });
  
  if (!result.submitData.success) {
    throw new Error(result.submitData.message);
  }
  
  return result.submitData.message;
}

// Query Data
export interface QueryFilter {
  storeType?: string;
  keyPattern?: string;
  limit?: number;
  offset?: number;
}

export interface DataEntry {
  key: string;
  storeType: string;
  value: any;
  metadata?: {
    publicKey: string;
    signature: string;
    timestamp: number;
  };
}

// Query Data - Note: This function is deprecated as the schema doesn't support it
// Use getDataByDbName or getDataByDbNameAndType instead
export async function queryData(_filter: QueryFilter): Promise<DataEntry[]> {
  console.warn('queryData is deprecated - use getDataByDbName instead');
  // Return empty array since this query doesn't exist in the schema
  return [];
}

// Get All Data - Note: This function is deprecated as the schema doesn't support it
// Use getDataByDbName or getDataByDbNameAndType instead
export async function getAllData(_storeType?: string, _limit?: number): Promise<DataEntry[]> {
  console.warn('getAllData is deprecated - use getDataByDbName instead');
  // Return empty array since this query doesn't exist in the schema
  return [];
}

// Query by Database Name and Type
export async function getDataByDbNameAndType(
  dbName: string,
  storeType: string
): Promise<DataEntry[]> {
  let query = '';
  let queryName = '';

  switch (storeType.toLowerCase()) {
    case 'string':
      queryName = 'getAllStrings';
      query = `
        query($dbName: String!) {
          getAllStrings(dbName: $dbName) {
            key
            value
            publicKey
            signature
          }
        }
      `;
      break;
    case 'hash':
      queryName = 'getAllHashes';
      query = `
        query($dbName: String!) {
          getAllHashes(dbName: $dbName) {
            key
            fields
            publicKey
            signature
          }
        }
      `;
      break;
    case 'list':
      queryName = 'getAllLists';
      query = `
        query($dbName: String!) {
          getAllLists(dbName: $dbName) {
            key
            items
            publicKey
            signature
          }
        }
      `;
      break;
    case 'set':
      queryName = 'getAllSets';
      query = `
        query($dbName: String!) {
          getAllSets(dbName: $dbName) {
            key
            members
            publicKey
            signature
          }
        }
      `;
      break;
    case 'sortedset':
      queryName = 'getAllSortedSets';
      query = `
        query($dbName: String!) {
          getAllSortedSets(dbName: $dbName) {
            key
            members
            publicKey
            signature
          }
        }
      `;
      break;
    case 'json':
      queryName = 'getAllJsons';
      query = `
        query($dbName: String!) {
          getAllJsons(dbName: $dbName) {
            key
            data
            publicKey
            signature
            timestamp
          }
        }
      `;
      break;
    case 'stream':
      queryName = 'getAllStreams';
      query = `
        query($dbName: String!) {
          getAllStreams(dbName: $dbName) {
            key
            entries
            publicKey
            signature
          }
        }
      `;
      break;
    case 'timeseries':
      queryName = 'getAllTimeseries';
      query = `
        query($dbName: String!) {
          getAllTimeseries(dbName: $dbName) {
            key
            points
            publicKey
            signature
          }
        }
      `;
      break;
    case 'geo':
      queryName = 'getAllGeo';
      query = `
        query($dbName: String!) {
          getAllGeo(dbName: $dbName) {
            key
            locations
            publicKey
            signature
          }
        }
      `;
      break;
    default:
      throw new Error(`Unknown store type: ${storeType}`);
  }

  const result = await graphqlQuery<any>(query, { dbName });
  const rawData = result[queryName];

  // Transform to common format
  return rawData.map((item: any) => ({
    key: item.key.replace(`${dbName}:`, ''), // Remove dbName prefix
    storeType,
    value: item.value || item.fields || item.items || item.members || item.data || item.entries || item.points || item.locations,
    metadata: item.publicKey ? {
      publicKey: item.publicKey,
      signature: item.signature,
      timestamp: item.timestamp || 0,
    } : undefined,
  }));
}

// Get data by dbName (all types)
export async function getDataByDbName(dbName: string): Promise<DataEntry[]> {
  const query = `
    query($dbName: String!) {
      getAll(dbName: $dbName) {
        key
        storeType
        value
        publicKey
        signature
      }
    }
  `;
  
  const result = await graphqlQuery<{ getAll: any[] }>(query, { dbName });
  
  return result.getAll.map((item: any) => ({
    key: item.key.replace(`${dbName}:`, ''), // Remove dbName prefix
    storeType: item.storeType,
    value: item.value,
    metadata: item.publicKey ? {
      publicKey: item.publicKey,
      signature: item.signature,
      timestamp: 0,
    } : undefined,
  }));
}

// Blob Operations
export async function uploadBlob(file: File): Promise<string> {
  const formData = new FormData();
  formData.append('blob', file);
  
  const response = await axiosInstance.post('/blobs/upload', formData, {
    headers: { 'Content-Type': 'multipart/form-data' },
  });
  
  return response.data.hash;
}

export async function downloadBlob(hash: string): Promise<Blob> {
  const response = await axiosInstance.get(`/blobs/${hash}`, {
    responseType: 'blob',
  });
  
  return response.data;
}

// Metrics (Prometheus format)
export async function getMetrics(): Promise<string> {
  const response = await axiosInstance.get('/metrics');
  return response.data;
}

export default axiosInstance;
