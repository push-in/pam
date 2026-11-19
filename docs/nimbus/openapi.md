# OpenAPI e clientes gerados

O Nimbus gera o documento OpenAPI 3 a partir das rotas registradas. Metadados opcionais em cada rota descrevem entrada e saída:

```php
$router->post('users', [UserController::class, 'store'])
    ->name('users.store')
    ->summary('Create a user')
    ->tags(['Users'])
    ->input(StoreUserRequest::class)     // um Input: vira o requestBody pelo Input::schema()
    ->output(UserResource::class);       // o JsonResource da resposta
```

## Servir a documentação: `docs()`

```php
$app->docs();                                       // GET /docs e GET /docs/openapi.json
$app->docs('/api/docs', 'Loja API', '2.1.0');       // caminho, título e versão
```

`/docs/openapi.json` devolve o documento, gerado na primeira requisição e mantido enquanto o worker viver. `/docs` é uma página HTML que carrega a referência interativa da Scalar a partir do CDN jsDelivr. Em produção, proteja ou omita essas rotas se a API não for pública.

## Gerar o documento e clientes

```php
use Nimbus\OpenApi\ClientLanguage;

$contract = $app->openApi('Loja API', '1.0.0');

$array = $contract->generate();
file_put_contents('openapi.json', $contract->toJson());

$typescript = $contract->client(ClientLanguage::TypeScript);
$kotlin = $contract->client(ClientLanguage::Kotlin);
$swift = $contract->client(ClientLanguage::Swift);
```

## Detectar mudanças incompatíveis

`CompatibilityChecker` compara dois documentos e lista remoções de caminhos e operações, cada uma com um `CompatibilityChangeCode` inteiro e estável. Rode no CI contra o documento da última versão publicada:

```php
use Nimbus\OpenApi\CompatibilityChecker;

$previous = json_decode(file_get_contents('openapi.released.json'), true, flags: JSON_THROW_ON_ERROR);
$changes = (new CompatibilityChecker())->breakingChanges($previous, $app->openApi()->generate());

if ($changes !== []) {
    exit(1);
}
```
