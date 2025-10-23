#!/usr/bin/env node

/**
 * Script to fetch data from the bootstrap node at 208.73.202.62
 * This demonstrates how to query data from a remote CyberFly node
 */

const BOOTSTRAP_URL = 'http://208.73.202.62:8080/graphql';

// Helper function to make GraphQL requests
async function graphqlQuery(url, query, variables = {}) {
    try {
        const response = await fetch(url, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ query, variables }),
        });

        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }

        const result = await response.json();
        
        if (result.errors) {
            console.error('GraphQL Errors:', JSON.stringify(result.errors, null, 2));
            throw new Error('GraphQL query failed');
        }
        
        return result.data;
    } catch (error) {
        console.error(`Failed to query ${url}:`, error.message);
        throw error;
    }
}

async function main() {
    console.log('🚀 Fetching data from bootstrap node at 208.73.202.62');
    console.log('Bootstrap URL:', BOOTSTRAP_URL);
    console.log();

    try {
        // 1. Get node information
        console.log('1. Getting bootstrap node information...');
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

        const nodeInfo = await graphqlQuery(BOOTSTRAP_URL, nodeInfoQuery);
        console.log('✅ Bootstrap Node Info:');
        console.log(`   Node ID: ${nodeInfo.getNodeInfo.nodeId}`);
        console.log(`   Health: ${nodeInfo.getNodeInfo.health}`);
        console.log(`   Connected Peers: ${nodeInfo.getNodeInfo.connectedPeers}`);
        console.log(`   Discovered Peers: ${nodeInfo.getNodeInfo.discoveredPeers}`);
        console.log(`   Uptime: ${nodeInfo.getNodeInfo.uptimeSeconds} seconds`);
        console.log();

        // 2. Get all blob operations
        console.log('2. Getting blob operations from bootstrap node...');
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

        const blobOps = await graphqlQuery(BOOTSTRAP_URL, blobOpsQuery);
        const operations = blobOps.getAllBlobOperations;
        
        console.log(`✅ Found ${operations.length} operations on bootstrap node:`);
        operations.forEach((op, index) => {
            const timestamp = new Date(parseInt(op.timestamp));
            console.log(`   ${index + 1}. ${op.key} (${op.storeType})`);
            console.log(`      DB: ${op.dbName.substring(0, 20)}...`);
            console.log(`      Time: ${timestamp.toISOString()}`);
            console.log(`      Value: ${op.value.substring(0, 50)}${op.value.length > 50 ? '...' : ''}`);
            console.log();
        });

        // 3. Try to fetch specific data types
        if (operations.length > 0) {
            console.log('3. Fetching specific data from bootstrap node...');
            
            // Find a string operation
            const stringOp = operations.find(op => op.storeType === 'String');
            if (stringOp) {
                console.log('   Fetching string data...');
                const stringQuery = `
                    query {
                        getString(dbName: "${stringOp.dbName}", key: "${stringOp.key}") {
                            key
                            value
                        }
                    }
                `;
                
                const stringData = await graphqlQuery(BOOTSTRAP_URL, stringQuery);
                console.log(`   ✅ String: ${stringData.getString.value}`);
            }

            // Find a JSON operation
            const jsonOp = operations.find(op => op.storeType === 'JSON');
            if (jsonOp) {
                console.log('   Fetching JSON data...');
                const jsonQuery = `
                    query {
                        getJson(dbName: "${jsonOp.dbName}", key: "${jsonOp.key}") {
                            key
                            value
                        }
                    }
                `;
                
                const jsonData = await graphqlQuery(BOOTSTRAP_URL, jsonQuery);
                console.log(`   ✅ JSON: ${jsonData.getJson.value}`);
            }
        }

        console.log();
        console.log('✅ Successfully fetched data from bootstrap node!');

    } catch (error) {
        console.error('❌ Failed to fetch data from bootstrap node:', error.message);
        
        // Check if it's a network connectivity issue
        if (error.message.includes('fetch')) {
            console.log();
            console.log('💡 Troubleshooting tips:');
            console.log('   1. Check if bootstrap node is running on 208.73.202.62:8080');
            console.log('   2. Verify network connectivity to the bootstrap node');
            console.log('   3. Check if firewall is blocking the connection');
            console.log('   4. Try: curl http://208.73.202.62:8080/graphql');
        }
        
        process.exit(1);
    }
}

// Run the script
main().catch(console.error);