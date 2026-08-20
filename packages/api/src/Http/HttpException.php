<?php

declare(strict_types=1);

namespace Pam\Api\Http;

class HttpException extends \RuntimeException
{
    /** @param array<string, mixed> $details */
    public function __construct(
        public readonly int $status,
        public readonly ProblemCode $problemCode,
        string $message,
        public readonly array $details = [],
        ?\Throwable $previous = null,
    ) {
        parent::__construct($message, $problemCode->value, $previous);
    }
}

