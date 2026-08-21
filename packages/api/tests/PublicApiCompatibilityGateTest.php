<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use PHPUnit\Framework\TestCase;

final class PublicApiCompatibilityGateTest extends TestCase
{
    public function testCommittedPublicApiBaselinePasses(): void
    {
        [$status, $output] = $this->runGate(__DIR__ . '/../api-surface.json');

        self::assertSame(0, $status, $output);
        self::assertStringContainsString('Public API compatible:', $output);
    }

    public function testRemovedPublicSymbolFailsWithStableIntegerCode(): void
    {
        $baseline = $this->baseline();
        $baseline['symbols']['Pam\\Api\\Compatibility\\RemovedFixture'] = [
            'kind' => 'class',
            'final' => false,
            'readonly' => false,
            'abstract' => false,
            'parent' => null,
            'interfaces' => [],
            'constants' => [],
            'properties' => [],
            'methods' => [],
        ];
        $this->assertGateFailure(
            $baseline,
            '[1] Pam\\Api\\Compatibility\\RemovedFixture: public symbol was removed',
        );
    }

    public function testChangedMethodSignatureFailsWithStableIntegerCode(): void
    {
        $baseline = $this->baseline();
        $router = $baseline['symbols']['Pam\\Api\\Router'] ?? null;
        $methods = is_array($router) ? ($router['methods'] ?? null) : null;
        $routes = is_array($methods) ? ($methods['routes'] ?? null) : null;
        if (!is_array($router) || !is_array($methods) || !is_array($routes)) {
            self::fail('Router API fixture is malformed.');
        }
        $routes['return'] = 'string';
        $methods['routes'] = $routes;
        $router['methods'] = $methods;
        $baseline['symbols']['Pam\\Api\\Router'] = $router;

        $this->assertGateFailure(
            $baseline,
            '[4] Pam\\Api\\Router::routes(): method return changed',
        );
    }

    public function testAddingAnInterfaceMethodIsDetectedAsBreaking(): void
    {
        $baseline = $this->baseline();
        $queue = $baseline['symbols']['Pam\\Api\\Jobs\\JobQueue'] ?? null;
        $methods = is_array($queue) ? ($queue['methods'] ?? null) : null;
        if (!is_array($queue) || !is_array($methods)) {
            self::fail('JobQueue API fixture is malformed.');
        }
        unset($methods['reserve']);
        $queue['methods'] = $methods;
        $baseline['symbols']['Pam\\Api\\Jobs\\JobQueue'] = $queue;

        $this->assertGateFailure(
            $baseline,
            '[3] Pam\\Api\\Jobs\\JobQueue::reserve(): method was added to an interface',
        );
    }

    public function testUnreviewedPublicSymbolAdditionMakesTheBaselineStale(): void
    {
        $baseline = $this->baseline();
        unset($baseline['symbols']['Pam\\Api\\Jobs\\JobCodec']);

        $this->assertGateFailure(
            $baseline,
            '[5] Pam\\Api\\Jobs\\JobCodec: public symbol is not in the baseline',
        );
    }

    /** @return array{schema: int, symbols: array<string, array<string, mixed>>} */
    private function baseline(): array
    {
        $baseline = json_decode(
            (string) file_get_contents(__DIR__ . '/../api-surface.json'),
            true,
            64,
            JSON_THROW_ON_ERROR,
        );
        self::assertIsArray($baseline);
        self::assertIsArray($baseline['symbols'] ?? null);
        /** @var array{schema: int, symbols: array<string, array<string, mixed>>} $baseline */
        return $baseline;
    }

    /**
     * @param array{schema: int, symbols: array<string, array<string, mixed>>} $baseline
     */
    private function assertGateFailure(array $baseline, string $expected): void
    {
        $fixture = tempnam(sys_get_temp_dir(), 'pam-api-surface-');
        self::assertIsString($fixture);
        file_put_contents($fixture, json_encode($baseline, JSON_THROW_ON_ERROR));

        try {
            [$status, $output] = $this->runGate($fixture);
            self::assertSame(1, $status);
            self::assertStringContainsString($expected, $output);
        } finally {
            unlink($fixture);
        }
    }

    /** @return array{int, string} */
    private function runGate(string $baseline): array
    {
        $command = escapeshellarg(PHP_BINARY)
            . ' ' . escapeshellarg(__DIR__ . '/../bin/api-compat')
            . ' check ' . escapeshellarg($baseline)
            . ' 2>&1';
        $output = [];
        $status = 0;
        exec($command, $output, $status);
        return [$status, implode("\n", $output)];
    }
}
