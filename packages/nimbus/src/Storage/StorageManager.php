<?php

declare(strict_types=1);

namespace Nimbus\Storage;

use League\Flysystem\Filesystem;
use League\Flysystem\FilesystemOperator;
use League\Flysystem\Local\LocalFilesystemAdapter;
use Nimbus\Security\UrlSigner;

/** Named disks built on first use and kept for the worker's lifetime. */
final class StorageManager
{
    /** @var array<string, Disk> */
    private array $disks = [];

    /**
     * @param ?UrlSigner $signer Signs temporary URLs of local disks.
     * @param ?string $routes Path of the registered `StorageRoutes`; null leaves local disks
     *        without temporary download and upload URLs.
     */
    public function __construct(
        private readonly StorageConfig $config,
        private readonly ?UrlSigner $signer = null,
        private readonly ?string $routes = null,
    ) {
        if (!interface_exists(FilesystemOperator::class)) {
            throw new \LogicException('Nimbus storage requires league/flysystem: composer require league/flysystem.');
        }
    }

    public function disk(?string $name = null): Disk
    {
        $name ??= $this->config->defaultDisk;
        return $this->disks[$name] ??= $this->build($name, $this->config->disk($name));
    }

    /** Adds or replaces a disk, such as one over another flysystem adapter. */
    public function register(Disk $disk): self
    {
        $this->disks[$disk->name()] = $disk;
        return $this;
    }

    /** Wraps a flysystem operator as a disk whose temporary URLs go through the signed routes. */
    public function filesystem(string $name, FilesystemOperator $filesystem): Disk
    {
        $disk = new Disk($name, $filesystem, $this->signedUrls($name));
        $this->register($disk);
        return $disk;
    }

    /** @param array<string, mixed> $options */
    private function build(string $name, array $options): Disk
    {
        return match ($options['driver'] ?? null) {
            'local' => $this->local($name, $options),
            's3' => $this->s3($name, $options),
            default => throw new \InvalidArgumentException("Disk {$name} has an unsupported driver."),
        };
    }

    /** @param array<string, mixed> $options */
    private function local(string $name, array $options): Disk
    {
        $root = $options['root'] ?? null;
        if (!is_string($root) || $root === '') {
            throw new \InvalidArgumentException("Local disk {$name} needs a root directory.");
        }
        $signed = $this->signedUrls($name);
        return new Disk($name, new Filesystem(
            new LocalFilesystemAdapter($root),
            self::publicUrl($options),
            temporaryUrlGenerator: $signed,
        ), $signed);
    }

    /** @param array<string, mixed> $options */
    private function s3(string $name, array $options): Disk
    {
        $clientClass = 'Aws\\S3\\S3Client';
        $adapterClass = 'League\\Flysystem\\AwsS3V3\\AwsS3V3Adapter';
        if (!class_exists($adapterClass) || !class_exists($clientClass)) {
            throw new \LogicException(
                "Disk {$name} requires league/flysystem-aws-s3-v3: composer require league/flysystem-aws-s3-v3.",
            );
        }
        $bucket = $options['bucket'] ?? null;
        if (!is_string($bucket) || $bucket === '') {
            throw new \InvalidArgumentException("S3 disk {$name} needs a bucket.");
        }
        $client = [
            'version' => 'latest',
            'region' => is_string($options['region'] ?? null) ? $options['region'] : 'us-east-1',
            'use_path_style_endpoint' => ($options['path_style'] ?? false) === true,
        ];
        if (is_string($options['endpoint'] ?? null)) {
            $client['endpoint'] = $options['endpoint'];
        }
        if (is_string($options['key'] ?? null) && is_string($options['secret'] ?? null)) {
            $client['credentials'] = ['key' => $options['key'], 'secret' => $options['secret']];
        }
        $prefix = is_string($options['prefix'] ?? null) ? $options['prefix'] : '';
        $client = new $clientClass($client);
        $adapter = new $adapterClass($client, $bucket, $prefix);
        if (!$adapter instanceof \League\Flysystem\FilesystemAdapter) {
            throw new \LogicException('The S3 adapter does not implement FilesystemAdapter.');
        }
        return new Disk(
            $name,
            new Filesystem($adapter, self::publicUrl($options)),
            new S3UploadUrlGenerator($client, $bucket, $prefix),
        );
    }

    private function signedUrls(string $disk): ?SignedStorageUrls
    {
        if ($this->signer === null || $this->routes === null) {
            return null;
        }
        return new SignedStorageUrls($this->signer, $disk, $this->routes, $this->config->url);
    }

    /**
     * @param array<string, mixed> $options
     * @return array<string, string>
     */
    private static function publicUrl(array $options): array
    {
        $url = $options['url'] ?? null;
        return is_string($url) && $url !== '' ? ['public_url' => $url] : [];
    }
}
