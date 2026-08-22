<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use JsonException;
use PHPUnit\Framework\Attributes\CoversNothing;
use PHPUnit\Framework\TestCase;

#[CoversNothing]
final class CoreDependencyBoundaryTest extends TestCase
{
    /** @throws JsonException */
    public function testProductionInstallContainsOnlyTheRuntimeContract(): void
    {
        $manifest = self::manifest();

        self::assertSame(
            ['php', 'pushinbr/pam-contracts'],
            array_keys($manifest['require']),
            'pam-http must remain an HTTP kernel; integrations belong in optional packages.',
        );
    }

    /** @throws JsonException */
    public function testIlluminateIsDevelopmentOnlyUntilTheLegacyBridgeIsExtracted(): void
    {
        $manifest = self::manifest();

        foreach (['database', 'events', 'filesystem', 'pagination'] as $component) {
            $package = 'illuminate/' . $component;

            self::assertArrayNotHasKey($package, $manifest['require']);
            self::assertArrayHasKey($package, $manifest['require-dev']);
            self::assertArrayHasKey($package, $manifest['suggest']);
        }
    }

    /**
     * @return array{
     *     require: array<string, string>,
     *     require-dev: array<string, string>,
     *     suggest: array<string, string>
     * }
     *
     * @throws JsonException
     */
    private static function manifest(): array
    {
        $manifest = json_decode(
            (string) file_get_contents(dirname(__DIR__) . '/composer.json'),
            true,
            512,
            JSON_THROW_ON_ERROR,
        );

        self::assertIsArray($manifest);
        self::assertArrayHasKey('require', $manifest);
        self::assertArrayHasKey('require-dev', $manifest);
        self::assertArrayHasKey('suggest', $manifest);

        return [
            'require' => self::stringMap($manifest['require']),
            'require-dev' => self::stringMap($manifest['require-dev']),
            'suggest' => self::stringMap($manifest['suggest']),
        ];
    }

    /** @return array<string, string> */
    private static function stringMap(mixed $value): array
    {
        self::assertIsArray($value);

        $map = [];

        foreach ($value as $key => $item) {
            self::assertIsString($key);
            self::assertIsString($item);
            $map[$key] = $item;
        }

        return $map;
    }
}
