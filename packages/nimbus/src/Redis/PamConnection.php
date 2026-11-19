<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Pam\Redis\Client;

/** A connection through the PAM runtime's non-blocking Redis client. */
final readonly class PamConnection implements Connection
{
    public function __construct(private Client $client)
    {
    }

    public function command(array $arguments): mixed
    {
        return $this->client->command($arguments);
    }
}
