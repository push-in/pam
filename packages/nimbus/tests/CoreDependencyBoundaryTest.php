<?php

declare(strict_types=1);

namespace Nimbus\Tests;

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
            'Nimbus core must stay dependency-free; integrations belong in optional packages.',
        );
    }

    /** @throws JsonException */
    public function testNoIlluminateComponentIsInstalledOrSuggested(): void
    {
        $manifest = self::manifest();

        foreach (['require', 'require-dev', 'suggest'] as $section) {
            foreach (array_keys($manifest[$section]) as $package) {
                self::assertStringStartsNotWith('illuminate/', $package, $section);
            }
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
