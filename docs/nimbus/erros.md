# Erros: problem+json e `ProblemCode`

Erros de rota, `HttpException` e exceções não tratadas usam o mesmo formato Problem Details (RFC 9457), com `content-type: application/problem+json`:

```json
{"type":"https://pam.dev/problems/5","title":"Version conflict.","status":409,"code":5}
```

- `title` é a mensagem da exceção.
- `code` é o número estável do `ProblemCode`; `type` aponta para ele.
- Campos extras (`details`) entram no corpo, mas não substituem `type`, `title`, `status` nem `code`.
- A mensagem de uma exceção inesperada vai para o log e nunca para o cliente; ele recebe `500` com `InternalError`.

## Lançar um erro

```php
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;

throw new HttpException(404, ProblemCode::NotFound, 'Product not found.');

throw new HttpException(409, ProblemCode::Conflict, 'Version conflict.', ['current_version' => 7]);
```

Assinatura: `new HttpException(int $status, ProblemCode $problemCode, string $message, array $details = [], ?Throwable $previous = null)`.

## Códigos

`Nimbus\Http\ProblemCode` é um enum com valores inteiros sequenciais. Clientes devem decidir pelo `code`, não pelo texto.

| Código | Caso | Uso típico |
| ---: | --- | --- |
| 1 | `ValidationFailed` | 422, validação ([Input](./validacao.md)) |
| 2 | `Unauthenticated` | 401 |
| 3 | `Forbidden` | 403 |
| 4 | `NotFound` | 404, inclusive rota inexistente |
| 5 | `Conflict` | 409 |
| 6 | `RateLimited` | 429 |
| 7 | `Timeout` | 504, prazo da requisição estourado (`DeadlineMiddleware`) |
| 8 | `InternalError` | 500 |
| 9 | `MethodNotAllowed` | 405, com cabeçalho `Allow` |
| 10 | `InvalidSignature` | 403, URL assinada inválida ou expirada |
| 11 | `PayloadTooLarge` | 413 |
| 12 | `UnsupportedMediaType` | 415 |
| 13 | `CsrfTokenMismatch` | 419, token CSRF ausente ou errado |

## Validação

`ValidationException` é uma `HttpException` com status `422`, código `1` e o membro `errors`:

```json
{
  "errors": {"email": ["The email field must be a valid email address."]},
  "type": "https://pam.dev/problems/1",
  "title": "The submitted data is invalid.",
  "status": 422,
  "code": 1
}
```

Em apps web com sessão, uma `ValidationException` numa requisição HTML vira redirect de volta com os erros e o input antigo na sessão (veja [Sessão](./sessao.md)).

## Tratador próprio

`$app->onError()` substitui o tratamento das exceções que não são `HttpException`. Veja [Controllers](./controllers.md#tratamento-de-erros).
