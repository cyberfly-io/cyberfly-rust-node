import { GraphQLClient, gql } from 'graphql-request';
import { createClient, Client as GraphQLWSClient, Sink } from 'graphql-ws';
import { WebSocket } from 'ws';
import { CryptoUtils, KeyPair } from './crypto';

/**
 * Configuration for CyberFly client
 */
export interface CyberFlyConfig {
  endpoint: string;
  wsEndpoint?: string;
  keyPair?: KeyPair;
  defaultDbName?: string;
}

/**
 * Store types supported by the database
 */
export type StoreType = 
  | 'String' 
  | 'Hash' 
  | 'List' 
  | 'Set' 
  | 'SortedSet' 
  | 'JSON' 
  | 'Stream' 
  | 'TimeSeries' 
  | 'Geo';

/**
 * Data submission input
 */
export interface DataInput {
  dbName: string;
  key: string;
  value: string;
  storeType: StoreType;
  field?: string;
  score?: number;
  jsonPath?: string;
  streamFields?: string;
  timestamp?: string;
  longitude?: number;
  latitude?: number;
}

/**
 * Signed data for submission (matches GraphQL SignedData type)
 * Note: GraphQL uses camelCase, but Rust uses snake_case internally
 */
export interface SignedData {
  dbName: string;
  key: string;
  value: string;
  publicKey: string;
  signature: string;
  storeType: string;
  field?: string;
  score?: number;
  jsonPath?: string;
  streamFields?: string;
  timestamp?: string;
  longitude?: number;
  latitude?: number;
}

/**
 * Query filter options
 */
export interface FilterOptions {
  pattern?: string;
  minScore?: number;
  maxScore?: number;
  startTime?: string;
  endTime?: string;
  latitude?: number;
  longitude?: number;
  radius?: number;
  unit?: 'km' | 'm' | 'mi' | 'ft';
}

/**
 * Message update from subscription
 */
export interface MessageUpdate {
  topic: string;
  payload: string;
  timestamp: string;
}

/**
 * Blob operation from persistent storage
 */
export interface BlobOperation {
  opId: string;
  timestamp: string;
  dbName: string;
  key: string;
  value: string;
  storeType: string;
  field?: string | null;
  score?: number | null;
  jsonPath?: string | null;
  streamFields?: string | null;
  tsTimestamp?: string | null;
  longitude?: number | null;
  latitude?: number | null;
  publicKey: string;
  signature: string;
}

/**
 * Subscription callback function
 */
export type SubscriptionCallback = (message: MessageUpdate) => void;

/**
 * Subscription error callback
 */
export type SubscriptionErrorCallback = (error: Error) => void;

/**
 * CyberFly client for interacting with the decentralized database
 */
export class CyberFlyClient {
  private client: GraphQLClient;
  private wsClient?: GraphQLWSClient;
  private wsEndpoint?: string;
  private keyPair?: KeyPair;
  private defaultDbName?: string;
  private activeSubscriptions: Map<string, () => void> = new Map();

  constructor(config: CyberFlyConfig) {
    this.client = new GraphQLClient(config.endpoint);
    this.keyPair = config.keyPair;
    this.defaultDbName = config.defaultDbName;
    
    // Set WebSocket endpoint (default to replacing http with ws in main endpoint)
    if (config.wsEndpoint) {
      this.wsEndpoint = config.wsEndpoint;
    } else {
      this.wsEndpoint = config.endpoint
        .replace('http://', 'ws://')
        .replace('https://', 'wss://')
        .replace(/\/$/, '') + '/ws';
    }
  }

  /**
   * Set the key pair for signing
   */
  setKeyPair(keyPair: KeyPair) {
    this.keyPair = keyPair;
  }

  /**
   * Set the default database name
   */
  setDefaultDbName(dbName: string) {
    this.defaultDbName = dbName;
  }

  /**
   * Get the full database name with public key
   */
  getFullDbName(dbName?: string): string {
    if (!this.keyPair) {
      throw new Error('Key pair not set. Call setKeyPair() first.');
    }
    
    const name = dbName || this.defaultDbName;
    if (!name) {
      throw new Error('Database name not provided and no default set.');
    }
    
    return CryptoUtils.createDbName(name, this.keyPair.publicKey);
  }

  /**
   * Sign data for submission
   */
  private async signData(data: DataInput): Promise<SignedData> {
    if (!this.keyPair) {
      throw new Error('Key pair not set. Call setKeyPair() first.');
    }

    // Create message to sign: db_name:key:value
    const message = `${data.dbName}:${data.key}:${data.value}`;
    
    // Sign the message
    const signature = await CryptoUtils.sign(message, this.keyPair.privateKey);
    const publicKey = CryptoUtils.bytesToHex(this.keyPair.publicKey);

    // Return with camelCase for GraphQL
    return {
      dbName: data.dbName,
      key: data.key,
      value: data.value,
      publicKey: publicKey,
      signature,
      storeType: data.storeType,
      field: data.field,
      score: data.score,
      jsonPath: data.jsonPath,
      streamFields: data.streamFields,
      timestamp: data.timestamp,
      longitude: data.longitude,
      latitude: data.latitude,
    };
  }

  /**
   * Store string data
   */
  async storeString(key: string, value: string, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value,
      storeType: 'String',
    };

    return this.storeData(data);
  }

  /**
   * Store hash data
   */
  async storeHash(key: string, field: string, value: string, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value,
      storeType: 'Hash',
      field,
    };

    return this.storeData(data);
  }

  /**
   * Store list data
   */
  async storeList(key: string, value: string, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value,
      storeType: 'List',
    };

    return this.storeData(data);
  }

  /**
   * Store set data
   */
  async storeSet(key: string, value: string, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value,
      storeType: 'Set',
    };

    return this.storeData(data);
  }

  /**
   * Store sorted set data
   */
  async storeSortedSet(key: string, value: string, score: number, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value,
      storeType: 'SortedSet',
      score,
    };

    return this.storeData(data);
  }

  /**
   * Store JSON data
   */
  async storeJSON(key: string, value: object, jsonPath?: string, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value: JSON.stringify(value),
      storeType: 'JSON',
      jsonPath,
    };

    return this.storeData(data);
  }

  /**
   * Store stream data
   */
  async storeStream(key: string, fields: Record<string, any>, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value: 'stream',
      storeType: 'Stream',
      streamFields: JSON.stringify(fields),
    };

    return this.storeData(data);
  }

  /**
   * Store time series data
   */
  async storeTimeSeries(key: string, value: number, timestamp?: string, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    // Convert ISO 8601 timestamp to Unix timestamp (seconds) if provided as ISO string
    let unixTimestamp: string;
    if (timestamp) {
      // Check if it's already a Unix timestamp (numeric string) or ISO 8601
      if (/^\d+$/.test(timestamp)) {
        unixTimestamp = timestamp;
      } else {
        // Convert ISO 8601 to Unix timestamp in seconds
        unixTimestamp = Math.floor(new Date(timestamp).getTime() / 1000).toString();
      }
    } else {
      // Use current time as Unix timestamp in seconds
      unixTimestamp = Math.floor(Date.now() / 1000).toString();
    }
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value: value.toString(),
      storeType: 'TimeSeries',
      timestamp: unixTimestamp,
    };

    return this.storeData(data);
  }

  /**
   * Store geospatial data
   */
  async storeGeo(key: string, member: string, longitude: number, latitude: number, dbName?: string): Promise<boolean> {
    const fullDbName = this.getFullDbName(dbName);
    
    const data: DataInput = {
      dbName: fullDbName,
      key,
      value: member,
      storeType: 'Geo',
      longitude,
      latitude,
    };

    return this.storeData(data);
  }

  /**
   * Store raw data with signature
   */
  async storeData(data: DataInput): Promise<boolean> {
    const signedData = await this.signData(data);

    const mutation = gql`
      mutation SubmitData($input: SignedData!) {
        submitData(input: $input) {
          success
          message
        }
      }
    `;

    const variables = {
      input: signedData,
    };

    const result = await this.client.request<{ submitData: { success: boolean; message: string } }>(mutation, variables);
    return result.submitData.success;
  }

  // Legacy aliases for backward compatibility
  /** @deprecated Use storeString instead */
  async submitString(key: string, value: string, dbName?: string): Promise<boolean> {
    return this.storeString(key, value, dbName);
  }

  /** @deprecated Use storeHash instead */
  async submitHash(key: string, field: string, value: string, dbName?: string): Promise<boolean> {
    return this.storeHash(key, field, value, dbName);
  }

  /** @deprecated Use storeList instead */
  async submitList(key: string, value: string, dbName?: string): Promise<boolean> {
    return this.storeList(key, value, dbName);
  }

  /** @deprecated Use storeSet instead */
  async submitSet(key: string, value: string, dbName?: string): Promise<boolean> {
    return this.storeSet(key, value, dbName);
  }

  /** @deprecated Use storeSortedSet instead */
  async submitSortedSet(key: string, value: string, score: number, dbName?: string): Promise<boolean> {
    return this.storeSortedSet(key, value, score, dbName);
  }

  /** @deprecated Use storeJSON instead */
  async submitJSON(key: string, value: object, jsonPath?: string, dbName?: string): Promise<boolean> {
    return this.storeJSON(key, value, jsonPath, dbName);
  }

  /** @deprecated Use storeStream instead */
  async submitStream(key: string, fields: Record<string, any>, dbName?: string): Promise<boolean> {
    return this.storeStream(key, fields, dbName);
  }

  /** @deprecated Use storeTimeSeries instead */
  async submitTimeSeries(key: string, value: number, timestamp?: string, dbName?: string): Promise<boolean> {
    return this.storeTimeSeries(key, value, timestamp, dbName);
  }

  /** @deprecated Use storeGeo instead */
  async submitGeo(key: string, member: string, longitude: number, latitude: number, dbName?: string): Promise<boolean> {
    return this.storeGeo(key, member, longitude, latitude, dbName);
  }

  /** @deprecated Use storeData instead */
  async submitData(data: DataInput): Promise<boolean> {
    return this.storeData(data);
  }

  /**
   * Query string data
   */
  async queryString(key: string, dbName?: string): Promise<string | null> {
    const fullDbName = this.getFullDbName(dbName);

    const query = gql`
      query GetString($dbName: String!, $key: String!) {
        getString(dbName: $dbName, key: $key) {
          key
          value
        }
      }
    `;

    const variables = {
      dbName: fullDbName,
      key,
    };

    const result = await this.client.request<{ getString: { key: string; value: string | null } }>(query, variables);
    return result.getString.value;
  }

  /**
   * Query hash data
   */
  async queryHash(key: string, field?: string, filter?: FilterOptions, dbName?: string): Promise<Record<string, string>> {
    const fullDbName = this.getFullDbName(dbName);

    if (field) {
      // Query specific field
      const query = gql`
        query GetHash($dbName: String!, $key: String!, $field: String!) {
          getHash(dbName: $dbName, key: $key, field: $field) {
            key
            value
          }
        }
      `;

      const variables = { dbName: fullDbName, key, field };
      const result = await this.client.request<{ getHash: { key: string; value: string | null } }>(query, variables);
      return result.getHash.value ? { [field]: result.getHash.value } : {};
    } else {
      // Query all fields
      const query = gql`
        query GetAllHash($dbName: String!, $key: String!) {
          getAllHash(dbName: $dbName, key: $key) {
            key
            value
          }
        }
      `;

      const variables = { dbName: fullDbName, key };
      const result = await this.client.request<{ getAllHash: Array<{ key: string; value: string | null }> }>(query, variables);
      
      // Convert array to object, extracting field name from key
      const hashObj: Record<string, string> = {};
      for (const item of result.getAllHash) {
        // Key format is "dbName:key:field", extract the field part
        const fieldName = item.key.split(':').pop() || '';
        if (item.value) {
          hashObj[fieldName] = item.value;
        }
      }
      return hashObj;
    }
  }

  /**
   * Query list data
   */
  async queryList(key: string, filter?: FilterOptions, dbName?: string): Promise<string[]> {
    const fullDbName = this.getFullDbName(dbName);

    const query = gql`
      query GetList($dbName: String!, $key: String!, $start: Int, $stop: Int) {
        getList(dbName: $dbName, key: $key, start: $start, stop: $stop)
      }
    `;

    const variables = {
      dbName: fullDbName,
      key,
      start: 0,
      stop: -1,
    };

    const result = await this.client.request<{ getList: string[] }>(query, variables);
    return result.getList;
  }

  /**
   * Query set data
   */
  async querySet(key: string, filter?: FilterOptions, dbName?: string): Promise<string[]> {
    const fullDbName = this.getFullDbName(dbName);

    const query = gql`
      query GetSet($dbName: String!, $key: String!) {
        getSet(dbName: $dbName, key: $key)
      }
    `;

    const variables = {
      dbName: fullDbName,
      key,
    };

    const result = await this.client.request<{ getSet: string[] }>(query, variables);
    return result.getSet;
  }

  /**
   * Query sorted set data
   */
  async querySortedSet(key: string, filter?: FilterOptions, dbName?: string): Promise<Array<{ value: string; score: number }>> {
    const fullDbName = this.getFullDbName(dbName);

    const query = gql`
      query GetSortedSet($dbName: String!, $key: String!, $start: Int, $stop: Int) {
        getSortedSet(dbName: $dbName, key: $key, start: $start, stop: $stop) {
          value
          score
        }
      }
    `;

    const variables = {
      dbName: fullDbName,
      key,
      start: 0,
      stop: -1,
    };

    const result = await this.client.request<{ getSortedSet: Array<{ value: string; score: number }> }>(query, variables);
    return result.getSortedSet;
  }

  /**
   * Query JSON data
   */
  async queryJSON(key: string, jsonPath?: string, filter?: FilterOptions, dbName?: string): Promise<any> {
    const fullDbName = this.getFullDbName(dbName);

    const query = gql`
      query GetJson($dbName: String!, $key: String!, $path: String) {
        getJson(dbName: $dbName, key: $key, path: $path) {
          key
          value
        }
      }
    `;

    const variables = {
      dbName: fullDbName,
      key,
      path: jsonPath || "$",
    };

    const result = await this.client.request<{ getJson: { key: string; value: string | null } }>(query, variables);
    return result.getJson.value ? JSON.parse(result.getJson.value) : null;
  }

  /**
   * Query stream data
   */
  async queryStream(key: string, filter?: FilterOptions, dbName?: string): Promise<any[]> {
    const fullDbName = this.getFullDbName(dbName);

    const query = gql`
      query GetStream($dbName: String!, $key: String!, $start: String, $end: String, $count: Int) {
        getStream(dbName: $dbName, key: $key, start: $start, end: $end, count: $count) {
          id
          fields {
            key
            value
          }
        }
      }
    `;

    const variables = {
      dbName: fullDbName,
      key,
      start: "-",
      end: "+",
      count: null,
    };

    const result = await this.client.request<{ getStream: Array<{ id: string; fields: Array<{ key: string; value: string }> }> }>(query, variables);
    return result.getStream;
  }

  /**
   * Query time series data
   */
  async queryTimeSeries(key: string, filter?: FilterOptions, dbName?: string): Promise<Array<{ timestamp: string; value: number }>> {
    const fullDbName = this.getFullDbName(dbName);

    // Default to last 24 hours if no filter provided
    // Convert milliseconds to seconds for Unix timestamp
    const toTimestamp = Math.floor(Date.now() / 1000).toString();
    const fromTimestamp = Math.floor((Date.now() - 24 * 60 * 60 * 1000) / 1000).toString();

    const query = gql`
      query GetTimeseries($dbName: String!, $key: String!, $fromTimestamp: String!, $toTimestamp: String!) {
        getTimeseries(dbName: $dbName, key: $key, fromTimestamp: $fromTimestamp, toTimestamp: $toTimestamp) {
          timestamp
          value
        }
      }
    `;

    const variables = {
      dbName: fullDbName,
      key,
      fromTimestamp: filter?.startTime || fromTimestamp,
      toTimestamp: filter?.endTime || toTimestamp,
    };

    const result = await this.client.request<{ getTimeseries: Array<{ timestamp: string; value: number }> }>(query, variables);
    return result.getTimeseries;
  }

  /**
   * Query geospatial data
   */
  async queryGeo(key: string, filter?: FilterOptions, dbName?: string): Promise<Array<{ member: string; longitude: number; latitude: number; distance?: number }>> {
    const fullDbName = this.getFullDbName(dbName);

    if (filter?.longitude !== undefined && filter?.latitude !== undefined && filter?.radius !== undefined) {
      // Search by radius
      const query = gql`
        query SearchGeoRadius($dbName: String!, $key: String!, $longitude: Float!, $latitude: Float!, $radius: Float!, $unit: String) {
          searchGeoRadius(dbName: $dbName, key: $key, longitude: $longitude, latitude: $latitude, radius: $radius, unit: $unit) {
            member
            longitude
            latitude
          }
        }
      `;

      const variables = {
        dbName: fullDbName,
        key,
        longitude: filter.longitude,
        latitude: filter.latitude,
        radius: filter.radius,
        unit: filter.unit || "m",
      };

      const result = await this.client.request<{ searchGeoRadius: Array<{ member: string; longitude: number; latitude: number }> }>(query, variables);
      return result.searchGeoRadius;
    } else {
      // Just return empty array if no filter (no "get all geo" query exists)
      return [];
    }
  }

  /**
   * Initialize WebSocket client if not already initialized
   */
  private ensureWsClient(): GraphQLWSClient {
    if (!this.wsClient) {
      this.wsClient = createClient({
        url: this.wsEndpoint!,
        webSocketImpl: WebSocket,
      });
    }
    return this.wsClient;
  }

  /**
   * Subscribe to messages on a specific topic
   * @param topic - MQTT topic pattern (supports wildcards: + for single level, # for multi-level)
   * @param callback - Function to call when a message is received
   * @param onError - Optional error handler
   * @returns Unsubscribe function
   */
  subscribeToTopic(
    topic: string,
    callback: SubscriptionCallback,
    onError?: SubscriptionErrorCallback
  ): () => void {
    const client = this.ensureWsClient();
    const subscriptionId = `topic:${topic}:${Date.now()}`;

    const query = `
      subscription SubscribeTopic($topicFilter: String!) {
        subscribeTopic(topicFilter: $topicFilter) {
          topic
          payload
          timestamp
        }
      }
    `;

    const unsubscribe = client.subscribe(
      {
        query,
        variables: { topicFilter: topic },
      },
      {
        next: (data: any) => {
          if (data?.data?.subscribeTopic) {
            callback(data.data.subscribeTopic);
          }
        },
        error: (error: any) => {
          if (onError) {
            onError(new Error(error.message || 'Subscription error'));
          }
          this.activeSubscriptions.delete(subscriptionId);
        },
        complete: () => {
          this.activeSubscriptions.delete(subscriptionId);
        },
      }
    );

    // Store the unsubscribe function
    this.activeSubscriptions.set(subscriptionId, unsubscribe);

    // Return unsubscribe function
    return () => {
      unsubscribe();
      this.activeSubscriptions.delete(subscriptionId);
    };
  }

  /**
   * Subscribe to all messages (no topic filter)
   * @param callback - Function to call when a message is received
   * @param onError - Optional error handler
   * @returns Unsubscribe function
   */
  subscribeToMessages(
    callback: SubscriptionCallback,
    onError?: SubscriptionErrorCallback
  ): () => void {
    const client = this.ensureWsClient();
    const subscriptionId = `messages:${Date.now()}`;

    const query = `
      subscription SubscribeAllMessages {
        subscribeAllMessages {
          topic
          payload
          timestamp
        }
      }
    `;

    const unsubscribe = client.subscribe(
      {
        query,
      },
      {
        next: (data: any) => {
          if (data?.data?.subscribeAllMessages) {
            callback(data.data.subscribeAllMessages);
          }
        },
        error: (error: any) => {
          if (onError) {
            onError(new Error(error.message || 'Subscription error'));
          }
          this.activeSubscriptions.delete(subscriptionId);
        },
        complete: () => {
          this.activeSubscriptions.delete(subscriptionId);
        },
      }
    );

    // Store the unsubscribe function
    this.activeSubscriptions.set(subscriptionId, unsubscribe);

    // Return unsubscribe function
    return () => {
      unsubscribe();
      this.activeSubscriptions.delete(subscriptionId);
    };
  }

  /**
   * Unsubscribe from all active subscriptions
   */
  unsubscribeAll(): void {
    for (const unsubscribe of this.activeSubscriptions.values()) {
      unsubscribe();
    }
    this.activeSubscriptions.clear();
  }

  /**
   * Close WebSocket connection and cleanup
   */
  async disconnect(): Promise<void> {
    this.unsubscribeAll();
    if (this.wsClient) {
      await this.wsClient.dispose();
      this.wsClient = undefined;
    }
  }

  // ============ Blob Operation Queries ============

  /**
   * Get blob operations for a specific database
   */
  async getBlobOperations(dbName: string, limit?: number): Promise<BlobOperation[]> {
    const query = gql`
      query GetBlobOperations($dbName: String!, $limit: Int) {
        getBlobOperations(dbName: $dbName, limit: $limit) {
          opId
          timestamp
          dbName
          key
          value
          storeType
          field
          score
          jsonPath
          streamFields
          tsTimestamp
          longitude
          latitude
          publicKey
          signature
        }
      }
    `;

    const data = await this.client.request<{ getBlobOperations: BlobOperation[] }>(query, {
      dbName,
      limit,
    });

    return data.getBlobOperations;
  }

  /**
   * Get all blob operations across all databases
   */
  async getAllBlobOperations(limit?: number): Promise<BlobOperation[]> {
    const query = gql`
      query GetAllBlobOperations($limit: Int) {
        getAllBlobOperations(limit: $limit) {
          opId
          timestamp
          dbName
          key
          value
          storeType
          field
          score
          jsonPath
          streamFields
          tsTimestamp
          longitude
          latitude
          publicKey
          signature
        }
      }
    `;

    const data = await this.client.request<{ getAllBlobOperations: BlobOperation[] }>(query, {
      limit,
    });

    return data.getAllBlobOperations;
  }

  /**
   * Get blob operations since a specific timestamp
   */
  async getBlobOperationsSince(
    dbName: string,
    timestamp: number | string,
    limit?: number
  ): Promise<BlobOperation[]> {
    const query = gql`
      query GetBlobOperationsSince($dbName: String!, $timestamp: String!, $limit: Int) {
        getBlobOperationsSince(dbName: $dbName, timestamp: $timestamp, limit: $limit) {
          opId
          timestamp
          dbName
          key
          value
          storeType
          field
          score
          jsonPath
          streamFields
          tsTimestamp
          longitude
          latitude
          publicKey
          signature
        }
      }
    `;

    const data = await this.client.request<{ getBlobOperationsSince: BlobOperation[] }>(query, {
      dbName,
      timestamp: timestamp.toString(),
      limit,
    });

    return data.getBlobOperationsSince;
  }

  /**
   * Get count of blob operations for a database
   */
  async getBlobOperationCount(dbName?: string): Promise<number> {
    const query = gql`
      query GetBlobOperationCount($dbName: String) {
        getBlobOperationCount(dbName: $dbName)
      }
    `;

    const data = await this.client.request<{ getBlobOperationCount: number }>(query, {
      dbName,
    });

    return data.getBlobOperationCount;
  }
}
