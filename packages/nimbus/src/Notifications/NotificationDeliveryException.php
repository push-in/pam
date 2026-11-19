<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

/**
 * A failed delivery. Retryable failures (timeouts, 429, 5xx) are retried by the job queue;
 * permanent ones are dead-lettered.
 */
final class NotificationDeliveryException extends \RuntimeException
{
    public function __construct(
        string $message,
        public readonly bool $retryable = true,
        ?\Throwable $previous = null,
    ) {
        parent::__construct($message, 0, $previous);
    }

    public static function permanent(string $message, ?\Throwable $previous = null): self
    {
        return new self($message, false, $previous);
    }
}
