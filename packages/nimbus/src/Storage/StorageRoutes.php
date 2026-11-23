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
 *
 * Files come from users, so they are never served as active content of the application's
 * origin: downloads carry `X-Content-Type-Options: nosniff`, a sandboxing CSP and
 * `Content-Disposition: attachment`, except the passive types in `$inlineTypes` (images other
 * than SVG, PDF, audio, video), which display inline. Uploads of HTML, SVG, XML or script types
 * or extensions are refused (415) unless `$allowActiveContent` is set.
 */
final readonly class StorageRoutes
{
    private const int CHUNK_BYTES = 65_536;

    public const array DEFAULT_INLINE_TYPES = [
        'image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/avif',
        'application/pdf', 'audio/mpeg', 'audio/ogg', 'video/mp4', 'video/webm', 'text/plain',
    ];

    private const array ACTIVE_EXTENSIONS = [
        'htm', 'html', 'xhtml', 'xht', 'shtml', 'svg', 'svgz', 'xml', 'xsl', 'xslt', 'js', 'mjs',
        'php', 'phtml', 'phar', 'swf', 'hta', 'vbs', 'wasm',
    ];

    /** @param list<string> $inlineTypes */
    public function __construct(
        private StorageManager $storage,
        private array $inlineTypes = self::DEFAULT_INLINE_TYPES,
        private bool $allowActiveContent = false,
    ) {
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
        // The declared size is checked before the body is touched.
        if ($length !== null && ctype_digit($length) && (int) $length > $max) {
            throw new HttpException(413, ProblemCode::PayloadTooLarge, 'The upload exceeds the allowed size.', [
                'maxBytes' => $max,
            ]);
        }
        $type = $query['type'] ?? null;
        $actual = strtolower(trim(explode(';', $request->getHeader('content-type') ?? '', 2)[0]));
        if (!$this->allowActiveContent && (self::active($actual) || (is_string($type) && self::active(strtolower($type)))
            || in_array(strtolower(pathinfo($path, PATHINFO_EXTENSION)), self::ACTIVE_EXTENSIONS, true))) {
            throw new HttpException(415, ProblemCode::UnsupportedMediaType, 'Active content (HTML, SVG, XML, scripts) cannot be uploaded.');
        }
        $body = $request->body();
        if (strlen($body) > $max) {
            throw new HttpException(413, ProblemCode::PayloadTooLarge, 'The upload exceeds the allowed size.', [
                'maxBytes' => $max,
            ]);
        }
        $config = [];
        if (is_string($type) && $type !== '') {
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
        $type = strtolower(trim(explode(';', $type, 2)[0]));
        $inline = in_array($type, $this->inlineTypes, true) && !self::active($type);
        $name = str_replace(['"', '\\', "\r", "\n"], '_', basename($path));
        $response
            ->header('x-content-type-options', 'nosniff')
            ->header('content-security-policy', "default-src 'none'; sandbox")
            ->header('content-disposition', ($inline ? 'inline' : 'attachment')
                . '; filename="' . $name . '"; filename*=UTF-8\'\'' . rawurlencode(basename($path)));
        $stream = $disk->readStream($path);
        return $response->stream(self::chunks($stream), $inline ? $type : 'application/octet-stream');
    }

    /** Types a browser runs as markup or script in the origin that serves them. */
    private static function active(string $type): bool
    {
        return $type !== '' && (str_contains($type, 'html') || str_contains($type, 'xml') || str_contains($type, 'svg')
            || str_contains($type, 'javascript') || str_contains($type, 'ecmascript') || $type === 'application/x-shockwave-flash');
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
