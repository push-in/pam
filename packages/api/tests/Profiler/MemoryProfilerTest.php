<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Profiler;

use Pam\App;
use Pam\Api\Profiler\MemoryProfiler;
use Pam\Api\Profiler\ProfilerMode;
use Pam\Api\Profiler\RequestProfile;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(MemoryProfiler::class)]
#[CoversClass(ProfilerMode::class)]
#[CoversClass(RequestProfile::class)]
final class MemoryProfilerTest extends TestCase
{
    public function testItProfilesRequestsWithoutCapturingPayloads(): void
    {
        $profiler = new MemoryProfiler(ProfilerMode::Testing);
        $app = new App(discoverPackages: false);
        $app->observe($profiler);
        $app->post('/login', static fn (Request $request, Response $response): Response =>
            $response->json(['secret' => 'response-secret']));

        $result = $app->handle(
            new Request('POST', '/login', [], ['authorization' => ['Bearer secret']], '{"password":"secret"}'),
            new Response(),
        )->export();
        $token = $result['headers']['x-debug-token'][0];
        $profile = $profiler->find($token);

        self::assertInstanceOf(RequestProfile::class, $profile);
        self::assertSame('POST', $profile->method);
        self::assertSame('/login', $profile->path);
        self::assertSame(200, $profile->statusCode);
        self::assertNull($profile->failureClass);
        self::assertStringNotContainsString('secret', serialize($profile));
    }

    public function testDisabledProfilerEmitsNoHeaderOrProfile(): void
    {
        $profiler = new MemoryProfiler();
        $app = new App(discoverPackages: false);
        $app->observe($profiler);
        $app->get('/', static fn (Request $request, Response $response): Response => $response->send('ok'));

        $result = $app->handle(new Request('GET', '/', [], [], ''), new Response())->export();

        self::assertArrayNotHasKey('x-debug-token', $result['headers']);
        self::assertSame([], $profiler->recent());
    }

    public function testItRecordsHandledFailureClassAndEvictsOldProfiles(): void
    {
        $profiler = new MemoryProfiler(ProfilerMode::Testing, maximumProfiles: 1);
        $app = new App(discoverPackages: false);
        $app->observe($profiler);
        $app->get('/failure', static function (): never {
            throw new \RuntimeException('secret failure detail');
        });

        $first = $app->handle(new Request('GET', '/failure', [], [], ''), new Response())->export();
        $second = $app->handle(new Request('GET', '/failure', [], [], ''), new Response())->export();

        self::assertNull($profiler->find($first['headers']['x-debug-token'][0]));
        $profile = $profiler->find($second['headers']['x-debug-token'][0]);
        self::assertInstanceOf(RequestProfile::class, $profile);
        self::assertSame(\RuntimeException::class, $profile->failureClass);
        self::assertStringNotContainsString('secret failure detail', serialize($profile));
    }
}
