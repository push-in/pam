<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Psr\Log\AbstractLogger;

/**
 * Forwards the queries Arcano drivers log, with their elapsed time, to a listener.
 *
 * @internal
 */
final class ArcanoQueryLogger extends AbstractLogger
{
    /** @param \Closure(string, float): void $listener */
    public function __construct(private readonly \Closure $listener)
    {
    }

    /** @param array<array-key, mixed> $context */
    public function log($level, string|\Stringable $message, array $context = []): void
    {
        // Queries carry their elapsed seconds; transaction notices do not.
        if (!isset($context['elapsed']) || !is_float($context['elapsed'])) {
            return;
        }
        ($this->listener)((string) $message, $context['elapsed'] * 1000);
    }
}
