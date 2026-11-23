<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\App;
use Nimbus\Cache\Cache;
use Nimbus\Http\Client\Http;

/**
 * The application the `Queue`, `Event`, `Mail`, `Storage` and `Broadcast` test facades
 * bind their fakes into. `TestCase` begins and ends it around every test; call
 * `begin()`/`end()` yourself when you do not extend it.
 */
final class Fakes
{
    private static ?App $app = null;

    /** @var array<class-string, object> */
    private static array $fakes = [];

    /** @var list<string> */
    private static array $directories = [];

    private function __construct()
    {
    }

    public static function begin(App $app): void
    {
        self::end();
        self::$app = $app;
    }

    /** Forgets the fakes, deletes faked storage and restores the worker-level facades. */
    public static function end(): void
    {
        self::$app = null;
        self::$fakes = [];
        foreach (self::$directories as $directory) {
            self::delete($directory);
        }
        self::$directories = [];
        Http::swap(null);
        Cache::useRepository(null);
    }

    public static function app(): App
    {
        return self::$app ?? throw new \LogicException(
            'No application to fake into: run the test in a Nimbus\\Testing\\TestCase or call Fakes::begin($app).',
        );
    }

    /**
     * @template T of object
     * @param T $fake
     * @return T
     */
    public static function remember(object $fake): object
    {
        self::$fakes[$fake::class] = $fake;
        return $fake;
    }

    /**
     * @template T of object
     * @param class-string<T> $class
     * @return T
     */
    public static function get(string $class, string $facade): object
    {
        $fake = self::$fakes[$class] ?? null;
        return $fake instanceof $class ? $fake : throw new \LogicException("Call {$facade}::fake() before its assertions.");
    }

    public static function temporaryDirectory(): string
    {
        $directory = sys_get_temp_dir() . '/nimbus-fake-' . bin2hex(random_bytes(6));
        if (!mkdir($directory, 0700, true) && !is_dir($directory)) {
            throw new \RuntimeException("Cannot create {$directory}.");
        }
        self::$directories[] = $directory;
        return $directory;
    }

    private static function delete(string $directory): void
    {
        if (!is_dir($directory)) {
            return;
        }
        $items = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($directory, \FilesystemIterator::SKIP_DOTS),
            \RecursiveIteratorIterator::CHILD_FIRST,
        );
        foreach ($items as $item) {
            if (!$item instanceof \SplFileInfo) {
                continue;
            }
            $item->isDir() && !$item->isLink() ? @rmdir($item->getPathname()) : @unlink($item->getPathname());
        }
        @rmdir($directory);
    }
}
