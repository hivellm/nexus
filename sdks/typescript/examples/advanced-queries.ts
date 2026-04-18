import { NexusClient } from '../src';

/**
 * Advanced queries example
 */
async function main() {
  const client = new NexusClient({
    baseUrl: 'http://localhost:7687',
    auth: {
      username: 'admin',
      password: 'password',
    },
  });

  try {
    // Create a small graph
    console.log('Creating a social network graph...\n');

    const alice = await client.createNode(['Person'], {
      name: 'Alice',
      age: 30,
      city: 'New York',
    });

    const bob = await client.createNode(['Person'], {
      name: 'Bob',
      age: 28,
      city: 'New York',
    });

    const charlie = await client.createNode(['Person'], {
      name: 'Charlie',
      age: 35,
      city: 'San Francisco',
    });

    await client.createRelationship(alice.id, bob.id, 'KNOWS', { since: 2018 });
    await client.createRelationship(alice.id, charlie.id, 'KNOWS', { since: 2020 });
    await client.createRelationship(bob.id, charlie.id, 'KNOWS', { since: 2019 });

    // Pattern matching query
    console.log('Finding friends of friends...');
    const foaf = await client.executeCypher(`
      MATCH (person:Person {name: $name})-[:KNOWS]->(friend)-[:KNOWS]->(foaf)
      WHERE person <> foaf
      RETURN DISTINCT foaf.name AS name, foaf.city AS city
    `, { name: 'Alice' });
    console.log('Friends of friends:', foaf.rows);
    console.log();

    // Aggregation query
    console.log('Counting persons by city...');
    const byCity = await client.executeCypher(`
      MATCH (p:Person)
      RETURN p.city AS city, count(p) AS count
      ORDER BY count DESC
    `);
    console.log('Persons by city:', byCity.rows);
    console.log();

    // Path query
    console.log('Finding shortest path...');
    const path = await client.executeCypher(`
      MATCH path = shortestPath((a:Person {name: $from})-[:KNOWS*]-(b:Person {name: $to}))
      RETURN length(path) AS distance
    `, { from: 'Alice', to: 'Charlie' });
    console.log('Shortest path distance:', path.rows[0]?.distance);
    console.log();

    // Filtering with multiple conditions
    console.log('Finding persons in NYC older than 25...');
    const filtered = await client.executeCypher(`
      MATCH (p:Person)
      WHERE p.city = $city AND p.age > $minAge
      RETURN p.name AS name, p.age AS age
      ORDER BY p.age DESC
    `, { city: 'New York', minAge: 25 });
    console.log('Results:', filtered.rows);
    console.log();

    // Using COLLECT to group data
    console.log('Getting friends list for each person...');
    const friendsLists = await client.executeCypher(`
      MATCH (p:Person)
      OPTIONAL MATCH (p)-[:KNOWS]->(friend)
      RETURN p.name AS person, collect(friend.name) AS friends
      ORDER BY p.name
    `);
    console.log('Friends lists:');
    friendsLists.rows.forEach(row => {
      console.log(`  ${row.person}: [${row.friends.join(', ')}]`);
    });
    console.log();

    // Cleanup
    console.log('Cleaning up...');
    await client.executeCypher('MATCH (p:Person) DETACH DELETE p');
    console.log('✓ Cleanup complete');

  } catch (error) {
    console.error('Error:', error);
    process.exit(1);
  }
}

main().catch(console.error);

