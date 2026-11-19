<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

/** A settable clock for the auth stores. */
final class Clock
{
    public function __construct(public int $now = 1_800_000_000)
    {
    }

    /** @return \Closure(): int */
    public function closure(): \Closure
    {
        return fn (): int => $this->now;
    }
}
