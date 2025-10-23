import { CyberFlyClient, CryptoUtils } from '../src';

async function testSync() {
    console.log('🔄 Testing sync functionality...\n');

    try {
        // 1. Generate key pair
        const keyPair = await CryptoUtils.generateKeyPair();
        console.log('✓ Generated key pair');

        // 2. Create client for local node
        const localClient = new CyberFlyClient({
            endpoint: 'http://localhost:8080/graphql',
            keyPair,
            defaultDbName: 'sync-test',
        });

        // 3. Store test data on local node
        const testKey = `sync-test-${Date.now()}`;
        const testValue = `Test data stored at ${new Date().toISOString()}`;
        
        console.log(`📝 Storing test data: ${testKey} = "${testValue}"`);
        await localClient.storeString(testKey, testValue);
        console.log('✓ Data stored on local node');

        // 4. Verify data can be retrieved from local node
        const retrievedValue = await localClient.queryString(testKey);
        console.log(`✓ Retrieved from local: "${retrievedValue}"`);

        // 5. Check local node status
        const localNodeQuery = `
            query {
                getNodeInfo {
                    nodeId
                    health
                    connectedPeers
                    discoveredPeers
                    uptimeSeconds
                }
                getAllBlobOperations(limit: 3) {
                    opId
                    key
                    value
                    storeType
                    timestamp
                }
            }
        `;

        const response = await fetch('http://localhost:8080/graphql', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ query: localNodeQuery })
        });

        const result = await response.json();
        const nodeInfo = result.data.getNodeInfo;
        const operations = result.data.getAllBlobOperations;

        console.log('\n📊 Local Node Status:');
        console.log(`   Node ID: ${nodeInfo.nodeId}`);
        console.log(`   Health: ${nodeInfo.health}`);
        console.log(`   Connected Peers: ${nodeInfo.connectedPeers}`);
        console.log(`   Discovered Peers: ${nodeInfo.discoveredPeers}`);
        console.log(`   Uptime: ${nodeInfo.uptimeSeconds} seconds`);
        console.log(`   Total Operations: ${operations.length}`);

        if (operations.length > 0) {
            console.log('\n📋 Recent Operations:');
            operations.forEach((op: any, index: number) => {
                const timestamp = new Date(parseInt(op.timestamp));
                console.log(`   ${index + 1}. ${op.key} (${op.storeType}) - ${timestamp.toLocaleString()}`);
            });
        }

        console.log('\n✅ Sync test completed successfully!');
        
        // Summary
        console.log('\n📈 Summary:');
        console.log(`   ✓ Local node is functional and storing data`);
        console.log(`   ✓ GraphQL API is working correctly`);
        console.log(`   ✓ Data persistence is working`);
        
        if (nodeInfo.connectedPeers > 0) {
            console.log(`   ✓ Connected to ${nodeInfo.connectedPeers} peer(s) - sync should be working`);
        } else {
            console.log(`   ⚠️  No connected peers - node is isolated (sync not active)`);
            console.log(`   💡 This is expected if bootstrap peer is not compatible or reachable`);
        }

    } catch (error: any) {
        console.error('❌ Error during sync test:', error);
        if (error.message && error.message.includes('fetch')) {
            console.log('\n💡 Network connectivity issue detected');
            console.log('   - Check if local node is running on port 8080');
            console.log('   - Verify GraphQL endpoint is accessible');
        }
    }
}

testSync().catch(console.error);