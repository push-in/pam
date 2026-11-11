<?php

namespace Laravel\Octane\Tests;

use Illuminate\Http\Request;
use Laravel\Octane\OctaneResponse;
use Laravel\Octane\RequestContext;
use Pam\Http\Request as PamRequest;
use Pam\Http\Response as PamResponse;
use Pam\Octane\PamClient;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpFoundation\StreamedResponse;

/**
 * Mirrors FrankenPhpClientTest / RoadRunnerClientTest for the PAM client.
 */
class PamClientTest extends TestCase
{
    private function context(string $method = 'GET', string $uri = '/home?name=Taylor', string $body = ''): RequestContext
    {
        $_GET = ['name' => 'Taylor'];
        $_POST = $_COOKIE = $_FILES = [];
        $_SERVER = [
            'REQUEST_METHOD' => $method,
            'REQUEST_URI' => $uri,
            'QUERY_STRING' => 'name=Taylor',
            'SERVER_NAME' => 'localhost',
            'SERVER_PORT' => '8000',
            'HTTP_HOST' => 'localhost:8000',
            'HTTP_ACCEPT' => 'text/html',
        ];

        return new RequestContext([
            'pamRequest' => new PamRequest($method, '/home', ['name' => 'Taylor'], ['host' => ['localhost:8000']], $body),
            'pamResponse' => new PamResponse(),
        ]);
    }

    public function test_marshal_request()
    {
        [$request, $context] = (new PamClient())->marshalRequest($context = $this->context());

        $this->assertInstanceOf(Request::class, $request);
        $this->assertSame('/home', $request->getPathInfo());
        $this->assertSame('Taylor', $request->query('name'));
        $this->assertSame('localhost:8000', $request->getHttpHost());
        $this->assertSame($request, $context['laravelRequest']);
    }

    public function test_marshal_request_keeps_the_raw_body()
    {
        [$request] = (new PamClient())->marshalRequest($this->context('POST', '/home', '{"a":1}'));

        $this->assertSame('{"a":1}', $request->getContent());
    }

    public function test_response()
    {
        $client = new PamClient();
        [, $context] = $client->marshalRequest($context = $this->context());

        $client->respond($context, new OctaneResponse(new Response('Hello World', 201, ['X-Test' => 'yes']), 'prefix:'));

        $export = $context['pamResponse']->export();
        $this->assertSame(201, $export['status']);
        $this->assertSame(['yes'], $export['headers']['x-test']);
        $this->assertSame('prefix:Hello World', $export['body']);
    }

    public function test_streamed_generator_response_flushes_each_yielded_chunk()
    {
        $client = new PamClient();
        [, $context] = $client->marshalRequest($this->context());

        $client->respond($context, new OctaneResponse(new StreamedResponse(function () {
            yield 'Hello';
            yield 'World';
        })));

        $this->assertSame(['Hello', 'World'], $context['pamResponse']->export()['chunks']);
    }

    public function test_error_method_sends_error_response_to_pam()
    {
        $client = new PamClient();
        [$request, $context] = $client->marshalRequest($this->context());
        $app = $this->createApplication();
        $app['config']['app.debug'] = false;

        $client->error(new \Exception('Something went wrong...'), $app, $request, $context);

        $export = $context['pamResponse']->export();
        $this->assertSame(500, $export['status']);
        $this->assertSame('Internal Server Error', $export['body']);
    }

    public function test_error_method_sends_detailed_error_response_to_pam_in_debug_mode()
    {
        $client = new PamClient();
        [$request, $context] = $client->marshalRequest($this->context());
        $app = $this->createApplication();
        $app['config']['app.debug'] = true;

        $client->error($e = new \Exception('Something went wrong...'), $app, $request, $context);

        $this->assertStringContainsString('Something went wrong...', $context['pamResponse']->export()['body']);
    }
}
