# Input e validação

## Input tipado

Uma classe que estende `Nimbus\Validation\Input` descreve os campos aceitos: cada propriedade pública é um campo, o tipo PHP é verificado e atributos adicionam regras. O controller recebe o objeto já validado.

```php
namespace App\Http\Requests;

use App\Models\ProductStatus;
use Nimbus\Validation\Attributes\Field;
use Nimbus\Validation\Attributes\Max;
use Nimbus\Validation\Attributes\Min;
use Nimbus\Validation\Input;

final class StoreProductRequest extends Input
{
    #[Max(120)]
    public string $name;

    #[Field('price_in_cents'), Min(1)]
    public int $price;

    public ProductStatus $status = ProductStatus::Active;   // enum com valor int ou string

    public ?string $description;                            // nullable: opcional

    public ?\DateTimeImmutable $publishAt = null;

    public ?ContactInput $contact = null;                   // Input aninhado

    /** @var list<string> */
    #[Max(10)]
    public array $tags = [];
}
```

```php
public function store(StoreProductRequest $request): ProductResource
{
    $product = new Product($request->name, $request->price);
    // ...
}
```

Regras de leitura:

- `GET`, `HEAD` e `DELETE` leem a query string; os outros métodos leem o corpo JSON, `application/x-www-form-urlencoded` ou campos de texto de `multipart/form-data`.
- Campo ausente sem valor padrão é obrigatório, a não ser que o tipo seja nullable.
- A reflexão roda uma vez por classe e por worker.

## Atributos de validação

Todos em `Nimbus\Validation\Attributes`:

| Atributo | Regra |
| --- | --- |
| `#[Min(1)]`, `#[Max(120)]` | mínimo e máximo: número, caracteres de string ou itens de lista |
| `#[Email]` | e-mail válido |
| `#[Url]` | URL válida |
| `#[Uuid]` | UUID |
| `#[Regex('/^[A-Z]{3}$/')]` | expressão regular |
| `#[In('recent', 'price')]` | um dos valores |
| `#[Field('price_in_cents')]` | lê o campo de outra chave do payload |

Uma regra própria é um atributo que implementa `Nimbus\Validation\ValidationRule` e devolve a mensagem de erro, ou `null` quando o valor é válido:

```php
use Nimbus\Validation\ValidationRule;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class Cpf implements ValidationRule
{
    public function validate(string $field, mixed $value): ?string
    {
        return is_string($value) && preg_match('/^\d{11}$/', $value) === 1
            ? null
            : "The {$field} field must be a CPF with 11 digits.";
    }
}
```

## Autorização no Input

Sobrescreva `authorize()` para recusar a requisição com `403` antes de validar:

```php
use Pam\Http\Request;

final class PublishProductRequest extends Input
{
    public function authorize(Request $request): bool
    {
        return $request->getHeader('x-team') === 'catalog';
    }
}
```

## Fora do HTTP

`Input::from($array)` monta e valida a partir de um array, útil em jobs e comandos. `Input::schema()` devolve o JSON Schema dos campos, usado pelo [OpenAPI](./openapi.md).

```php
$input = StoreProductRequest::from(['name' => 'Teclado', 'price_in_cents' => 25_000]);
```

## Resposta de erro

Falhas viram `Nimbus\Validation\ValidationException`, respondida com `422` em problem+json. O membro `errors` lista as mensagens por campo:

```json
{
  "errors": {
    "name": ["The name field is required."],
    "price_in_cents": ["The price_in_cents field must be at least 1."]
  },
  "type": "https://pam.dev/problems/1",
  "title": "The submitted data is invalid.",
  "status": 422,
  "code": 1
}
```

Para lançar o mesmo erro a partir do seu código:

```php
throw new ValidationException(['email' => ['Este e-mail já está cadastrado.']]);
```

## FormRequest com regras em array

`Nimbus\Validation\FormRequest` é a alternativa com regras declaradas num array, para quem vem do Laravel. As regras nativas são `required`, `string`, `integer`, `boolean`, `array` e `email`, mais `Rule::enum()` e qualquer objeto `ValidationRule`. `validated()` devolve só os campos declarados.

```php
use Nimbus\Validation\FormRequest;
use Nimbus\Validation\Rule;

final class LoginRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'email' => ['required', 'string', 'email'],
            'type' => ['required', Rule::enum(UserType::class)],
        ];
    }
}

public function store(LoginRequest $request): AuthResource
{
    return new AuthResource($this->login->handle($request->validated()));
}
```

Prefira `Input` em código novo: os campos ficam tipados e entram no OpenAPI.
