<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

final readonly class MessageResult
{
    public function __construct(
        public MessageDisposition $disposition,
        public ?int $retryAfterMilliseconds = null,
    ) {
        if ($disposition !== MessageDisposition::Retry && $retryAfterMilliseconds !== null) {
            throw new \InvalidArgumentException('Only retry results may declare a retry delay.');
        }
        if ($retryAfterMilliseconds !== null
            && ($retryAfterMilliseconds < 0 || $retryAfterMilliseconds > 86_400_000)
        ) {
            throw new \InvalidArgumentException('Transport retry delay must be between zero and one day.');
        }
    }

    public static function acknowledge(): self
    {
        return new self(MessageDisposition::Acknowledge);
    }

    public static function retry(?int $afterMilliseconds = null): self
    {
        return new self(MessageDisposition::Retry, $afterMilliseconds);
    }

    public static function reject(): self
    {
        return new self(MessageDisposition::Reject);
    }
}
