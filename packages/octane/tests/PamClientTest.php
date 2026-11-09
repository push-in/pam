<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Laravel\Octane\OctaneResponse;
use Laravel\Octane\RequestContext;
use Pam\Http\Request as PamRequest;
use Pam\Http\Response as PamResponse;
use Pam\Octane\PamClient;
use PHPUnit\Framework\TestCase;
use Symfony\Component\HttpFoundation\Response;
use UnexpectedValueException;

final class PamClientTest extends TestCase
{
    protected function setUp(): void
    {
        parent::setUp();

        $_GET = $_POST = $_COOKIE = $_FILES = [];
        $_SERVER = [
            'REQUEST_METHOD' => 'GET',
            'REQUEST_URI' => '/ping',
            'SERVER_NAME' => 'localhost',
            'SERVER_PORT' => '8000',
        ];
    }

    public function testItBridgesACompleteOctaneExchange(): void
    {
        $target = new PamResponse();
        $context = new RequestContext([
            'pamRequest' => new PamRequest('GET', '/ping', [], [], ''),
            'pamResponse' => $target,
        ]);
        $client = new PamClient();

        [$request, $context] = $client->marshalRequest($context);
        $client->respond($context, new OctaneResponse(new Response('pong')));

        self::assertSame('ping', $request->path());
        self::assertSame('pong', $target->export()['body']);
    }

    public function testItServesPublicFilesAndTheStorageLinkOnly(): void
    {
        $root = sys_get_temp_dir().'/pam-octane-static-'.bin2hex(random_bytes(6));
        mkdir($root.'/public/build', 0o700, true);
        mkdir($root.'/storage/app/public', 0o700, true);
        file_put_contents($root.'/public/build/app.js', 'console.log(1)');
        file_put_contents($root.'/public/index.php', '<?php');
        file_put_contents($root.'/public/.env', 'SECRET=1');
        file_put_contents($root.'/storage/app/public/avatar.txt', 'avatar');
        file_put_contents($root.'/secret.txt', 'secret');
        symlink($root.'/storage/app/public', $root.'/public/storage');

        try {
            $client = new PamClient();
            $client->serveStaticFilesFrom($root.'/public', ['build/*' => ['Cache-Control' => 'immutable']]);

            $served = $this->serve($client, '/build/app.js');
            self::assertSame(200, $served['status']);
            self::assertSame('console.log(1)', $served['body'].implode('', $served['chunks']));
            self::assertStringContainsString('javascript', $served['headers']['content-type'][0]);
            self::assertSame(['immutable, public'], $served['headers']['cache-control'] ?? null);

            self::assertSame('avatar', implode('', $this->serve($client, '/storage/avatar.txt')['chunks']));

            foreach (['/index.php', '/.env', '/../secret.txt', '/storage/../../secret.txt', '/missing.js', '/'] as $path) {
                self::assertFalse($this->servable($client, $path), $path);
            }
            self::assertFalse($this->servable($client, '/build/app.js', 'POST'));
        } finally {
            exec('rm -rf '.escapeshellarg($root));
        }
    }

    /** @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} */
    private function serve(PamClient $client, string $path): array
    {
        $target = new PamResponse();
        $request = \Illuminate\Http\Request::create($path);
        $context = new RequestContext(['laravelRequest' => $request, 'pamResponse' => $target]);
        self::assertTrue($client->canServeRequestAsStaticFile($request, $context));
        $client->serveStaticFile($request, $context);

        return $target->export();
    }

    private function servable(PamClient $client, string $path, string $method = 'GET'): bool
    {
        $request = \Illuminate\Http\Request::create($path, $method);

        return $client->canServeRequestAsStaticFile($request, new RequestContext());
    }

    public function testItRejectsAnInvalidRuntimeContext(): void
    {
        $this->expectException(UnexpectedValueException::class);

        (new PamClient())->marshalRequest(new RequestContext());
    }
}
