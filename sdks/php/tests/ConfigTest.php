<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\Config;

class ConfigTest extends TestCase
{
    public function testConfigDefaultValues(): void
    {
        $config = new Config();

        $this->assertEquals('http://localhost:15474', $config->baseUrl);
        $this->assertNull($config->apiKey);
        $this->assertNull($config->username);
        $this->assertNull($config->password);
        $this->assertEquals(30, $config->timeout);
    }

    public function testConfigCustomValues(): void
    {
        $config = new Config(
            baseUrl: 'http://custom:8080',
            apiKey: 'my-api-key',
            username: 'admin',
            password: 'secret',
            timeout: 60
        );

        $this->assertEquals('http://custom:8080', $config->baseUrl);
        $this->assertEquals('my-api-key', $config->apiKey);
        $this->assertEquals('admin', $config->username);
        $this->assertEquals('secret', $config->password);
        $this->assertEquals(60, $config->timeout);
    }
}
