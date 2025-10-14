/**
 * Test script for node information queries
 * 
 * This example demonstrates:
 * 1. Getting node information (health, peer counts)
 * 2. Listing connected peers
 * 3. Listing discovered peers
 * 4. Network health monitoring
 */

import { GraphQLClient } from 'graphql-request';

const endpoint = 'http://localhost:8080';
const client = new GraphQLClient(endpoint);

interface NodeInfo {
  nodeId: string;
  peerId: string;
  health: string;
  connectedPeers: number;
  discoveredPeers: number;
  uptimeSeconds: number;
  relayUrl?: string;
}

interface PeerInfo {
  peerId: string;
  connectionStatus: string;
  lastSeen: string;
}

interface NodeInfoResponse {
  getNodeInfo: NodeInfo;
}

interface ConnectedPeersResponse {
  getConnectedPeers: PeerInfo[];
}

interface DiscoveredPeersResponse {
  getDiscoveredPeers: PeerInfo[];
}

async function testNodeInfo() {
  console.log('=== Testing Node Information Queries ===\n');

  try {
    // 1. Get node information
    console.log('1. Getting node information...');
    const nodeInfoQuery = `
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

    const nodeInfoResult = await client.request<NodeInfoResponse>(nodeInfoQuery);
    const nodeInfo = nodeInfoResult.getNodeInfo;
    
    console.log('Node Information:');
    console.log(`  Node ID: ${nodeInfo.nodeId}`);
    console.log(`  Peer ID: ${nodeInfo.peerId}`);
    console.log(`  Health: ${nodeInfo.health}`);
    console.log(`  Connected Peers: ${nodeInfo.connectedPeers}`);
    console.log(`  Discovered Peers: ${nodeInfo.discoveredPeers}`);
    console.log(`  Uptime: ${nodeInfo.uptimeSeconds} seconds`);
    if (nodeInfo.relayUrl) {
      console.log(`  Relay URL: ${nodeInfo.relayUrl}`);
    }
    console.log();

    // Interpret health status
    console.log('Health Status Analysis:');
    switch (nodeInfo.health) {
      case 'healthy':
        console.log('  ✅ Node is healthy with active peer connections');
        break;
      case 'discovering':
        console.log('  ⚠️  Node found peers but not connected yet');
        break;
      case 'isolated':
        console.log('  ❌ Node is isolated - no peer connections or discoveries');
        break;
      default:
        console.log(`  ❓ Unknown health status: ${nodeInfo.health}`);
    }
    console.log();

    // 2. Get connected peers
    console.log('2. Getting connected peers...');
    const connectedPeersQuery = `
      query {
        getConnectedPeers {
          peerId
          connectionStatus
          lastSeen
        }
      }
    `;

    const connectedResult = await client.request<ConnectedPeersResponse>(connectedPeersQuery);
    const connectedPeers = connectedResult.getConnectedPeers;
    
    console.log(`Found ${connectedPeers.length} connected peer(s):`);
    if (connectedPeers.length > 0) {
      connectedPeers.forEach((peer, index) => {
        console.log(`  ${index + 1}. Peer: ${peer.peerId.substring(0, 16)}...`);
        console.log(`     Status: ${peer.connectionStatus}`);
        console.log(`     Last Seen: ${peer.lastSeen}`);
      });
    } else {
      console.log('  (No connected peers)');
    }
    console.log();

    // 3. Get discovered peers
    console.log('3. Getting discovered peers...');
    const discoveredPeersQuery = `
      query {
        getDiscoveredPeers {
          peerId
          connectionStatus
          lastSeen
        }
      }
    `;

    const discoveredResult = await client.request<DiscoveredPeersResponse>(discoveredPeersQuery);
    const discoveredPeers = discoveredResult.getDiscoveredPeers;
    
    console.log(`Found ${discoveredPeers.length} discovered peer(s):`);
    if (discoveredPeers.length > 0) {
      discoveredPeers.forEach((peer, index) => {
        console.log(`  ${index + 1}. Peer: ${peer.peerId.substring(0, 16)}...`);
        console.log(`     Status: ${peer.connectionStatus}`);
        console.log(`     Last Seen: ${peer.lastSeen}`);
      });
    } else {
      console.log('  (No discovered peers)');
    }
    console.log();

    // 4. Combined query (more efficient)
    console.log('4. Getting all information in one query...');
    const combinedQuery = `
      query {
        getNodeInfo {
          nodeId
          health
          connectedPeers
          discoveredPeers
        }
        getConnectedPeers {
          peerId
          connectionStatus
        }
        getDiscoveredPeers {
          peerId
          connectionStatus
        }
      }
    `;

    interface CombinedResponse {
      getNodeInfo: NodeInfo;
      getConnectedPeers: PeerInfo[];
      getDiscoveredPeers: PeerInfo[];
    }

    const combinedResult = await client.request<CombinedResponse>(combinedQuery);
    
    console.log('Combined Query Results:');
    console.log(`  Node Health: ${combinedResult.getNodeInfo.health}`);
    console.log(`  Connected Peers: ${combinedResult.getConnectedPeers.length}`);
    console.log(`  Discovered Peers: ${combinedResult.getDiscoveredPeers.length}`);
    console.log();

    // 5. Calculate connection rate
    if (discoveredPeers.length > 0) {
      const connectionRate = (connectedPeers.length / discoveredPeers.length) * 100;
      console.log('Network Statistics:');
      console.log(`  Connection Rate: ${connectionRate.toFixed(1)}%`);
      console.log(`  (${connectedPeers.length} connected out of ${discoveredPeers.length} discovered)`);
    } else {
      console.log('Network Statistics:');
      console.log('  No peers discovered yet');
    }

  } catch (error) {
    console.error('Error:', error);
    if (error instanceof Error) {
      console.error('Message:', error.message);
    }
  }
}

// Monitoring function that runs periodically
async function monitorNetwork(intervalSeconds: number = 30) {
  console.log(`\n=== Starting Network Monitor (checking every ${intervalSeconds}s) ===`);
  console.log('Press Ctrl+C to stop\n');

  while (true) {
    const timestamp = new Date().toISOString();
    console.log(`[${timestamp}] Checking network status...`);

    try {
      const query = `
        query {
          getNodeInfo {
            health
            connectedPeers
            discoveredPeers
            uptimeSeconds
          }
        }
      `;

      const result = await client.request<NodeInfoResponse>(query);
      const info = result.getNodeInfo;

      const healthEmoji = info.health === 'healthy' ? '✅' : 
                         info.health === 'discovering' ? '⚠️' : '❌';

      console.log(`  ${healthEmoji} Health: ${info.health}`);
      console.log(`  👥 Connected: ${info.connectedPeers}, Discovered: ${info.discoveredPeers}`);
      console.log(`  ⏱️  Uptime: ${info.uptimeSeconds}s`);
      console.log();

      // Alert on issues
      if (info.health === 'isolated') {
        console.error('  🚨 ALERT: Node is isolated!');
      } else if (info.connectedPeers < 3 && info.discoveredPeers > 0) {
        console.warn('  ⚠️  WARNING: Low peer count');
      }

    } catch (error) {
      console.error('  ❌ Error fetching node info:', error);
    }

    // Wait for next check
    await new Promise(resolve => setTimeout(resolve, intervalSeconds * 1000));
  }
}

// Main execution
const args = process.argv.slice(2);

if (args.includes('--monitor')) {
  const intervalIndex = args.indexOf('--interval');
  const interval = intervalIndex >= 0 ? parseInt(args[intervalIndex + 1]) : 30;
  monitorNetwork(interval).catch(console.error);
} else {
  testNodeInfo().catch(console.error);
}
