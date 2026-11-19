<?php

declare(strict_types=1);

namespace Nimbus\Storage;

/**
 * Presigned S3 `PutObject` URLs through the AWS SDK client (flysystem only presigns reads).
 * S3 binds the content type of a presigned PUT but cannot cap its size: check `Disk::size()`
 * after the client reports the upload, or keep uploads of untrusted size on a local disk route.
 */
final readonly class S3UploadUrlGenerator implements UploadUrlGenerator
{
    /** @param object $client An `Aws\S3\S3Client`. */
    public function __construct(
        private object $client,
        private string $bucket,
        private string $prefix = '',
    ) {
    }

    public function temporaryUploadUrl(
        string $path,
        \DateTimeImmutable $expiresAt,
        int $maxBytes,
        ?string $contentType = null,
    ): string {
        $arguments = ['Bucket' => $this->bucket, 'Key' => ltrim(rtrim($this->prefix, '/') . '/' . ltrim($path, '/'), '/')];
        if ($contentType !== null) {
            $arguments['ContentType'] = $contentType;
        }
        $client = $this->client;
        if (!method_exists($client, 'getCommand') || !method_exists($client, 'createPresignedRequest')) {
            throw new \LogicException('S3 uploads require an Aws\\S3\\S3Client.');
        }
        $request = $client->createPresignedRequest($client->getCommand('PutObject', $arguments), $expiresAt);
        if (!is_object($request) || !method_exists($request, 'getUri')) {
            throw new \UnexpectedValueException('The AWS SDK returned no presigned request.');
        }
        $uri = $request->getUri();
        return is_string($uri) || $uri instanceof \Stringable ? (string) $uri : throw new \UnexpectedValueException('The presigned request has no URI.');
    }
}
