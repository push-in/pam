<?php

declare(strict_types=1);

namespace Pam\Api\Resilience;

final class CircuitBreaker
{
    private CircuitState $state = CircuitState::Closed;
    private int $failures = 0;
    private int $openedAt = 0;

    public function __construct(
        private readonly int $failureThreshold = 5,
        private readonly int $cooldownSeconds = 30,
    ) {
        if ($failureThreshold < 1 || $cooldownSeconds < 1) {
            throw new \InvalidArgumentException('Circuit breaker configuration is invalid.');
        }
    }

    public function state(): CircuitState
    {
        if ($this->state === CircuitState::Open && time() >= $this->openedAt + $this->cooldownSeconds) {
            $this->state = CircuitState::HalfOpen;
        }
        return $this->state;
    }

    public function call(callable $operation): mixed
    {
        if ($this->state() === CircuitState::Open) {
            throw new CircuitOpenException('Circuit is open.');
        }
        try {
            $result = $operation();
            $this->failures = 0;
            $this->state = CircuitState::Closed;
            return $result;
        } catch (\Throwable $error) {
            ++$this->failures;
            if ($this->failures >= $this->failureThreshold) {
                $this->state = CircuitState::Open;
                $this->openedAt = time();
            }
            throw $error;
        }
    }
}

