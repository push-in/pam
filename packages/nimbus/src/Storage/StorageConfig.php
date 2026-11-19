<?php

declare(strict_types=1);

namespace Nimbus\Storage;

/**
 * Named disks. A disk is `['driver' => 'local', 'root' => ..., 'url' => ?]` or
 * `['driver' => 's3', 'bucket' => ..., 'region' => ..., 'endpoint' => ?, 'key' => ?,
 * 'secret' => ?, 'path_style' => bool, 'prefix' => ?, 'url' => ?]`.
 */
final readonly class StorageConfig
{
    /**
     * @param non-empty-string $defaultDisk
     * @param array<non-empty-string, array<string, mixed>> $disks
     * @param string $url Absolute base URL prepended to signed storage routes (APP_URL).
     */
    public function __construct(
        public string $defaultDisk,
        public array $disks,
        public string $url = '',
    ) {
        if (!isset($this->disks[$this->defaultDisk])) {
            throw new \InvalidArgumentException("The default disk {$this->defaultDisk} is not configured.");
        }
        foreach ($this->disks as $name => $disk) {
            $driver = $disk['driver'] ?? null;
            if ($driver !== 'local' && $driver !== 's3') {
                throw new \InvalidArgumentException("Disk {$name} must use the local or s3 driver.");
            }
        }
    }

    /**
     * Disk `local` from STORAGE_ROOT (default `./storage`) and STORAGE_URL; disk `s3` when
     * AWS_BUCKET is set (AWS_DEFAULT_REGION, AWS_ENDPOINT, AWS_ACCESS_KEY_ID,
     * AWS_SECRET_ACCESS_KEY, AWS_USE_PATH_STYLE_ENDPOINT, AWS_URL). FILESYSTEM_DISK picks the default.
     */
    public static function fromEnvironment(): self
    {
        $disks = [
            'local' => self::filter([
                'driver' => 'local',
                'root' => self::environment('STORAGE_ROOT') ?? (getcwd() ?: '.') . '/storage',
                'url' => self::environment('STORAGE_URL'),
            ]),
        ];
        $bucket = self::environment('AWS_BUCKET');
        if ($bucket !== null) {
            $disks['s3'] = self::filter([
                'driver' => 's3',
                'bucket' => $bucket,
                'region' => self::environment('AWS_DEFAULT_REGION') ?? 'us-east-1',
                'endpoint' => self::environment('AWS_ENDPOINT'),
                'key' => self::environment('AWS_ACCESS_KEY_ID'),
                'secret' => self::environment('AWS_SECRET_ACCESS_KEY'),
                'path_style' => filter_var(self::environment('AWS_USE_PATH_STYLE_ENDPOINT') ?? 'false', FILTER_VALIDATE_BOOL),
                'url' => self::environment('AWS_URL'),
            ]);
        }
        $default = self::environment('FILESYSTEM_DISK') ?? 'local';
        return new self($default, $disks, rtrim(self::environment('APP_URL') ?? '', '/'));
    }

    /** @return array<string, mixed> */
    public function disk(string $name): array
    {
        return $this->disks[$name] ?? throw new \InvalidArgumentException("Disk {$name} is not configured.");
    }

    /** @return ?non-empty-string */
    private static function environment(string $key): ?string
    {
        $value = getenv($key);
        return is_string($value) && $value !== '' ? $value : null;
    }

    /**
     * @param array<string, mixed> $options
     * @return array<string, mixed>
     */
    private static function filter(array $options): array
    {
        return array_filter($options, static fn (mixed $value): bool => $value !== null);
    }
}
