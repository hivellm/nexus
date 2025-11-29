<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\QueryBuilder;
use Nexus\SDK\NodePattern;
use Nexus\SDK\RelationshipPattern;
use Nexus\SDK\PathBuilder;

class QueryBuilderTest extends TestCase
{
    public function testSimpleMatch(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->return('n')
            ->build();

        $this->assertEquals('MATCH (n:Person) RETURN n', $query);
    }

    public function testMatchWithWhere(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->where('n.name = $name')
            ->return('n')
            ->build();

        $this->assertEquals('MATCH (n:Person) WHERE n.name = $name RETURN n', $query);
    }

    public function testMatchWithWhereAndOr(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->where('n.age > 18')
            ->and('n.active = true')
            ->or('n.admin = true')
            ->return('n')
            ->build();

        $this->assertEquals('MATCH (n:Person) WHERE n.age > 18 AND n.active = true OR n.admin = true RETURN n', $query);
    }

    public function testOptionalMatch(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->optionalMatch('(n)-[:KNOWS]->(friend)')
            ->return('n', 'friend')
            ->build();

        $this->assertEquals('MATCH (n:Person) OPTIONAL MATCH (n)-[:KNOWS]->(friend) RETURN n, friend', $query);
    }

    public function testCreate(): void
    {
        $query = QueryBuilder::create()
            ->createPattern('(n:Person {name: $name})')
            ->return('n')
            ->build();

        $this->assertEquals('CREATE (n:Person {name: $name}) RETURN n', $query);
    }

    public function testMerge(): void
    {
        $query = QueryBuilder::create()
            ->merge('(n:Person {id: $id})')
            ->set('n.updated = timestamp()')
            ->return('n')
            ->build();

        $this->assertEquals('MERGE (n:Person {id: $id}) SET n.updated = timestamp() RETURN n', $query);
    }

    public function testDelete(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person {name: $name})')
            ->delete('n')
            ->build();

        $this->assertEquals('MATCH (n:Person {name: $name}) DELETE n', $query);
    }

    public function testDetachDelete(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->detachDelete('n')
            ->build();

        $this->assertEquals('MATCH (n:Person) DETACH DELETE n', $query);
    }

    public function testOrderBy(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->return('n.name', 'n.age')
            ->orderBy('n.name')
            ->build();

        $this->assertEquals('MATCH (n:Person) RETURN n.name, n.age ORDER BY n.name', $query);
    }

    public function testOrderByDesc(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->return('n.name', 'n.age')
            ->orderByDesc('n.age')
            ->build();

        $this->assertEquals('MATCH (n:Person) RETURN n.name, n.age ORDER BY n.age DESC', $query);
    }

    public function testSkipAndLimit(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->return('n')
            ->skip(10)
            ->limit(5)
            ->build();

        $this->assertEquals('MATCH (n:Person) RETURN n SKIP 10 LIMIT 5', $query);
    }

    public function testReturnDistinct(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->returnDistinct('n.name')
            ->build();

        $this->assertEquals('MATCH (n:Person) RETURN DISTINCT n.name', $query);
    }

    public function testParameters(): void
    {
        $query = QueryBuilder::create()
            ->match('(n:Person)')
            ->where('n.name = $name')
            ->withParam('name', 'Alice')
            ->return('n');

        $this->assertEquals(['name' => 'Alice'], $query->getParameters());
    }

    public function testMultipleParameters(): void
    {
        $query = QueryBuilder::create()
            ->withParams(['name' => 'Alice', 'age' => 30]);

        $this->assertEquals(['name' => 'Alice', 'age' => 30], $query->getParameters());
    }

    public function testComplexQuery(): void
    {
        $query = QueryBuilder::create()
            ->match('(p:Person)')
            ->match('(c:Company)')
            ->where('p.name = $name')
            ->and('c.name = $company')
            ->createPattern('(p)-[:WORKS_AT]->(c)')
            ->return('p', 'c')
            ->build();

        $expected = 'MATCH (p:Person) MATCH (c:Company) WHERE p.name = $name AND c.name = $company CREATE (p)-[:WORKS_AT]->(c) RETURN p, c';
        $this->assertEquals($expected, $query);
    }
}

class NodePatternTest extends TestCase
{
    public function testSimpleNode(): void
    {
        $pattern = NodePattern::create('n')->build();
        $this->assertEquals('(n)', $pattern);
    }

    public function testNodeWithLabel(): void
    {
        $pattern = NodePattern::create('n')
            ->withLabel('Person')
            ->build();

        $this->assertEquals('(n:Person)', $pattern);
    }

    public function testNodeWithMultipleLabels(): void
    {
        $pattern = NodePattern::create('n')
            ->withLabels('Person', 'Employee')
            ->build();

        $this->assertEquals('(n:Person:Employee)', $pattern);
    }

    public function testNodeWithProperties(): void
    {
        $pattern = NodePattern::create('n')
            ->withLabel('Person')
            ->withProperty('name', 'Alice')
            ->withProperty('age', 30)
            ->build();

        // Properties order might vary, so we check parts
        $this->assertStringContainsString('(n:Person {', $pattern);
        $this->assertStringContainsString("name: 'Alice'", $pattern);
        $this->assertStringContainsString('age: 30', $pattern);
        $this->assertStringEndsWith('})', $pattern);
    }

    public function testNodeWithNullProperty(): void
    {
        $pattern = NodePattern::create('n')
            ->withProperty('deleted', null)
            ->build();

        $this->assertEquals('(n {deleted: null})', $pattern);
    }

    public function testNodeWithBooleanProperty(): void
    {
        $pattern = NodePattern::create('n')
            ->withProperty('active', true)
            ->withProperty('deleted', false)
            ->build();

        $this->assertStringContainsString('active: true', $pattern);
        $this->assertStringContainsString('deleted: false', $pattern);
    }

    public function testNodeToString(): void
    {
        $pattern = NodePattern::create('n')->withLabel('Person');
        $this->assertEquals('(n:Person)', (string) $pattern);
    }
}

class RelationshipPatternTest extends TestCase
{
    public function testSimpleRelationship(): void
    {
        $pattern = RelationshipPattern::create('r')->build();
        $this->assertEquals('-[r]->', $pattern);
    }

    public function testRelationshipWithType(): void
    {
        $pattern = RelationshipPattern::create('r')
            ->withType('KNOWS')
            ->build();

        $this->assertEquals('-[r:KNOWS]->', $pattern);
    }

    public function testIncomingRelationship(): void
    {
        $pattern = RelationshipPattern::create('r')
            ->withType('KNOWS')
            ->incoming()
            ->build();

        $this->assertEquals('<-[r:KNOWS]-', $pattern);
    }

    public function testUndirectedRelationship(): void
    {
        $pattern = RelationshipPattern::create('r')
            ->withType('KNOWS')
            ->undirected()
            ->build();

        $this->assertEquals('-[r:KNOWS]-', $pattern);
    }

    public function testVariableLengthPath(): void
    {
        $pattern = RelationshipPattern::create('r')
            ->withType('KNOWS')
            ->withHops(1, 3)
            ->build();

        $this->assertEquals('-[r:KNOWS*1..3]->', $pattern);
    }

    public function testMinHopsOnly(): void
    {
        $pattern = RelationshipPattern::create('r')
            ->withMinHops(2)
            ->build();

        $this->assertEquals('-[r*2..]->', $pattern);
    }

    public function testMaxHopsOnly(): void
    {
        $pattern = RelationshipPattern::create('r')
            ->withMaxHops(5)
            ->build();

        $this->assertEquals('-[r*..5]->', $pattern);
    }

    public function testRelationshipToString(): void
    {
        $pattern = RelationshipPattern::create('r')->withType('KNOWS');
        $this->assertEquals('-[r:KNOWS]->', (string) $pattern);
    }
}

class PathBuilderTest extends TestCase
{
    public function testSimplePath(): void
    {
        $node1 = NodePattern::create('a')->withLabel('Person')->build();
        $rel = RelationshipPattern::create('r')->withType('KNOWS')->build();
        $node2 = NodePattern::create('b')->withLabel('Person')->build();

        $path = PathBuilder::path($node1, $rel, $node2);

        $this->assertEquals('(a:Person)-[r:KNOWS]->(b:Person)', $path);
    }

    public function testComplexPath(): void
    {
        $node1 = NodePattern::create('a')->withLabel('Person')->build();
        $rel1 = RelationshipPattern::create('r1')->withType('KNOWS')->build();
        $node2 = NodePattern::create('b')->withLabel('Person')->build();
        $rel2 = RelationshipPattern::create('r2')->withType('WORKS_WITH')->build();
        $node3 = NodePattern::create('c')->withLabel('Person')->build();

        $path = PathBuilder::path($node1, $rel1, $node2, $rel2, $node3);

        $this->assertEquals('(a:Person)-[r1:KNOWS]->(b:Person)-[r2:WORKS_WITH]->(c:Person)', $path);
    }
}
