# Cliente HTTP

`Nimbus\Http\Client` chama APIs externas com uma interface fluente. Dentro do PAM, uma requisição suspende só a Fiber da requisição atual e o worker continua atendendo as outras enquanto espera; fora do PAM (um script, o PHPUnit sem o runtime), o mesmo código usa o `symfony/http-client`.

## Uma requisição

```php
use Nimbus\Http\Client\Http;

$response = Http::withToken($token)
    ->acceptJson()
    ->timeout(5)
    ->post('https://api.example.com/orders', ['sku' => 'A1', 'quantity' => 2]);

if ($response->successful()) {
    $orderId = $response->json('data.id');
}
```

Métodos de envio: `get($url, $query)`, `head($url, $query)`, `post($url, $data)`, `put`, `patch`, `delete` e `send($method, $url, $data, $query)`. O `$data` de `post`, `put`, `patch` e `delete` vai como JSON por padrão.

## Configurar o pedido

Cada método devolve uma cópia (`PendingRequest` é imutável), então um pedido configurado pode ficar num serviço e ser usado por requisições concorrentes:

```php
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\PendingRequest;

final readonly class GithubApi
{
    private PendingRequest $http;

    public function __construct(string $token)
    {
        $this->http = Http::baseUrl('https://api.github.com/')
            ->withToken($token)
            ->withHeaders(['x-github-api-version' => '2022-11-28'])
            ->acceptJson()
            ->timeout(10)
            ->retry(3, 200);
    }

    public function user(string $login): array
    {
        return $this->http->get('users/' . rawurlencode($login))->throw()->array();
    }
}
```

| Método | Efeito |
| --- | --- |
| `baseUrl($url)` | prefixo das URLs relativas |
| `withHeaders([...])`, `withHeader($nome, $valor)` | cabeçalhos |
| `withToken($token, $tipo = 'Bearer')`, `withBasicAuth($usuário, $senha)` | `Authorization` |
| `withUserAgent($ua)`, `accept($tipo)`, `acceptJson()` | cabeçalhos comuns |
| `withQueryParameters([...])` | query string somada a toda requisição |
| `asJson()` (padrão), `asForm()` | corpo em JSON ou `application/x-www-form-urlencoded` |
| `withBody($corpo, $contentType)` | corpo cru; ignora o `$data` da chamada |
| `timeout($segundos)` | tempo total de cada tentativa, redirecionamentos incluídos (padrão 30) |
| `withoutRedirecting()`, `maxRedirects($n)` | redirecionamentos (padrão 5) |
| `withoutVerifying()` | não valida o certificado TLS; só para desenvolvimento |
| `retry(...)` | novas tentativas, abaixo |
| `throw()` | toda resposta 4xx/5xx desse pedido lança `RequestException` |

`Http::request()` devolve um `PendingRequest` sem configuração. A fachada `Http` tem atalhos estáticos para os métodos mais usados (`withToken`, `baseUrl`, `asForm`, `timeout`, `retry`, `get`, `post`...).

## Respostas

`Nimbus\Http\Client\Response`:

| Método | Devolve |
| --- | --- |
| `status()`, `body()` | status e corpo cru |
| `json()`, `json('data.items.0.id', $default)` | JSON decodificado, ou o valor numa chave com pontos |
| `array()` | o objeto ou lista JSON; `[]` para qualquer outro corpo |
| `header($nome)`, `headers()` | cabeçalhos |
| `successful()` (2xx), `ok()` (200), `redirect()` (3xx) | |
| `failed()`, `clientError()` (4xx), `serverError()` (5xx) | |
| `throw()` | lança `RequestException` em 4xx/5xx; senão devolve a própria resposta |

Respostas 4xx e 5xx são devolvidas, não lançadas, até você chamar `throw()`. `RequestException` traz a resposta em `$exception->response`. Quando nenhuma resposta chega (DNS, conexão, TLS, timeout), o envio lança `Nimbus\Http\Client\ConnectionException`.

```php
use Nimbus\Http\Client\ConnectionException;
use Nimbus\Http\Client\RequestException;

try {
    $charge = Http::withToken($key)->post('https://pay.example.com/charges', $payload)->throw()->json();
} catch (RequestException $error) {
    $log->warning('charge.rejected', ['status' => $error->response->status()]);
    throw $error;
} catch (ConnectionException $error) {
    // sem resposta: pode tentar de novo mais tarde
}
```

## Novas tentativas

```php
Http::retry(3, 100)->get($url);                       // 3 tentativas no total, 100 ms entre elas
Http::retry(4, [100, 500, 2_000])->get($url);         // espera por tentativa
Http::retry(5, fn (int $attempt) => 100 * 2 ** $attempt)->get($url);
```

Por padrão `retry()` tenta de novo em falha de conexão, `5xx` e `429`. O terceiro argumento troca o critério; ele recebe a resposta (ou a `ConnectionException`) e o `OutgoingRequest`:

```php
use Nimbus\Http\Client\ConnectionException;
use Nimbus\Http\Client\OutgoingRequest;
use Nimbus\Http\Client\Response;

Http::retry(3, 200, fn (Response|ConnectionException $outcome, OutgoingRequest $request): bool =>
    $outcome instanceof ConnectionException || $outcome->status() === 503
)->get($url);
```

Esgotadas as tentativas, uma resposta de erro lança `RequestException`; com `throw: false` a última resposta é devolvida. Dentro do PAM a espera suspende só a Fiber da requisição. Só use `retry()` em operações que podem repetir sem efeito duplicado (GET, ou POST com chave de idempotência do serviço chamado).

## Requisições em paralelo

```php
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\Pool;

$responses = Http::pool(fn (Pool $pool) => [
    $pool->as('user')->get('https://api.example.com/users/7'),
    $pool->as('orders')->with(fn ($request) => $request->withToken($token))->get('https://api.example.com/orders', ['user' => 7]),
]);

$user = $responses['user']->json();
```

As respostas voltam pelas chaves de `as()` (ou pela ordem, sem `as()`). `with()` configura só a próxima requisição do pool. Uma requisição que não teve resposta aparece como `ConnectionException` no array, sem lançar. No PAM cada requisição roda numa Fiber irmã; com o Symfony, elas são multiplexadas.

## Transporte

`Http` mantém uma `Nimbus\Http\Client\Factory` por worker, com o transporte detectado: `PamTransport` dentro do PAM (HTTP/1.1, respostas até 16 MiB) e `SymfonyTransport` fora dele. Para injetar o cliente no lugar de usar a fachada:

```php
use Nimbus\Http\Client\Factory;
use Nimbus\Http\Client\Http;

$app->container()->instance(Factory::class, Http::factory());

final readonly class Weather
{
    public function __construct(private Factory $http) {}

    public function today(string $city): array
    {
        return $this->http->request()->get('https://weather.example.com/today', ['city' => $city])->throw()->array();
    }
}
```

`new Factory($transport)` aceita qualquer `Nimbus\Http\Client\Transport`, e `Http::swap($factory)` troca a factory da fachada (`null` volta à detectada).

## Testes

`Http::fake()` troca o transporte por um `HttpFake`, que grava os pedidos e responde com stubs:

```php
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\OutgoingRequest;

public function test_imports_the_github_user(): void
{
    $http = $this->fakeHttp([
        'api.github.com/users/*' => ['login' => 'octocat', 'id' => 1],
        'api.example.com/flaky' => Http::sequence()->pushStatus(503)->push(['ok' => true]),
        'api.example.com/down' => Http::failedConnection(),
        '*' => Http::response('', 404),
    ]);
    $http->preventStrayRequests();

    $this->postJson('/api/imports', ['login' => 'octocat'])->assertCreated();

    $http->assertSent(fn (OutgoingRequest $request) =>
        $request->method() === 'GET'
        && str_ends_with($request->url(), '/users/octocat')
        && $request->hasHeader('authorization', 'Bearer test-token'));
    $http->assertSentCount(1);
}
```

Formas de stub, por padrão de URL (`*` casa qualquer trecho, e um `*` no início é implícito):

- array: corpo JSON com status 200;
- string: corpo cru;
- `Http::response($corpo, $status, $cabeçalhos)`;
- `Http::sequence()`: respostas em ordem (`push`, `pushStatus`, `pushResponse`, `pushFailedConnection`, `whenEmpty`);
- `Http::failedConnection($mensagem)`: lança `ConnectionException`;
- closure: recebe o `OutgoingRequest` e devolve uma das formas acima. Uma closure sozinha, sem array, responde todas as URLs.

Uma URL sem stub recebe `200` vazio, ou lança depois de `preventStrayRequests()`. Asserções: `assertSent`, `assertNotSent`, `assertSentCount`, `assertNothingSent`; `recorded()` lista pedidos e respostas. No callback, `OutgoingRequest` tem `method()`, `url()`, `body()`, `header($nome)`, `hasHeader($nome, $valor)` e `data()` (o corpo JSON ou de formulário como array).

Em `Nimbus\Testing\TestCase`, prefira `$this->fakeHttp()`: além da fachada, ele troca a `Factory` registrada no container. O `TestCase` restaura a fachada no fim de cada teste.
