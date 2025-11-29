<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\QueryStats;

class QueryStatsTest extends TestCase
{
    public function testQueryStatsConstruction(): void
    {
        $stats = new QueryStats(
            nodesCreated: 1,
            nodesDeleted: 0,
            relationshipsCreated: 2,
            relationshipsDeleted: 0,
            propertiesSet: 3,
            executionTimeMs: 5.5
        );

        $this->assertEquals(1, $stats->nodesCreated);
        $this->assertEquals(0, $stats->nodesDeleted);
        $this->assertEquals(2, $stats->relationshipsCreated);
        $this->assertEquals(0, $stats->relationshipsDeleted);
        $this->assertEquals(3, $stats->propertiesSet);
        $this->assertEquals(5.5, $stats->executionTimeMs);
    }

    public function testQueryStatsFromArray(): void
    {
        $data = [
            'nodes_created' => 5,
            'nodes_deleted' => 2,
            'relationships_created' => 3,
            'relationships_deleted' => 1,
            'properties_set' => 10,
            'execution_time_ms' => 12.5
        ];

        $stats = QueryStats::fromArray($data);

        $this->assertEquals(5, $stats->nodesCreated);
        $this->assertEquals(2, $stats->nodesDeleted);
        $this->assertEquals(3, $stats->relationshipsCreated);
        $this->assertEquals(1, $stats->relationshipsDeleted);
        $this->assertEquals(10, $stats->propertiesSet);
        $this->assertEquals(12.5, $stats->executionTimeMs);
    }

    public function testQueryStatsDefaultValues(): void
    {
        $stats = new QueryStats();

        $this->assertEquals(0, $stats->nodesCreated);
        $this->assertEquals(0, $stats->nodesDeleted);
        $this->assertEquals(0, $stats->relationshipsCreated);
        $this->assertEquals(0, $stats->relationshipsDeleted);
        $this->assertEquals(0, $stats->propertiesSet);
        $this->assertEquals(0.0, $stats->executionTimeMs);
    }
}
