<?php

declare(strict_types=1);

namespace Nimbus\Runtime;

use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;

final readonly class Deadline
{
    private int $expiresAtNanoseconds;

    public function __construct(float $seconds)
    {
        if ($seconds <= 0) {
            throw new \InvalidArgumentException('Deadline duration must be positive.');
        }
        $this->expiresAtNanoseconds = hrtime(true) + (int) ($seconds * 1_000_000_000);
    }

    public function expired(): bool
    {
        return hrtime(true) >= $this->expiresAtNanoseconds;
    }

    public function throwIfExpired(): void
    {
        if ($this->expired()) {
            throw new HttpException(504, ProblemCode::Timeout, 'Request deadline exceeded.');
        }
    }
}

