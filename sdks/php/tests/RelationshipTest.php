<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\Relationship;

class RelationshipTest extends TestCase
{
    public function testRelationshipConstruction(): void
    {
        $rel = new Relationship(
            id: '789',
            type: 'KNOWS',
            startNode: '123',
            endNode: '456',
            properties: ['since' => 2020]
        );

        $this->assertEquals('789', $rel->id);
        $this->assertEquals('KNOWS', $rel->type);
        $this->assertEquals('123', $rel->startNode);
        $this->assertEquals('456', $rel->endNode);
        $this->assertEquals(['since' => 2020], $rel->properties);
    }

    public function testRelationshipFromArray(): void
    {
        $data = [
            'id' => '111',
            'type' => 'WORKS_AT',
            'start_node' => '222',
            'end_node' => '333',
            'properties' => ['role' => 'Developer']
        ];

        $rel = Relationship::fromArray($data);

        $this->assertEquals('111', $rel->id);
        $this->assertEquals('WORKS_AT', $rel->type);
        $this->assertEquals('222', $rel->startNode);
        $this->assertEquals('333', $rel->endNode);
        $this->assertEquals(['role' => 'Developer'], $rel->properties);
    }

    public function testRelationshipFromArrayWithMissingFields(): void
    {
        $data = [];
        $rel = Relationship::fromArray($data);

        $this->assertEquals('', $rel->id);
        $this->assertEquals('', $rel->type);
        $this->assertEquals('', $rel->startNode);
        $this->assertEquals('', $rel->endNode);
        $this->assertEquals([], $rel->properties);
    }
}
