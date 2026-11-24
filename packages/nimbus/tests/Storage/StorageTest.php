<?php

declare(strict_types=1);

namespace Nimbus\Tests\Storage;

use Nimbus\App;
use Nimbus\Http\ProblemCode;
use Nimbus\Security\UrlSigner;
use Nimbus\Storage\Disk;
use Nimbus\Storage\SignedStorageUrls;
use Nimbus\Storage\StorageConfig;
use Nimbus\Storage\StorageManager;
use Nimbus\Storage\StorageRoutes;
use Nimbus\Storage\StorageServiceProvider;
use Nimbus\Testing\TestClient;
use Nimbus\Testing\TestResponse;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(StorageManager::class)]
#[CoversClass(StorageConfig::class)]
#[CoversClass(StorageServiceProvider::class)]
#[CoversClass(StorageRoutes::class)]
#[CoversClass(SignedStorageUrls::class)]
#[CoversClass(Disk::class)]
final class StorageTest extends TestCase
{
    private string $root;

    private UrlSigner $signer;

    protected function setUp(): void
    {
        $this->root = sys_get_temp_dir() . '/nimbus-storage-' . bin2hex(random_bytes(6));
        $this->signer = new UrlSigner(str_repeat('k', 32));
    }

    protected function tearDown(): void
    {
        if (!is_dir($this->root)) {
            return;
        }
        $files = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($this->root, \FilesystemIterator::SKIP_DOTS),
            \RecursiveIteratorIterator::CHILD_FIRST,
        );
        foreach ($files as $file) {
            assert($file instanceof \SplFileInfo);
            $file->isDir() ? rmdir($file->getPathname()) : unlink($file->getPathname());
        }
        rmdir($this->root);
    }

    public function testLocalDiskReadsAndWrites(): void
    {
        $disk = (new StorageManager($this->config()))->disk();

        $disk->put('avatars/1.txt', 'hello');
        self::assertTrue($disk->exists('avatars/1.txt'));
        self::assertSame('hello', $disk->get('avatars/1.txt'));
        self::assertSame(5, $disk->size('avatars/1.txt'));
        self::assertSame('text/plain', $disk->mimeType('avatars/1.txt'));
        self::assertSame('https://cdn.example.com/avatars/1.txt', $disk->url('avatars/1.txt'));

        $source = fopen('php://memory', 'r+');
        self::assertIsResource($source);
        fwrite($source, 'streamed');
        rewind($source);
        $disk->writeStream('avatars/2.txt', $source);
        $stream = $disk->readStream('avatars/2.txt');
        self::assertSame('streamed', stream_get_contents($stream));
        fclose($stream);

        $disk->delete('avatars/1.txt');
        self::assertFalse($disk->exists('avatars/1.txt'));
    }

    public function testUnknownDisksAndMissingRoutesFailClearly(): void
    {
        $storage = new StorageManager($this->config());
        try {
            $storage->disk('missing');
            self::fail('An unknown disk must be rejected.');
        } catch (\InvalidArgumentException $error) {
            self::assertStringContainsString('missing', $error->getMessage());
        }
        $this->expectException(\LogicException::class);
        $storage->disk()->temporaryUploadUrl('a.txt', 60, 10);
    }

    public function testProviderRegistersStorageInTheContainer(): void
    {
        $app = $this->app();

        self::assertSame($app->storage(), $app->container()->get(StorageManager::class));
        self::assertSame($app->storage()->disk(), $app->container()->get(Disk::class));

        $app->post('/avatars', static fn (Disk $disk, Response $response): Response => $response->json([
            'upload' => $disk->temporaryUploadUrl('avatars/1.png', 600, 1_000, 'image/png'),
        ]));
        $upload = (new TestClient($app))->postJson('/avatars', [])->assertStatus(200)->json();
        self::assertIsArray($upload);
        self::assertIsString($upload['upload']);
        self::assertStringContainsString('type=image%2Fpng', $upload['upload']);
    }

    public function testSignedUploadStoresTheBody(): void
    {
        $app = $this->app();
        $url = $app->storage()->disk()->temporaryUploadUrl('docs/a.pdf', 60, 16, 'application/pdf');
        self::assertStringStartsWith('https://api.example.com/files/upload?', $url);

        $this->send($app, 'PUT', $url, '%PDF-1.7', 'application/pdf')
            ->assertStatus(201)
            ->assertJson(['disk' => 'local', 'path' => 'docs/a.pdf', 'size' => 8]);
        self::assertSame('%PDF-1.7', file_get_contents($this->root . '/docs/a.pdf'));
    }

    public function testUploadRejectsOversizedBodies(): void
    {
        $app = $this->app();
        $url = $app->storage()->disk()->temporaryUploadUrl('big.bin', 60, 4);

        $this->send($app, 'PUT', $url, '12345', 'application/octet-stream')
            ->assertStatus(413)
            ->assertJsonPath('code', ProblemCode::PayloadTooLarge->value)
            ->assertJsonPath('maxBytes', 4);
        self::assertFileDoesNotExist($this->root . '/big.bin');
    }

    public function testUploadRejectsOtherContentTypes(): void
    {
        $app = $this->app();
        $url = $app->storage()->disk()->temporaryUploadUrl('photo.png', 60, 64, 'image/png');

        $this->send($app, 'PUT', $url, 'GIF89a', 'image/gif')
            ->assertStatus(415)
            ->assertJsonPath('code', ProblemCode::UnsupportedMediaType->value);
        $this->send($app, 'PUT', $url, 'PNG', 'image/png; charset=binary')->assertStatus(201);
    }

    public function testUploadRejectsTamperedAndExpiredUrls(): void
    {
        $app = $this->app();
        $url = $app->storage()->disk()->temporaryUploadUrl('a.txt', 60, 4);

        $this->send($app, 'PUT', str_replace('max=4', 'max=4000', $url), 'hello world')
            ->assertStatus(403)
            ->assertJsonPath('code', ProblemCode::InvalidSignature->value);
        $this->send($app, 'PUT', str_replace('path=a.txt', 'path=b.txt', $url), 'hi')->assertStatus(403);

        $expired = $this->signer->sign('/files/upload', ['disk' => 'local', 'path' => 'a.txt', 'max' => 4], time() - 1);
        $this->send($app, 'PUT', $expired, 'hi')
            ->assertStatus(403)
            ->assertJsonPath('title', 'The signed URL has expired.');

        // A download URL must not authorize an upload to the same file.
        $download = $app->storage()->disk()->temporaryUrl('a.txt', 60);
        $this->send($app, 'PUT', str_replace('/download', '/upload', $download), 'hi')->assertStatus(403);
        self::assertFileDoesNotExist($this->root . '/a.txt');
    }

    public function testTemporaryDownloadStreamsTheFile(): void
    {
        $app = $this->app();
        $disk = $app->storage()->disk();
        $disk->put('report.txt', 'quarterly');

        [$path, $query] = self::split($disk->temporaryUrl('report.txt', 60));
        $exported = $app->handle(new Request('GET', $path, $query, [], ''), new Response())->export();
        self::assertSame(200, $exported['status']);
        self::assertSame('quarterly', implode('', $exported['chunks']));
        self::assertSame(['text/plain'], $exported['headers']['content-type']);

        [$path, $query] = self::split($disk->temporaryUrl('missing.txt', 60));
        (new TestClient($app))->request('GET', $path, $query)->assertStatus(404);
    }

    public function testDownloadsNeverRunAsActiveContentOfTheOrigin(): void
    {
        $app = $this->app();
        $disk = $app->storage()->disk();
        $disk->put('evil.html', '<script>alert(1)</script>');
        $disk->put('photo.png', (string) base64_decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', true));

        [$path, $query] = self::split($disk->temporaryUrl('evil.html', 60));
        $html = $app->handle(new Request('GET', $path, $query, [], ''), new Response())->export();
        self::assertSame(200, $html['status']);
        self::assertSame(['application/octet-stream'], $html['headers']['content-type']);
        self::assertSame(['nosniff'], $html['headers']['x-content-type-options']);
        self::assertStringStartsWith('attachment;', $html['headers']['content-disposition'][0] ?? '');
        self::assertStringContainsString('sandbox', $html['headers']['content-security-policy'][0] ?? '');

        [$path, $query] = self::split($disk->temporaryUrl('photo.png', 60));
        $png = $app->handle(new Request('GET', $path, $query, [], ''), new Response())->export();
        self::assertSame(['image/png'], $png['headers']['content-type']);
        self::assertStringStartsWith('inline;', $png['headers']['content-disposition'][0] ?? '');
    }

    public function testUploadsOfActiveContentAreRefused(): void
    {
        $app = $this->app();
        $disk = $app->storage()->disk();

        $this->send($app, 'PUT', $disk->temporaryUploadUrl('a.html', 60, 64), '<script>', 'application/octet-stream')
            ->assertStatus(415);
        $this->send($app, 'PUT', $disk->temporaryUploadUrl('a.bin', 60, 64), '<svg/>', 'image/svg+xml')
            ->assertStatus(415);
        $this->send($app, 'PUT', $disk->temporaryUploadUrl('a.bin', 60, 64, 'text/html'), '<p>', 'text/html')
            ->assertStatus(415);
        self::assertFileDoesNotExist($this->root . '/a.html');
        self::assertFileDoesNotExist($this->root . '/a.bin');
        $this->send($app, 'PUT', $disk->temporaryUploadUrl('a.bin', 60, 64), 'bytes', 'application/octet-stream')
            ->assertStatus(201);
    }

    public function testTheDeclaredLengthIsCheckedBeforeTheBody(): void
    {
        $app = $this->app();
        $url = $app->storage()->disk()->temporaryUploadUrl('big.bin', 60, 4);
        [$path, $query] = self::split($url);
        (new TestClient($app))->request('PUT', $path, $query, ['content-length' => '999999999'], 'x')
            ->assertStatus(413);
    }

    public function testMissingAppKeyIsReportedForSignedRoutes(): void
    {
        $previous = getenv('APP_KEY');
        putenv('APP_KEY');
        try {
            (new App(discoverPackages: false))->provider(new StorageServiceProvider($this->config(), routes: '/files'));
            self::fail('Signed routes without APP_KEY must be rejected.');
        } catch (\RuntimeException $error) {
            self::assertStringContainsString('APP_KEY', $error->getMessage());
        } finally {
            if (is_string($previous)) {
                putenv('APP_KEY=' . $previous);
            }
        }
    }

    private function app(): App
    {
        $app = new App(discoverPackages: false);
        $app->provider(new StorageServiceProvider($this->config(), routes: '/files', signer: $this->signer));
        return $app;
    }

    private function config(): StorageConfig
    {
        return new StorageConfig(
            'local',
            ['local' => ['driver' => 'local', 'root' => $this->root, 'url' => 'https://cdn.example.com']],
            'https://api.example.com',
        );
    }

    private function send(App $app, string $method, string $url, string $body, ?string $type = null): TestResponse
    {
        [$path, $query] = self::split($url);
        $headers = ['content-length' => (string) strlen($body)];
        if ($type !== null) {
            $headers['content-type'] = $type;
        }
        return (new TestClient($app))->request($method, $path, $query, $headers, $body);
    }

    /** @return array{string, array<string, mixed>} */
    private static function split(string $url): array
    {
        $path = parse_url($url, PHP_URL_PATH);
        parse_str((string) parse_url($url, PHP_URL_QUERY), $query);
        self::assertIsString($path);
        /** @var array<string, mixed> $query */
        return [$path, $query];
    }
}
