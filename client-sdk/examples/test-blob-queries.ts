#!/usr/bin/env node

/**
 * Test script for Blob Operation Queries
 * 
 * This script demonstrates how to:
 * 1. Submit signed data to the database
 * 2. Query blob operations by database name
 * 3. Get operation counts
 * 4. Retrieve operations since a timestamp
 */

const API_URL = 'http://localhost:8080';

// GraphQL response type
interface GraphQLResponse {
    data?: any;
    errors?: Array<{ message: string }>;
}

// Helper function to make GraphQL requests
async function graphqlQuery(query: string, variables?: Record<string, any>): Promise<any> {
    const response = await fetch(API_URL, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({ query, variables }),
    });

    const result = await response.json() as GraphQLResponse;
    
    if (result.errors) {
        console.error('GraphQL Errors:', JSON.stringify(result.errors, null, 2));
        throw new Error('GraphQL query failed');
    }
    
    return result.data;
}

// Test 1: Get operation count
async function testGetOperationCount(dbName: string) {
    console.log('\n=== Test 1: Get Operation Count ===');
    
    const query = `
        query GetOperationCount($dbName: String) {
            getBlobOperationCount(dbName: $dbName)
        }
    `;
    
    const data = await graphqlQuery(query, { dbName });
    console.log(`Total operations for database "${dbName}":`, data.getBlobOperationCount);
    
    // Get total count across all databases
    const allData = await graphqlQuery(query);
    console.log('Total operations across all databases:', allData.getBlobOperationCount);
}

// Test 2: Get blob operations by database name
async function testGetBlobOperations(dbName: string, limit: number = 10) {
    console.log('\n=== Test 2: Get Blob Operations ===');
    
    const query = `
        query GetBlobOperations($dbName: String!, $limit: Int) {
            getBlobOperations(dbName: $dbName, limit: $limit) {
                opId
                timestamp
                dbName
                key
                value
                storeType
                publicKey
                signature
            }
        }
    `;
    
    const data = await graphqlQuery(query, { dbName, limit });
    console.log(`Found ${data.getBlobOperations.length} operations:`);
    
    data.getBlobOperations.forEach((op: any, idx: number) => {
        console.log(`\n[${idx + 1}] Operation ${op.opId.substring(0, 8)}...`);
        console.log(`    Timestamp: ${new Date(parseInt(op.timestamp)).toISOString()}`);
        console.log(`    Key: ${op.key}`);
        console.log(`    Store Type: ${op.storeType}`);
        console.log(`    Value: ${op.value.substring(0, 50)}${op.value.length > 50 ? '...' : ''}`);
    });
}

// Test 3: Get operations since timestamp
async function testGetOperationsSince(dbName: string, minutesAgo: number = 5) {
    console.log('\n=== Test 3: Get Operations Since Timestamp ===');
    
    const timestamp = Date.now() - (minutesAgo * 60 * 1000);
    console.log(`Getting operations since ${minutesAgo} minutes ago (${new Date(timestamp).toISOString()})`);
    
    const query = `
        query GetOperationsSince($dbName: String!, $timestamp: String!, $limit: Int) {
            getBlobOperationsSince(dbName: $dbName, timestamp: $timestamp, limit: $limit) {
                opId
                timestamp
                key
                value
                storeType
            }
        }
    `;
    
    const data = await graphqlQuery(query, { 
        dbName, 
        timestamp: timestamp.toString(),
        limit: 10 
    });
    
    console.log(`Found ${data.getBlobOperationsSince.length} recent operations`);
    
    data.getBlobOperationsSince.forEach((op: any, idx: number) => {
        console.log(`\n[${idx + 1}] ${op.key} (${op.storeType})`);
        console.log(`    Time: ${new Date(parseInt(op.timestamp)).toISOString()}`);
    });
}

// Test 4: Get all blob operations across databases
async function testGetAllBlobOperations(limit: number = 20) {
    console.log('\n=== Test 4: Get All Blob Operations ===');
    
    const query = `
        query GetAllBlobOperations($limit: Int) {
            getAllBlobOperations(limit: $limit) {
                dbName
                timestamp
                key
                storeType
            }
        }
    `;
    
    const data = await graphqlQuery(query, { limit });
    console.log(`Found ${data.getAllBlobOperations.length} total operations across all databases`);
    
    // Group by database
    const byDatabase = data.getAllBlobOperations.reduce((acc: any, op: any) => {
        if (!acc[op.dbName]) {
            acc[op.dbName] = [];
        }
        acc[op.dbName].push(op);
        return acc;
    }, {});
    
    console.log('\nOperations by database:');
    Object.entries(byDatabase).forEach(([dbName, ops]: [string, any]) => {
        console.log(`  ${dbName}: ${ops.length} operations`);
    });
}

// Main test runner
async function main() {
    console.log('🚀 Starting Blob Operation Query Tests');
    console.log('API URL:', API_URL);
    
    // Replace with your actual database name
    // Format: <name>-<public_key_hex>
    const TEST_DB_NAME = 'example-68ca42370da148bd4e3f6922a4de62a417477994ebaf1c1fd3d0601766a8fff7';
    
    try {
        // Run all tests
        await testGetOperationCount(TEST_DB_NAME);
        await testGetBlobOperations(TEST_DB_NAME, 10);
        await testGetOperationsSince(TEST_DB_NAME, 5);
        await testGetAllBlobOperations(20);
        
        console.log('\n✅ All tests completed successfully!');
        
    } catch (error) {
        console.error('\n❌ Test failed:', error);
        process.exit(1);
    }
}

// Check if node is running
async function checkServer() {
    try {
        const response = await fetch(API_URL, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ query: '{ __typename }' }),
        });
        return response.ok;
    } catch {
        return false;
    }
}

// Run tests
(async () => {
    console.log('Checking if server is running...');
    
    if (!(await checkServer())) {
        console.error('❌ Server is not running at', API_URL);
        console.error('Start the server with: cargo run');
        process.exit(1);
    }
    
    console.log('✅ Server is running\n');
    await main();
})();
