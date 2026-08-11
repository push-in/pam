# Compatibilidade com Composer

Pam usa Composer sem wrappers de pacote e sem lockfile próprio. O autoloader é
descoberto a partir do entrypoint, inclusive quando `config.vendor-dir` aponta para
outro diretório.

## Contrato

| Categoria | Situação |
| --- | --- |
| Pacotes pure PHP e PSR-4 | Compatíveis pelo autoloader normal |
| `pushinbr/pam-api` | Rotas e middleware opcionais instalados pelo Composer |
| `pushinbr/pam-socket` | Eventos e rooms opcionais sobre o transporte WebSocket nativo |
| PSR-7, PSR-15 e PSR-17 | Fornecidos por `pushinbr/pam-psr-bridge` com as interfaces oficiais |
| PSR-3 | Consumido quando `psr/log` está instalado |
| PHPUnit e Pest | Executados dentro da Embed SAPI por `pam test` |
| Amp, Revolt e ReactPHP | Carregados e exercitados por timers/futures na suíte smoke |
| Guzzle, Monolog e OpenTelemetry | Exercitados pela suíte smoke |
| Illuminate 13 | Container, Events e Pipeline exercitados por comportamento |
| Symfony 8 | HttpFoundation + HttpKernel exercitados com dispatch real |
| Slim 4 | App, routing e PSR-7 exercitados com request real |
| Extensões PHP | Compatíveis quando carregadas pela Embed SAPI |
| Bibliotecas síncronas | Funcionam, mas bloqueiam aquele worker durante a chamada |
| Código dependente de SAPI específica | Deve ser validado; `PHP_SAPI` é `embed` |

“Compatível com Composer” não significa que toda biblioteca existente possa ter
seu comportamento garantido. Pacotes podem exigir uma extensão ausente, assumir
FPM/Apache/CLI, instalar handlers globais incompatíveis com processo persistente ou
manter estado de request em singletons. Execute sempre:

```bash
pam composer install
pam doctor
pam test
```

`doctor` valida o Embed, seus INIs, o autoloader e o platform check gerado pelo
Composer dentro do próprio PAM. Se um PHP CLI existir, compara também versão/ABI,
ZTS, debug build, integer size e extensões. O CLI é diagnóstico opcional, não uma
dependência do runtime. Quando a comparação existe, diferença de extensões é
aviso explícito — ela não é escondida. Requisitos declarados pelo projeto
continuam sendo validados no próprio Embed pelo platform check do Composer.

## Aplicações persistentes

O entrypoint e objetos globais vivem por várias requisições. Evite armazenar
Request, Response, usuário autenticado, transação ou tenant em propriedades
estáticas. Pam limpa superglobais, headers, sessões e uploads, mas não pode
adivinhar estado criado pela aplicação ou por um pacote.

Para uma biblioteca bloqueante, há três estratégias:

1. usar a variante Amp/Revolt/non-blocking do pacote;
2. executar em `Pam\Task\ProcessPool`;
3. dimensionar múltiplos workers, aceitando que uma chamada ocupe um worker.

O projeto `compat/composer-smoke` é o contrato executável e possui lockfile. Antes
de ampliar a matriz, faça preflight da versão e compatibilidade e só depois altere
as dependências.

O segundo contrato, `compat/laravel-smoke`, executa Laravel 12 e 13 no host
persistente `Pam\Laravel`, valida o bridge com PHPStan nível 9 e testa isolamento
do container e estabilidade de RSS. A matriz cobre SQLite, MySQL, PostgreSQL,
Redis, fila database, Artisan, scheduler, Eloquent, auth/Sanctum, sessão/CSRF,
uploads, Flysystem, Blade, Livewire, Inertia, Scout, Reverb e gravação real de
Telescope/Pulse. Também valida injeção request-scoped em Events/Bus, locale,
serialização segura por worker, streaming progressivo, cancelamento por
desconexão, limites de resposta, downloads `Range` e `HEAD`. As classes de
integração pertencem ao binário; nenhum fork ou pacote `pam/laravel` substitui
código do framework.

## Event loops de terceiros

Amp 3 usa Revolt. Quando um Future de terceiro suspende a Fiber raiz sem um
envelope Pam, o runtime executa o loop Revolt até o Future concluir e depois
retoma o dispatch. Isso preserva o código e o autoload normais, inclusive dentro
de uma rota persistente. Não significa que Revolt virou um driver Tokio: enquanto
o loop de terceiro está rodando, ele ocupa aquele worker. Para hot paths, use
`Pam\Async`, `Pam\Http\Client`, streams e operações nativas; para
compatibilidade, use o pacote Composer sem fork.

O OpenTelemetry recebe um contexto raiz explícito ao entrar na Fiber, então não
depende de liberar FFI para instalar o observer de Fiber. Se o usuário habilitar o
observer oficial do pacote, a configuração dele continua sendo respeitada.
