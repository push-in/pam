# Storage e URLs assinadas

`Nimbus\Storage` organiza discos nomeados sobre o [league/flysystem](https://flysystem.thephpleague.com) v3. O Nimbus não exige o pacote; instale o que o app usar:

```bash
pam composer require league/flysystem                  # discos locais
pam composer require league/flysystem-aws-s3-v3        # S3, MinIO, R2 e compatíveis
```

## Configuração

```php
use Nimbus\Storage\StorageConfig;
use Nimbus\Storage\StorageServiceProvider;

$app->provider(new StorageServiceProvider(
    StorageConfig::fromEnvironment(),
    routes: '/files',   // opcional: rotas assinadas de upload e download para discos locais
));
```

`StorageConfig::fromEnvironment()` define:

- `local`, a partir de `STORAGE_ROOT` (padrão `./storage`) e `STORAGE_URL`;
- `s3`, quando `AWS_BUCKET` existe (`AWS_DEFAULT_REGION`, `AWS_ENDPOINT`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_USE_PATH_STYLE_ENDPOINT`, `AWS_URL`).

`FILESYSTEM_DISK` escolhe o disco padrão (`local`). `APP_URL` prefixa as rotas assinadas. Para mais discos, passe um array de discos ao construtor ou registre qualquer operador flysystem com `StorageManager::filesystem($nome, $operador)`.

## Usar um disco

```php
$disk = $app->storage()->disk();          // disco padrão
$s3 = $app->storage()->disk('s3');

$disk->put('avatars/42.png', $bytes);
$disk->get('avatars/42.png');
$disk->exists('avatars/42.png');
$disk->size('avatars/42.png');
$disk->mimeType('avatars/42.png');
$disk->delete('avatars/42.png');
$disk->url('avatars/42.png');                  // prefixo STORAGE_URL / AWS_URL, ou a URL do objeto no S3
$disk->temporaryUrl('invoices/7.pdf', 300);    // link de download válido por 5 minutos
```

`Nimbus\Storage\Disk` é injetável e resolve para o disco padrão. `Disk::filesystem()` expõe o operador flysystem para listar, mover e mudar visibilidade.

## Upload direto

`temporaryUploadUrl()` devolve uma URL para o cliente enviar o arquivo com `PUT`, sem passar pelo handler:

```php
use Nimbus\Storage\Disk;
use Pam\Http\Response;

$router->post('avatars', static fn (Disk $disk, Response $response): Response => $response->json([
    'upload' => $disk->temporaryUploadUrl('avatars/42.png', ttl: 600, maxBytes: 2_000_000, contentType: 'image/png'),
]));
```

- **S3**: a URL é um `PutObject` pré-assinado pelo AWS SDK. O S3 amarra o content type, mas não limita o tamanho de um PUT pré-assinado; confira `Disk::size()` quando o cliente avisar que terminou.
- **Local**: a URL aponta para `PUT {routes}/upload`, que responde `413` acima de `maxBytes`, `415` para outro content type e `403` para URL adulterada ou vencida; no sucesso grava o corpo e responde `201` com `{disk, path, size}`. O runtime guarda o corpo da requisição até a opção `maxBodyBytes` do listener (2 MiB por padrão); aumente-a para uploads locais maiores ou use S3.

`GET {routes}/download` entrega o arquivo de um `temporaryUrl()` local.

## URLs assinadas

`Nimbus\Security\UrlSigner` assina caminho e query com HMAC-SHA256, usando uma chave derivada do `APP_KEY`. Mudar o caminho, qualquer parâmetro ou a validade invalida a assinatura, que é comparada em tempo constante.

```php
use Nimbus\Security\UrlSigner;

$signer = $app->container()->get(UrlSigner::class);
$link = $signer->temporary('/reports/7', ttl: 3600, query: ['format' => 'csv']);

$router->get('reports/{id}', [ReportController::class, 'show'])->signed();
```

`signed()` vale para uma rota ou para um grupo e responde `403` com o código `InvalidSignature`.
