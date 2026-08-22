<?php

declare(strict_types=1);

namespace Pam\Http {
    final readonly class Request
    {
        /**
         * @param array<string, mixed> $query
         * @param array<string, list<string>> $headers
         */
        public function __construct(
            public string $method,
            public string $path,
            public array $query,
            public array $headers,
            public string $body,
        ) {
        }
    }

    final class Response
    {
        /** @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} */
        public function export(): array
        {
            return [
                'status' => 200,
                'headers' => ['content-type' => ['application/json']],
                'body' => '{"data":{"id":42}}',
                'chunks' => [],
            ];
        }
    }
}

namespace {
    use Pam\Contracts\Http\ApplicationInterface;
    use Pam\Http\Request;
    use Pam\Http\Response;
    use Pam\Http\Testing\TestClient;

    require dirname(__DIR__) . '/vendor/autoload.php';

    $application = new class implements ApplicationInterface {
        public function route(string $method, string $path, callable $handler): self
        {
            return $this;
        }

        public function middleware(object|callable $middleware): self
        {
            return $this;
        }

        public function onError(callable $handler): self
        {
            return $this;
        }

        public function handle(Request $request, Response $response): Response
        {
            if ($request->method !== 'GET' || $request->path !== '/users/42') {
                throw new RuntimeException('Test client did not construct the expected request.');
            }

            return $response;
        }
    };

    $response = (new TestClient($application))->get('/users/42?expand=profile');
    $response->assertSuccessful()->assertJsonPath('data.id', 42);

    if (!class_exists(Pam\Testing\TestClient::class)) {
        throw new RuntimeException('The legacy Pam\\Testing alias is unavailable.');
    }

    fwrite(STDOUT, "pam-http-testing-ok\n");
}
