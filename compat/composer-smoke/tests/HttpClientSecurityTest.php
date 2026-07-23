<?php

declare(strict_types=1);

namespace Pam\Compatibility\Tests;

use Pam\Http\Client;
use PHPUnit\Framework\TestCase;

if (!class_exists(Client::class)) {
    require_once __DIR__ . '/../../../runtime/async.php';
    require_once __DIR__ . '/../../../runtime/http_client.php';
}

final class HttpClientSecurityTest extends TestCase
{
    public function testMalformedAuthoritiesAreRejectedBeforeConnecting(): void
    {
        $client = new Client(timeout: 0.1);
        $rejected = 0;
        foreach ([
            'http://invalid_host.example/health',
            'http://user:secret@127.0.0.1/health',
        ] as $url) {
            try {
                $client->get($url);
                self::fail("Malformed URL was accepted: {$url}");
            } catch (\InvalidArgumentException) {
                ++$rejected;
            }
        }
        self::assertSame(2, $rejected);
    }
}
