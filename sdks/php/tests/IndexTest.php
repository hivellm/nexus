<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\Index;

class IndexTest extends TestCase
{
    public function testIndexConstruction(): void
    {
        $index = new Index(
            name: 'person_name_idx',
            label: 'Person',
            properties: ['name'],
            type: 'btree'
        );

        $this->assertEquals('person_name_idx', $index->name);
        $this->assertEquals('Person', $index->label);
        $this->assertEquals(['name'], $index->properties);
        $this->assertEquals('btree', $index->type);
    }

    public function testIndexFromArray(): void
    {
        $data = [
            'name' => 'company_idx',
            'label' => 'Company',
            'properties' => ['name', 'id'],
            'type' => 'composite'
        ];

        $index = Index::fromArray($data);

        $this->assertEquals('company_idx', $index->name);
        $this->assertEquals('Company', $index->label);
        $this->assertEquals(['name', 'id'], $index->properties);
        $this->assertEquals('composite', $index->type);
    }

    public function testIndexDefaultValues(): void
    {
        $index = new Index();

        $this->assertEquals('', $index->name);
        $this->assertEquals('', $index->label);
        $this->assertEquals([], $index->properties);
        $this->assertEquals('', $index->type);
    }
}
