<?php

declare(strict_types=1);

namespace Nimbus\Tests\Http;

use Nimbus\Http\ClientIpResolver;
use Pam\Http\Request;
use PHPUnit\Framework\TestCase;

final class ClientIpResolverTest extends TestCase
{
    private mixed $remote = null;

    protected function setUp(): void
    {
        $this->remote = $_SERVER['REMOTE_ADDR'] ?? null;
    }

    protected function tearDown(): void
    {
        $_SERVER['REMOTE_ADDR'] = $this->remote;
    }

    public function testTheClientIsTheRightmostUntrustedHop(): void
    {
        $resolver = new ClientIpResolver(['10.0.0.0/8']);
        // The client wrote the left entry; the proxy appended the real address.
        self::assertSame('203.0.113.9', $this->resolve($resolver, '10.0.0.1', '6.6.6.6, 203.0.113.9'));
        self::assertSame('198.51.100.4', $this->resolve($resolver, '10.0.0.1', '6.6.6.6, 198.51.100.4, 10.1.2.3'));
        self::assertSame('10.9.9.9', $this->resolve($resolver, '10.0.0.1', '10.9.9.9, 10.1.1.1'));
    }

    public function testUntrustedPeersAndInvalidEntriesCannotChooseTheAddress(): void
    {
        $resolver = new ClientIpResolver(['127.0.0.1']);
        self::assertSame('203.0.113.7', $this->resolve($resolver, '203.0.113.7', '1.2.3.4'));
        self::assertSame('127.0.0.1', $this->resolve($resolver, '127.0.0.1', '1.2.3.4, unknown'));
        self::assertSame('127.0.0.1', $this->resolve($resolver, '127.0.0.1', '<script>'));
        self::assertSame('192.0.2.1', $this->resolve($resolver, '127.0.0.1', '192.0.2.1'));
    }

    public function testMatchesIpv6RangesAndRejectsInvalidRanges(): void
    {
        $resolver = new ClientIpResolver(['fd00::/8']);
        self::assertSame('2001:db8::1', $this->resolve($resolver, 'fd12::1', '2001:db8::1'));

        $this->expectException(\InvalidArgumentException::class);
        new ClientIpResolver(['10.0.0.0/33']);
    }

    private function resolve(ClientIpResolver $resolver, string $peer, string $forwarded): string
    {
        $_SERVER['REMOTE_ADDR'] = $peer;
        return $resolver->resolve(new Request('GET', '/', [], ['x-forwarded-for' => [$forwarded]], ''));
    }
}
