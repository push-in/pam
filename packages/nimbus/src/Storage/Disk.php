<?php

declare(strict_types=1);

namespace Nimbus\Storage;

use League\Flysystem\FilesystemOperator;

/** A named disk over a flysystem operator. Failures raise `League\Flysystem\FilesystemException`. */
final readonly class Disk
{
    public function __construct(
        private string $name,
        private FilesystemOperator $filesystem,
        private ?UploadUrlGenerator $uploads = null,
    ) {
    }

    public function name(): string
    {
        return $this->name;
    }

    /** The underlying flysystem operator, for listing, moving, visibility and the rest. */
    public function filesystem(): FilesystemOperator
    {
        return $this->filesystem;
    }

    /** @param array<string, mixed> $config Flysystem write options (visibility, mimetype, ...). */
    public function put(string $path, string $contents, array $config = []): void
    {
        $this->filesystem->write($path, $contents, $config);
    }

    public function get(string $path): string
    {
        return $this->filesystem->read($path);
    }

    /** @return resource */
    public function readStream(string $path)
    {
        return $this->filesystem->readStream($path);
    }

    /**
     * @param resource $stream
     * @param array<string, mixed> $config
     */
    public function writeStream(string $path, $stream, array $config = []): void
    {
        $this->filesystem->writeStream($path, $stream, $config);
    }

    public function exists(string $path): bool
    {
        return $this->filesystem->fileExists($path);
    }

    public function delete(string $path): void
    {
        $this->filesystem->delete($path);
    }

    public function size(string $path): int
    {
        return $this->filesystem->fileSize($path);
    }

    public function mimeType(string $path): string
    {
        return $this->filesystem->mimeType($path);
    }

    /** Public URL: the disk `url` prefix, or the adapter's own (S3). */
    public function url(string $path): string
    {
        return $this->filesystem->publicUrl($path);
    }

    /** Download URL valid for `$ttl` seconds: presigned on S3, a signed route on local disks. */
    public function temporaryUrl(string $path, int $ttl): string
    {
        return $this->filesystem->temporaryUrl($path, self::expiresAt($ttl));
    }

    /**
     * URL a client uploads `$path` to with `PUT` within `$ttl` seconds. Local disks enforce
     * `$maxBytes` and `$contentType` in the signed upload route; S3 enforces the content type only.
     */
    public function temporaryUploadUrl(string $path, int $ttl, int $maxBytes, ?string $contentType = null): string
    {
        if ($maxBytes < 1) {
            throw new \InvalidArgumentException('The upload size limit must be positive.');
        }
        if ($this->uploads === null) {
            throw new \LogicException(
                "Disk {$this->name} has no upload URLs; register the storage routes for local disks.",
            );
        }
        return $this->uploads->temporaryUploadUrl($path, self::expiresAt($ttl), $maxBytes, $contentType);
    }

    /**
     * Test assertion: every path exists, and holds `$contents` when given.
     *
     * @param string|list<string> $paths
     */
    public function assertExists(string|array $paths, ?string $contents = null): self
    {
        foreach (is_string($paths) ? [$paths] : $paths as $path) {
            self::assert($this->exists($path), "Unable to find a file at path [{$path}] on disk {$this->name}.");
            if ($contents !== null) {
                self::assert($this->get($path) === $contents, "File [{$path}] on disk {$this->name} does not hold the expected contents.");
            }
        }
        return $this;
    }

    /**
     * Test assertion: no path exists.
     *
     * @param string|list<string> $paths
     */
    public function assertMissing(string|array $paths): self
    {
        foreach (is_string($paths) ? [$paths] : $paths as $path) {
            self::assert(!$this->exists($path), "Found an unexpected file at path [{$path}] on disk {$this->name}.");
        }
        return $this;
    }

    private static function assert(bool $condition, string $message): void
    {
        if (class_exists(\PHPUnit\Framework\Assert::class)) {
            \PHPUnit\Framework\Assert::assertTrue($condition, $message);
            return;
        }
        if (!$condition) {
            throw new \LogicException($message);
        }
    }

    private static function expiresAt(int $ttl): \DateTimeImmutable
    {
        if ($ttl < 1) {
            throw new \InvalidArgumentException('A temporary URL lifetime must be positive.');
        }
        return new \DateTimeImmutable('@' . (time() + $ttl));
    }
}
