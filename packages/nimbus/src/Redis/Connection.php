<?php

declare(strict_types=1);

namespace Nimbus\Redis;

interface Connection
{
    /**
     * Sends one command and returns its decoded reply.
     *
     * @param list<string|int|float> $arguments
     */
    public function command(array $arguments): mixed;
}
