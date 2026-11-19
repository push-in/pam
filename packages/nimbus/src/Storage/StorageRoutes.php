<?php

declare(strict_types=1);

namespace Nimbus\Storage;

use Nimbus\App;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Signed routes behind the temporary URLs of local disks: `PUT {path}/upload` writes the request
 * body (bounded by the signed size and content type) and `GET {path}/download` streams a file.
 * The runtime buffers request bodies up to its `maxBodyBytes` listen option (2 MiB by default).
 */
final readonly class StorageRoutes
{
    private const int CHUNK_BYTES = 65_536;

    public function __construct(private StorageManager $storage)
    {
    }

    public static function uploadPath(string $path): string
    {
        return rtrim('/' . trim($path, '/'), '/') . '/upload';
    }

    public static function downloadPath(string $path): string
    {
        return rtrim('/' . trim($path, '/'), '/') . '/download';
    }

    public function register(App $app, string $path): void
    {
        $app->put(self::uploadPath($path), $this->upload(...))
            ->signed()
            ->summary('Upload a file to a signed storage URL')
            ->tags(['storage']);
        $app->get(self::downloadPath($path), $this->download(...))
            ->signed()
            ->summary('Download a file from a signed storage URL')
            ->tags(['storage']);
    }

    public function upload(Request $request, Response $response): Response
    {
        [$disk, $path] = $this->target($request);
        $query = $request->query();
        $max = $query['max'] ?? null;
        if (!is_string($max) && !is_int($max) || !is_numeric($max) || (int) $max < 1) {
            throw new HttpException(403, ProblemCode::InvalidSignature, 'The upload URL has no size limit.');
        }
        $max = (int) $max;
        $length = $request->getHeader('content-length');
        $body = $request->body();
        if (($length !== null && ctype_digit($length) && (int) $length > $max) || strlen($body) > $max) {
            throw new HttpException(413, ProblemCode::PayloadTooLarge, 'The upload exceeds the allowed size.', [
                'maxBytes' => $max,
            ]);
        }
        $type = $query['type'] ?? null;
        $config = [];
        if (is_string($type) && $type !== '') {
            $actual = strtolower(trim(explode(';', $request->getHeader('content-type') ?? '', 2)[0]));
            if ($actual !== strtolower($type)) {
                throw new HttpException(415, ProblemCode::UnsupportedMediaType, 'The upload has another content type.', [
                    'contentType' => $type,
                ]);
            }
            $config['mimetype'] = $type;
        }
        $disk->put($path, $body, $config);
        return $response->json(['disk' => $disk->name(), 'path' => $path, 'size' => strlen($body)], 201);
    }

    public function download(Request $request, Response $response): Response
    {
        [$disk, $path] = $this->target($request);
        if (!$disk->exists($path)) {
            throw new HttpException(404, ProblemCode::NotFound, 'File not found.');
        }
        try {
            $type = $disk->mimeType($path);
        } catch (\League\Flysystem\FilesystemException) {
            $type = 'application/octet-stream';
        }
        $stream = $disk->readStream($path);
        return $response->stream(self::chunks($stream), $type);
    }

    /** @return array{Disk, string} */
    private function target(Request $request): array
    {
        $query = $request->query();
        $name = $query['disk'] ?? null;
        $path = $query['path'] ?? null;
        if (!is_string($name) || !is_string($path) || $path === '') {
            throw new HttpException(403, ProblemCode::InvalidSignature, 'The storage URL names no file.');
        }
        try {
            return [$this->storage->disk($name), $path];
        } catch (\InvalidArgumentException) {
            throw new HttpException(404, ProblemCode::NotFound, 'Disk not found.');
        }
    }

    /**
     * @param resource $stream
     * @return \Generator<int, string>
     */
    private static function chunks($stream): \Generator
    {
        try {
            while (!feof($stream)) {
                $chunk = fread($stream, self::CHUNK_BYTES);
                if ($chunk === false) {
                    break;
                }
                if ($chunk !== '') {
                    yield $chunk;
                }
            }
        } finally {
            fclose($stream);
        }
    }
}
