<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\QueryResult;
use Nexus\SDK\QueryStats;

class QueryResultTest extends TestCase
{
    public function testQueryResultConstruction(): void
    {
        $stats = new QueryStats(nodesCreated: 1);
        $result = new QueryResult(
            columns: ['name', 'age'],
            rows: [['Alice', 30], ['Bob', 25]],
            stats: $stats
        );

        $this->assertEquals(['name', 'age'], $result->columns);
        $this->assertEquals([['Alice', 30], ['Bob', 25]], $result->rows);
        $this->assertNotNull($result->stats);
        $this->assertEquals(1, $result->stats->nodesCreated);
    }

    public function testQueryResultFromArray(): void
    {
        $data = [
            'columns' => ['n.name'],
            'rows' => [['Alice'], ['Bob']],
            'stats' => [
                'nodes_created' => 0,
                'execution_time_ms' => 1.5
            ]
        ];

        $result = QueryResult::fromArray($data);

        $this->assertEquals(['n.name'], $result->columns);
        $this->assertEquals([['Alice'], ['Bob']], $result->rows);
        $this->assertNotNull($result->stats);
        $this->assertEquals(1.5, $result->stats->executionTimeMs);
    }

    public function testQueryResultFromArrayWithoutStats(): void
    {
        $data = [
            'columns' => ['count'],
            'rows' => [[42]]
        ];

        $result = QueryResult::fromArray($data);

        $this->assertEquals(['count'], $result->columns);
        $this->assertEquals([[42]], $result->rows);
        $this->assertNull($result->stats);
    }

    public function testQueryResultDefaultValues(): void
    {
        $result = new QueryResult();

        $this->assertEquals([], $result->columns);
        $this->assertEquals([], $result->rows);
        $this->assertNull($result->stats);
    }
}
