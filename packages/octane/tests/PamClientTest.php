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

    public function testItRejectsAnInvalidRuntimeContext(): void
    {
        $this->expectException(UnexpectedValueException::class);

        (new PamClient())->marshalRequest(new RequestContext());
    }
}
