<?php

declare(strict_types=1);

namespace Nimbus\Tests\Http\Client;

use Nimbus\Http\Client\ConnectionException;
use Nimbus\Http\Client\Factory;
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\HttpFake;
use Nimbus\Http\Client\OutgoingRequest;
use Nimbus\Http\Client\PendingRequest;
use Nimbus\Http\Client\Pool;
use Nimbus\Http\Client\RequestException;
use Nimbus\Http\Client\Response;
use Nimbus\Http\Client\ResponseSequence;
use Nimbus\Http\Client\SymfonyTransport;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;
use Symfony\Component\HttpClient\MockHttpClient;
use Symfony\Component\HttpClient\Response\MockResponse;

#[CoversClass(Http::class)]
#[CoversClass(Factory::class)]
#[CoversClass(PendingRequest::class)]
#[CoversClass(Pool::class)]
#[CoversClass(HttpFake::class)]
#[CoversClass(ResponseSequence::class)]
#[CoversClass(Response::class)]
#[CoversClass(OutgoingRequest::class)]
#[CoversClass(SymfonyTransport::class)]
#[CoversClass(RequestException::class)]
final class HttpClientTest extends TestCase
{
    protected function tearDown(): void
    {
        Http::swap(null);
        parent::tearDown();
    }

    public function testItSendsJsonWithTokenAndAssertsTheRequest(): void
    {
        $http = Http::fake(['api.example.test/users/*' => ['id' => 7, 'data' => ['name' => 'Ana']]]);

        $response = Http::withToken('secret')->timeout(5)->post('https://api.example.test/users/7', ['name' => 'Ana']);

        self::assertTrue($response->ok());
        self::assertSame(7, $response->json('id'));
        self::assertSame('Ana', $response->json('data.name'));
        self::assertNull($response->json('data.missing'));
        self::assertSame('application/json', $response->header('Content-Type'));
        $http->assertSent(static fn (OutgoingRequest $request): bool => $request->method() === 'POST'
            && $request->hasHeader('authorization', 'Bearer secret')
            && $request->header('content-type') === 'application/json'
            && $request->data() === ['name' => 'Ana']
            && $request->timeout === 5.0);
        $http->assertSentCount(1);
        $http->assertNotSent(static fn (OutgoingRequest $request): bool => $request->method() === 'GET');
    }

    public function testFormsQueriesAndBaseUrls(): void
    {
        $http = Http::fake();

        Http::asForm()->post('https://example.test/token', ['grant_type' => 'client_credentials']);
        Http::baseUrl('https://example.test/v1/')->withQueryParameters(['key' => 'k'])->get('search', ['q' => 'a b']);

        [$form, $search] = array_column($http->recorded(), 0);
        self::assertSame('grant_type=client_credentials', $form->body);
        self::assertSame('application/x-www-form-urlencoded', $form->header('content-type'));
        self::assertSame('https://example.test/v1/search?q=a%20b&key=k', $search->url);
        self::assertSame(['q' => 'a b', 'key' => 'k'], $search->data());
    }

    public function testFailedResponsesAreReturnedUntilThrown(): void
    {
        Http::fake(['*' => Http::response(['error' => 'nope'], 422)]);

        $response = Http::get('https://example.test/x');
        self::assertTrue($response->clientError());
        self::assertTrue($response->failed());

        $this->expectException(RequestException::class);
        $this->expectExceptionCode(422);
        $response->throw();
    }

    public function testItRetriesServerErrorsAndConnectionFailures(): void
    {
        $http = Http::fake([
            'example.test/flaky' => Http::sequence()->pushStatus(503)->pushFailedConnection()->push(['ok' => true]),
        ]);

        $response = Http::retry(3)->get('https://example.test/flaky');

        self::assertSame(['ok' => true], $response->json());
        $http->assertSentCount(3);
    }

    public function testExhaustedRetriesThrow(): void
    {
        Http::fake(['*' => Http::response('', 500)]);

        try {
            Http::retry(2, [0])->get('https://example.test/down');
            self::fail('The retries should throw.');
        } catch (RequestException $failure) {
            self::assertSame(500, $failure->response->status());
        }

        $response = Http::retry(2, 0, throw: false)->get('https://example.test/down');
        self::assertTrue($response->serverError());
    }

    public function testRetryWhenDecidesWhatIsRetried(): void
    {
        $http = Http::fake(['*' => Http::response('', 409)]);

        Http::retry(3, 0, static fn (Response|ConnectionException $outcome): bool => $outcome instanceof Response
            && $outcome->status() === 409, throw: false)->get('https://example.test/conflict');

        $http->assertSentCount(3);
    }

    public function testConnectionFailuresThrow(): void
    {
        Http::fake(['*' => Http::failedConnection('refused')]);

        $this->expectException(ConnectionException::class);
        Http::get('https://example.test/');
    }

    public function testPoolsKeyResponses(): void
    {
        $http = Http::fake([
            'example.test/a' => ['name' => 'a'],
            'example.test/b' => Http::failedConnection(),
            '*' => static fn (OutgoingRequest $request): Response => Http::response($request->header('authorization') ?? '', 201),
        ]);

        $responses = Http::pool(static fn (Pool $pool): array => [
            $pool->as('first')->get('https://example.test/a'),
            $pool->as('broken')->get('https://example.test/b'),
            $pool->with(static fn (PendingRequest $request): PendingRequest => $request->withToken('t'))->post('https://example.test/c'),
        ]);

        self::assertInstanceOf(Response::class, $responses['first']);
        self::assertSame('a', $responses['first']->json('name'));
        self::assertInstanceOf(ConnectionException::class, $responses['broken']);
        self::assertInstanceOf(Response::class, $responses[0]);
        self::assertSame('Bearer t', $responses[0]->body());
        $http->assertSentCount(3);
    }

    public function testStrayRequestsCanBePrevented(): void
    {
        Http::fake(['example.test/known' => 'ok'])->preventStrayRequests();

        self::assertSame('ok', Http::get('https://example.test/known')->body());
        $this->expectException(\LogicException::class);
        Http::get('https://other.test/');
    }

    public function testNothingSentAssertion(): void
    {
        Http::fake()->assertNothingSent();
    }

    public function testSymfonyTransportSendsAndMultiplexes(): void
    {
        $seen = [];
        $mock = new MockHttpClient(static function (string $method, string $url, array $options) use (&$seen): MockResponse {
            $seen[] = $method . ' ' . $url;
            if (str_ends_with($url, '/down')) {
                return new MockResponse('', ['error' => 'connection refused']);
            }
            return new MockResponse('{"url":"' . $url . '"}', ['http_code' => 202, 'response_headers' => ['X-Trace: 1']]);
        });
        $http = (new Factory(new SymfonyTransport($mock)))->request();

        $response = $http->withHeader('X-Test', '1')->put('https://example.test/item', ['a' => 1]);
        self::assertSame(202, $response->status());
        self::assertSame('1', $response->header('x-trace'));
        self::assertSame('https://example.test/item', $response->json('url'));

        $results = $http->pool(static function (Pool $pool): void {
            $pool->as('up')->get('https://example.test/up');
            $pool->as('down')->get('https://example.test/down');
        });
        self::assertInstanceOf(Response::class, $results['up']);
        self::assertInstanceOf(ConnectionException::class, $results['down']);
        self::assertSame(['PUT https://example.test/item', 'GET https://example.test/up', 'GET https://example.test/down'], $seen);

        $this->expectException(ConnectionException::class);
        $http->get('https://example.test/down');
    }

    public function testInvalidRequestsAreRejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        Http::fake();
        Http::get('ftp://example.test/');
    }
}
