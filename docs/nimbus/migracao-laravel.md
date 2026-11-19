# Migração do Laravel

O Nimbus segue o mapa de pastas do Laravel e muitos nomes de método, mas não é compatível no código: não há fachadas, o ORM é outro (data mapper, não Active Record) e as rotas ficam num arquivo só. Para rodar um app Laravel sem reescrever, use o PAM como servidor do próprio Laravel ([PAM com Laravel](https://github.com/push-in/pam/blob/main/docs/laravel.md)); esta página é para quem vai portar o código para o Nimbus.

## Diferenças de modelo

- **Workers persistentes**: o app fica carregado entre requisições. Nada de estado do usuário em `singleton` ou propriedade estática; use `scoped` ou passe pelo `Request`.
- **Sem fachadas**: dependências chegam pelo construtor ou pelos parâmetros da action.
- **Data mapper**: entidades são classes simples; quem lê e grava é o `ArcanoManager`, não a própria entidade.
- **Rotas num arquivo**: `routes.php` com `$router`, sem `Route::`. Grupos são closures.
- **Erros em problem+json** com `code` inteiro estável.

## Equivalências

### Estrutura e CLI

- `php artisan serve` → `pam dev index.php`
- `php artisan` (comandos) → `pam` (`pam make:*`, `pam routes`, `pam test`)
- `composer require` → `pam composer require`
- `php artisan route:list` → `pam routes`
- `php artisan test` → `pam test`
- `php artisan migrate` → `pam exec bin/migrate` (template web) ou `ArcanoMigrationManager::migrate()`
- `php artisan db:seed` → `pam exec bin/migrate seed` ou `SeederRunner`
- `routes/api.php` e `routes/web.php` → `routes.php`
- `app/Http/Controllers` → `app/Controllers`
- `app/Models` → `app/Models` (entidades Arcano)
- `bootstrap/app.php` → `bootstrap/app.php` (monta o `Nimbus\App`) ou `index.php`
- `config/*.php` + `env()` → `DatabaseConfig::fromEnvironment()`, `Configuration::fromEnvironment()` com tipos
- Laravel Forge / Envoyer → `pam deploy:remote`

### Rotas e HTTP

- `Route::get('/x', [C::class, 'm'])` → `$router->get('x', [C::class, 'm'])`
- `Route::controller(C::class)->group(fn () => ...)` → `$router->controller(C::class, function ($router) { $router->get('x', 'm'); })`
- `Route::apiResource('users', C::class)` → `$router->resource('users', C::class)` ou `apiResource()`
- `Route::prefix('admin')->middleware('auth')->group(...)` → `$router->prefix('admin')->auth()->group(...)`
- `->middleware('can:update,product')` → `->can('update', 'product')`
- `->middleware('signed')` → `->signed()`
- `->whereNumber('id')` → `->where('id', RouteConstraint::Integer)`
- `Route::model()` / route model binding → `$container->bindRoute()` ou `RouteBindable`
- `$request->route('id')` → `$request->route('id')`
- `response()->json($d, 201)` → `$response->json($d, 201)`
- `redirect('/x')` / `back()` → `Redirect::to('/x')` / `Redirect::back($request)`
- `abort(404)` → `throw new HttpException(404, ProblemCode::NotFound, '...')`
- `ThrottleRequests` → `RateLimitMiddleware`
- `HandleCors` → `CorsMiddleware`

### Validação e respostas

- `FormRequest` com `rules()` → `Nimbus\Validation\FormRequest` (mesmo formato, regras básicas) ou `Input` com propriedades tipadas e atributos
- `'max:120'`, `'min:1'`, `'email'`, `'in:a,b'` → `#[Max(120)]`, `#[Min(1)]`, `#[Email]`, `#[In('a', 'b')]`
- `Rule::enum(X::class)` → `Rule::enum(X::class)` ou propriedade tipada com o enum
- `$request->validated()` → `$request->validated()` (FormRequest) ou as propriedades do `Input`
- `JsonResource` / `ResourceCollection` → `Nimbus\Http\JsonResource` / `ResourceCollection` (`toArray(Request $request)`)
- `->paginate(20)` → `Paginator::paginate($select, $request, 20)`
- `->cursorPaginate(20)` → `Paginator::cursor($select, $request, 20)`
- Scribe / L5-Swagger → `$app->docs()` e `$app->openApi()`

### Banco

- Model do Laravel (Active Record) → entidade Arcano com `#[Entity]` e `#[Column]`
- `Product::find($id)` → `$arcano->select(Product::class)->wherePK($id)->fetchOne()`
- `Product::where(...)->get()` → `$arcano->select(Product::class)->where(...)->fetchAll()`
- `->with('posts')` → `->load('posts')`
- `$model->save()` → `$arcano->entityManager()->persist($entity)->run()`
- `$model->delete()` → `$arcano->entityManager()->delete($entity)->run()`
- `hasMany` / `belongsTo` → `#[HasMany]` / `#[BelongsTo]`
- `DB::transaction(fn () => ...)` → `$arcano->transaction(fn () => ...)`
- `DB::table('x')` → `$arcano->database()->table('x')`
- `Schema::create()` em migration → `$this->table('x')->addColumn(...)->create()` em `Arcano\Migrations\Migration`
- `php artisan make:migration` → `pam make:migration` ou `ArcanoMigrationManager::generate()` pela diferença das entidades
- Factories (`Product::factory()->count(3)->create()`) → `ProductFactory::new()->count(3)->create()`
- Seeders → `Nimbus\Database\Seeder` com `run()` e `call()`

### Autenticação e autorização

- Sanctum, tokens pessoais → `PersonalAccessTokens` + `PersonalAccessTokenAuthenticator`
- `$user->createToken('cli', ['x:read'])` → `$tokens->create('user:42', 'cli', ['x:read'])`
- `tokenCan('x')` → `$principal->can('x')`
- Passport / JWT → `JwtCodec` + `JwtAuthenticator`, `TokenPairIssuer` para access + refresh
- Socialite → `SocialLogin` (sobre `league/oauth2-client`)
- `Password::sendResetLink` / verificação de e-mail → `PasswordResets`, `EmailVerification` e `AuthRoutes`
- `Auth::login($user)` (web) → `SessionAuthenticator::login($user)`
- `auth()->user()` → parâmetro `Principal $user` na action
- `Gate::define()` / `Gate::policy()` → `$app->gate()->define()` / `->policy()`
- `$this->authorize('update', $p)` → `$gate->authorize('update', $p)`
- `Hash::make()` / `Hash::check()` → `Nimbus\Security\Hash::make()` / `::check()`
- `Crypt::encryptString()` → `Crypt::fromEnvironment()->encrypt()`
- `URL::temporarySignedRoute()` → `UrlSigner::temporary()`

### Filas, agendador e notificações

- Job com `ShouldQueue` → classe `SerializableJob` com `handle()`
- `dispatch(new X)` → `$queue->dispatch(new X, maximumAttempts: 3, delaySeconds: 0)`
- Driver `redis` / `database` → `RedisJobQueue` / `DatabaseJobQueue`
- `php artisan queue:work` + Supervisor → `pam start --task-workers 1` (ou `Background::run()`)
- `failed_jobs`, `queue:retry` → `FailedJobRepository::failed()`, `retryFailed()`
- `Schedule` em `routes/console.php` + cron do sistema → `Schedule` + `$app->background()->scheduler(...)`, sem cron
- `->withoutOverlapping()`, `->onOneServer()` → `withoutOverlapping()`, `SchedulerLock` compartilhado
- `Notification` com `via()`, `toMail()` → `Nimbus\Notifications\Notification` (mail, push, database, webhook)
- `$user->notify()` / `Notification::send()` → `$notifier->send()` / `sendLater()`
- `Notification::fake()` → `Notifier::fake()`

### Storage, cache e Redis

- `Storage::disk('s3')->put()` → `$app->storage()->disk('s3')->put()` ou `Disk` injetado
- `Storage::temporaryUrl()` → `$disk->temporaryUrl()`; upload direto com `temporaryUploadUrl()`
- `Redis::command()` → `RedisPool::command([...])`
- `Cache::remember()` de resposta HTTP → `ResponseCacheMiddleware`

### Web

- Blade (`.blade.php`) → views `.nim.php` com as mesmas diretivas principais e componentes `<x-...>`
- `view('x', $d)` → `view('x', $d)`
- `session()->flash()` → `Session::flash()` (injetada)
- `@csrf`, `VerifyCsrfToken` → `@csrf`, `VerifyCsrfTokenMiddleware`
- `__('x')`, `trans_choice()` → `__('x')`, `trans_choice()` com `lang/{locale}`

### Testes

- `Tests\TestCase` → `Nimbus\Testing\TestCase` com `createApplication()`
- `RefreshDatabase` → `Nimbus\Testing\RefreshDatabase`
- `$this->getJson()->assertOk()->assertJsonPath()` → mesmos nomes
- `$this->actingAs($user)` → `$this->actingAs($principal)`
- `assertDatabaseHas` → `assertDatabaseHas`

## Ordem sugerida

1. `pam init app --template api` ao lado do app atual.
2. Portar as entidades para Arcano e gerar as migrations pela diferença; conferir com `status()` contra uma cópia do banco.
3. Portar rotas e controllers por recurso, com os testes de feature junto.
4. Trocar autenticação e filas.
5. Medir os endpoints mais usados antes de trocar o tráfego.
