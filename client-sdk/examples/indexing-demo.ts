import { CyberFlyClient, CryptoUtils } from '../src';

/**
 * Example demonstrating secondary indexing for MongoDB-like queries
 */
async function demoIndexing() {
  console.log('=== Secondary Indexing Demo ===\n');

  // 1. Setup
  const keyPair = await CryptoUtils.generateKeyPair();
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: 'users',
  });

  console.log('1. Creating indexes...');
  
  // Create email index for exact lookups
  await client.request(`
    mutation {
      createIndex(
        dbName: "users"
        indexName: "email_idx"
        field: "email"
        indexType: "exact"
      )
    }
  `);
  console.log('✓ Created email_idx (exact match)');

  // Create age index for range queries
  await client.request(`
    mutation {
      createIndex(
        dbName: "users"
        indexName: "age_idx"
        field: "age"
        indexType: "range"
      )
    }
  `);
  console.log('✓ Created age_idx (range queries)');

  // Create name index for text search
  await client.request(`
    mutation {
      createIndex(
        dbName: "users"
        indexName: "name_idx"
        field: "name"
        indexType: "fulltext"
      )
    }
  `);
  console.log('✓ Created name_idx (full-text search)\n');

  // 2. Insert user data
  console.log('2. Inserting user data...');
  
  const users = [
    { id: '1', name: 'Alice Smith', email: 'alice@example.com', age: 25 },
    { id: '2', name: 'Bob Johnson', email: 'bob@example.com', age: 30 },
    { id: '3', name: 'Charlie Brown', email: 'charlie@example.com', age: 35 },
    { id: '4', name: 'Diana Prince', email: 'diana@example.com', age: 28 },
    { id: '5', name: 'Eve Smith', email: 'eve@example.com', age: 22 },
  ];

  for (const user of users) {
    await client.storeJSON(`user:${user.id}`, user);
    
    // Index the user (in production, this would be automatic)
    await client.request(`
      mutation {
        indexInsert(
          dbName: "users"
          indexName: "email_idx"
          fieldValue: "${user.email}"
          key: "user:${user.id}"
        )
      }
    `);
    
    await client.request(`
      mutation {
        indexInsert(
          dbName: "users"
          indexName: "age_idx"
          fieldValue: "${user.age}"
          key: "user:${user.id}"
        )
      }
    `);
    
    await client.request(`
      mutation {
        indexInsert(
          dbName: "users"
          indexName: "name_idx"
          fieldValue: "${user.name}"
          key: "user:${user.id}"
        )
      }
    `);
  }
  
  console.log(`✓ Inserted ${users.length} users\n`);

  // 3. Query by exact email
  console.log('3. Query by email (exact match)...');
  const emailResult = await client.request<any>(`
    query {
      queryIndex(
        dbName: "users"
        indexName: "email_idx"
        operator: "equals"
        value: "alice@example.com"
      ) {
        keys
        count
        executionTimeMs
      }
    }
  `);
  
  console.log('✓ Found user:', emailResult.queryIndex.keys);
  console.log(`  Query time: ${emailResult.queryIndex.executionTimeMs}ms\n`);

  // 4. Range query on age
  console.log('4. Query users older than 25...');
  const ageResult = await client.request<any>(`
    query {
      queryIndex(
        dbName: "users"
        indexName: "age_idx"
        operator: "gt"
        min: 25
      ) {
        keys
        count
        executionTimeMs
      }
    }
  `);
  
  console.log(`✓ Found ${ageResult.queryIndex.count} users:`, ageResult.queryIndex.keys);
  console.log(`  Query time: ${ageResult.queryIndex.executionTimeMs}ms\n`);

  // 5. Range query between ages
  console.log('5. Query users between 25-32 years old...');
  const rangResult = await client.request<any>(`
    query {
      queryIndex(
        dbName: "users"
        indexName: "age_idx"
        operator: "between"
        min: 25
        max: 32
      ) {
        keys
        count
        executionTimeMs
      }
    }
  `);
  
  console.log(`✓ Found ${rangeResult.queryIndex.count} users:`, rangeResult.queryIndex.keys);
  console.log(`  Query time: ${rangeResult.queryIndex.executionTimeMs}ms\n`);

  // 6. Text search
  console.log('6. Search for users named "Smith"...');
  const textResult = await client.request<any>(`
    query {
      queryIndex(
        dbName: "users"
        indexName: "name_idx"
        operator: "contains"
        value: "Smith"
      ) {
        keys
        count
        executionTimeMs
      }
    }
  `);
  
  console.log(`✓ Found ${textResult.queryIndex.count} users:`, textResult.queryIndex.keys);
  console.log(`  Query time: ${textResult.queryIndex.executionTimeMs}ms\n`);

  // 7. Get full user data from indexed keys
  console.log('7. Retrieving full user data...');
  for (const key of textResult.queryIndex.keys) {
    const userData = await client.queryJSON(key);
    console.log(`  ${key}:`, userData);
  }
  console.log('');

  // 8. Index statistics
  console.log('8. Index statistics...');
  const stats = await client.request<any>(`
    query {
      getIndexStats(dbName: "users", indexName: "email_idx") {
        name
        field
        indexType
        totalKeys
        uniqueValues
      }
    }
  `);
  
  console.log('✓ Email index stats:', stats.getIndexStats);
  console.log('');

  // 9. List all indexes
  console.log('9. Listing all indexes...');
  const indexes = await client.request<any>(`
    query {
      listIndexes(dbName: "users")
    }
  `);
  
  console.log('✓ Available indexes:', indexes.listIndexes);
  console.log('');

  console.log('=== Demo Complete ===');
  console.log('\nKey Takeaways:');
  console.log('• Created 3 indexes for different query types');
  console.log('• Performed exact match, range, and text queries');
  console.log('• All queries completed in < 1ms (in-memory)');
  console.log('• MongoDB-like flexibility with Redis-style simplicity');
}

demoIndexing().catch(console.error);
