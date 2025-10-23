import { CyberFlyClient, CryptoUtils } from '../src';
import { GraphQLClient } from 'graphql-request';

async function testBootstrapNode() {
    console.log('🚀 Testing connection to bootstrap node at 208.73.202.62');
    
    try {
        // Create GraphQL client for direct queries
        const graphqlClient = new GraphQLClient('http://208.73.202.62:8080/graphql');
        
        // Get node info
        console.log('\n📊 Getting bootstrap node information...');
        const nodeInfoQuery = `
            query {
                getNodeInfo {
                    nodeId
                    peerId
                    health
                    connectedPeers
                    discoveredPeers
                    uptimeSeconds
                }
            }
        `;
        
        const nodeInfoResponse = await graphqlClient.request(nodeInfoQuery) as any;
        const nodeInfo = nodeInfoResponse.getNodeInfo;
        console.log(`Node ID: ${nodeInfo.nodeId}`);
        console.log(`Health: ${nodeInfo.health}`);
        console.log(`Connected Peers: ${nodeInfo.connectedPeers}`);
        console.log(`Discovered Peers: ${nodeInfo.discoveredPeers}`);
        console.log(`Uptime: ${nodeInfo.uptimeSeconds} seconds`);
        
        // Get all blob operations to see what's stored
        console.log('\n📋 Getting all blob operations from bootstrap node...');
        const blobOpsQuery = `
            query {
                getAllBlobOperations(limit: 10) {
                    opId
                    timestamp
                    dbName
                    key
                    value
                    storeType
                    publicKey
                }
            }
        `;
        
        const blobOpsResponse = await graphqlClient.request(blobOpsQuery) as any;
        const operations = blobOpsResponse.getAllBlobOperations;
        console.log(`Found ${operations.length} operations:`);
        
        if (operations.length > 0) {
            operations.forEach((op: any, index: number) => {
                const timestamp = new Date(parseInt(op.timestamp));
                console.log(`  ${index + 1}. ${op.key} (${op.storeType}) - ${timestamp.toLocaleString()}`);
                console.log(`      Value: ${op.value.substring(0, 50)}${op.value.length > 50 ? '...' : ''}`);
            });
            
            // Try to fetch a specific piece of data
            const firstOp = operations[0];
            if (firstOp.storeType === 'String') {
                console.log('\n🔍 Fetching specific string data...');
                const stringQuery = `
                    query {
                        getString(dbName: "${firstOp.dbName}", key: "${firstOp.key}") {
                            key
                            value
                        }
                    }
                `;
                
                const stringResponse = await graphqlClient.request(stringQuery) as any;
                console.log(`✅ Retrieved: ${stringResponse.getString.value}`);
            }
        } else {
            console.log('   No operations found on bootstrap node');
        }
        
        console.log('\n✅ Bootstrap node test completed successfully!');
        
    } catch (error: any) {
        console.error('❌ Error connecting to bootstrap node:', error);
        if (error.message && error.message.includes('fetch')) {
            console.log('\n💡 Network connectivity issue detected');
            console.log('   - Check if bootstrap node is accessible');
            console.log('   - Verify firewall settings');
        }
    }
}

// Run the test
testBootstrapNode().catch(console.error);