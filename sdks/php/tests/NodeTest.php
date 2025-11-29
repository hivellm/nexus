<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\Node;

class NodeTest extends TestCase
{
    public function testNodeConstruction(): void
    {
        $node = new Node(
            id: '123',
            labels: ['Person', 'Employee'],
            properties: ['name' => 'Alice', 'age' => 30]
        );

        $this->assertEquals('123', $node->id);
        $this->assertEquals(['Person', 'Employee'], $node->labels);
        $this->assertEquals(['name' => 'Alice', 'age' => 30], $node->properties);
    }

    public function testNodeFromArray(): void
    {
        $data = [
            'id' => '456',
            'labels' => ['Person'],
            'properties' => ['name' => 'Bob']
        ];

        $node = Node::fromArray($data);

        $this->assertEquals('456', $node->id);
        $this->assertEquals(['Person'], $node->labels);
        $this->assertEquals(['name' => 'Bob'], $node->properties);
    }

    public function testNodeFromArrayWithMissingFields(): void
    {
        $data = [];
        $node = Node::fromArray($data);

        $this->assertEquals('', $node->id);
        $this->assertEquals([], $node->labels);
        $this->assertEquals([], $node->properties);
    }

    public function testNodeDefaultValues(): void
    {
        $node = new Node();

        $this->assertEquals('', $node->id);
        $this->assertEquals([], $node->labels);
        $this->assertEquals([], $node->properties);
    }
}
