<?php

declare(strict_types=1);

namespace Nimbus\Health;

final class HealthRegistry
{
    /** @var array<string, HealthCheck> */
    private array $checks = [];

    public function add(string $name, HealthCheck $check): self
    {
        if ($name === '' || isset($this->checks[$name])) {
            throw new \InvalidArgumentException('Health-check name must be non-empty and unique.');
        }
        $this->checks[$name] = $check;
        return $this;
    }

    /**
     * A check that throws counts as unhealthy instead of failing the probe.
     *
     * @return array{state: int, checks: array<string, array{state: int, details: array<string, scalar|null>}>}
     */
    public function run(): array
    {
        $state = HealthState::Healthy;
        $checks = [];
        foreach ($this->checks as $name => $check) {
            try {
                $result = $check->check();
            } catch (\Throwable $error) {
                $result = new HealthResult(HealthState::Unhealthy, ['error' => $error::class]);
            }
            if ($result->state->value > $state->value) {
                $state = $result->state;
            }
            $checks[$name] = ['state' => $result->state->value, 'details' => $result->details];
        }
        return ['state' => $state->value, 'checks' => $checks];
    }
}
