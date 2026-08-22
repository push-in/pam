<?php

declare(strict_types=1);

namespace Pam\Api\Resilience;

final readonly class RetryPolicy
{
    /** @var \Closure(\Throwable): bool */
    private \Closure $when;

    public function __construct(
        private int $attempts = 3,
        private int $initialDelayMilliseconds = 10,
        ?callable $when = null,
    ) {
        if ($attempts < 1 || $initialDelayMilliseconds < 0) {
            throw new \InvalidArgumentException('Retry policy configuration is invalid.');
        }
        $this->when = $when === null
            ? static fn (\Throwable $error): bool => true
            : \Closure::fromCallable($when);
    }

    public function run(callable $operation): mixed
    {
        for ($attempt = 1; ; ++$attempt) {
            try {
                return $operation($attempt);
            } catch (\Throwable $error) {
                if ($attempt >= $this->attempts || !($this->when)($error)) {
                    throw $error;
                }
                $delay = $this->initialDelayMilliseconds * (2 ** ($attempt - 1));
                if ($delay > 0) {
                    usleep($delay * 1_000);
                }
            }
        }
    }
}

