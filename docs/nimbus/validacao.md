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

## FormRequest com regras do Laravel

`Nimbus\Validation\FormRequest` recebe as regras no formato do Laravel, para portar um `FormRequest` sem reescrever: strings com `|` ou listas de strings e objetos `ValidationRule`. Os dados validados são o corpo (JSON ou formulário) somado à query string, com o corpo prevalecendo, como no Laravel.

```php
use Nimbus\Validation\FormRequest;
use Nimbus\Validation\Rule;

final class AskRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'hours' => 'nullable|integer|between:1,720',
            'since' => ['sometimes', 'date'],
            'type' => ['required', Rule::enum(UserType::class)],
            'items' => 'required|array|min:1',
            'items.*.sku' => ['required', 'string', Rule::exists('products', 'sku')],
            'email' => ['required', 'email', Rule::unique('users', 'email')->ignore($this->request->route('user'))],
        ];
    }

    public function messages(): array
    {
        return ['since.date' => 'Use o formato AAAA-MM-DD.', 'items.*.sku.exists' => 'Produto :value não existe.'];
    }

    public function attributes(): array
    {
        return ['since' => 'data inicial'];
    }
}

public function store(AskRequest $request): AuthResource
{
    return new AuthResource($this->ask->handle($request->validated()));
}
```

Regras disponíveis:

| Grupo | Regras |
| --- | --- |
| Presença | `required`, `required_if:campo,valor`, `required_unless`, `required_with:a,b`, `required_without`, `present`, `filled`, `nullable`, `sometimes`, `bail` |
| Tipos | `string`, `integer`, `numeric`, `boolean`, `array`, `list`, `email`, `url`, `uuid`, `ip`, `json`, `alpha`, `alpha_num`, `alpha_dash`, `accepted` |
| Tamanho | `min`, `max`, `between`, `size`, `digits` (número, caracteres ou itens, conforme o valor) |
| Datas | `date`, `date_format:Y-m-d`, `after`, `after_or_equal`, `before`, `before_or_equal` (data, `tomorrow` ou outro campo) |
| Valores | `in:a,b`, `not_in`, `regex:/.../`, `not_regex`, `confirmed`, `same:campo`, `different:campo`, `Rule::in()`, `Rule::notIn()`, `Rule::enum()` |
| Banco | `exists:tabela,coluna`, `unique:tabela,coluna,exceto,colunaId`, `Rule::exists()->where()`, `Rule::unique()->ignore()` |

- Chaves com ponto (`customer.email`) e curinga (`items.*.sku`) valem; o erro sai no caminho concreto (`items.1.sku`) e `validated()` devolve a mesma estrutura aninhada, só com os campos que têm regra.
- Como no Laravel, campos ausentes ou com string vazia só passam pelas regras implícitas (`required*`, `present`, `accepted`), e a primeira falha de uma regra implícita encerra as regras do campo.
- `integer`, `numeric` e `boolean` aceitam os valores em string da query e do formulário (`"24"`, `"1"`) e `validated()` devolve o tipo PHP (`24`, `true`). O Laravel devolve a string.
- As mensagens são as padrão do Laravel em inglês; `messages()` troca por `campo.regra`, `campo` ou `regra` (com curinga), com `:attribute`, `:value`, `:min`, `:max`, `:size`, `:values`, `:other`, `:date` e `:format`. `attributes()` dá o nome exibido em `:attribute`.
- `exists` e `unique` contam linhas na conexão do Arcano (`conexao.tabela` escolhe a conexão). O `ArcanoServiceProvider` registra o verificador; fora dele, chame `Validator::usePresenceVerifier()`.

Fora de um request, `Validator::make($dados, $regras, $mensagens)->validate()` devolve os dados validados ou lança `ValidationException`; `fails()` e `errors()` não lançam.

Prefira `Input` em código novo: os campos ficam tipados e entram no OpenAPI.
