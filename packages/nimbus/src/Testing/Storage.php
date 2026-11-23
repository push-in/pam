<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use League\Flysystem\Filesystem;
use League\Flysystem\Local\LocalFilesystemAdapter;
use Nimbus\Storage\Disk;
use Nimbus\Storage\StorageConfig;
use Nimbus\Storage\StorageManager;

/**
 * Test facade: `Storage::fake('avatars')` replaces the disk with an empty local disk in a
 * temporary directory, deleted after the test. Assert on it with `Disk::assertExists()`.
 */
final class Storage
{
    private function __construct()
    {
    }

    /** @param ?string $disk Defaults to the default disk (`local` without StorageServiceProvider). */
    public static function fake(?string $disk = null): Disk
    {
        if (!class_exists(LocalFilesystemAdapter::class)) {
            throw new \LogicException('Storage::fake() requires league/flysystem.');
        }
        $container = Fakes::app()->container();
        $directory = Fakes::temporaryDirectory();
        $manager = $container->has(StorageManager::class) ? $container->get(StorageManager::class) : null;
        $config = $container->has(StorageConfig::class) ? $container->get(StorageConfig::class) : null;
        $default = $config instanceof StorageConfig ? $config->defaultDisk : 'local';
        $name = $disk ?? $default;
        if ($name === '') {
            throw new \InvalidArgumentException('The disk name cannot be empty.');
        }
        if (!$manager instanceof StorageManager) {
            $config = new StorageConfig($name, [$name => ['driver' => 'local', 'root' => $directory]]);
            $manager = new StorageManager($config);
            $container->instance(StorageConfig::class, $config)->instance(StorageManager::class, $manager);
            $default = $name;
        }
        $faked = $manager->filesystem($name, new Filesystem(new LocalFilesystemAdapter($directory)));
        if ($name === $default) {
            $container->instance(Disk::class, $faked);
        }
        return $faked;
    }

    public static function disk(?string $name = null): Disk
    {
        $manager = Fakes::app()->container()->get(StorageManager::class);
        if (!$manager instanceof StorageManager) {
            throw new \LogicException('Call Storage::fake() first or register StorageServiceProvider.');
        }
        return $manager->disk($name);
    }
}
