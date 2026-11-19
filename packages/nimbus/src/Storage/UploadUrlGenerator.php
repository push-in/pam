<?php

declare(strict_types=1);

namespace Nimbus\Storage;

/** Produces URLs a client uses to upload one file directly, without the application in between. */
interface UploadUrlGenerator
{
    public function temporaryUploadUrl(
        string $path,
        \DateTimeImmutable $expiresAt,
        int $maxBytes,
        ?string $contentType = null,
    ): string;
}
