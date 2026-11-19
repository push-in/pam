<?php

declare(strict_types=1);

namespace Nimbus\Storage;

use League\Flysystem\Config;
use League\Flysystem\UrlGeneration\TemporaryUrlGenerator;
use Nimbus\Security\UrlSigner;

/** Temporary download and upload URLs for a disk, served by the signed routes of `StorageRoutes`. */
final readonly class SignedStorageUrls implements TemporaryUrlGenerator, UploadUrlGenerator
{
    /**
     * @param string $baseUrl Absolute origin (APP_URL); empty yields root-relative URLs.
     * @param string $routes Path the storage routes are registered under.
     */
    public function __construct(
        private UrlSigner $signer,
        private string $disk,
        private string $routes,
        private string $baseUrl = '',
    ) {
    }

    public function temporaryUrl(string $path, \DateTimeInterface $expiresAt, Config $config): string
    {
        return $this->baseUrl . $this->signer->sign(
            StorageRoutes::downloadPath($this->routes),
            ['disk' => $this->disk, 'path' => $path],
            $expiresAt->getTimestamp(),
        );
    }

    public function temporaryUploadUrl(
        string $path,
        \DateTimeImmutable $expiresAt,
        int $maxBytes,
        ?string $contentType = null,
    ): string {
        $query = ['disk' => $this->disk, 'path' => $path, 'max' => $maxBytes];
        if ($contentType !== null) {
            $query['type'] = $contentType;
        }
        return $this->baseUrl . $this->signer->sign(
            StorageRoutes::uploadPath($this->routes),
            $query,
            $expiresAt->getTimestamp(),
        );
    }
}
