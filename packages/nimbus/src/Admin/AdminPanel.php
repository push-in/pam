<?php

declare(strict_types=1);

namespace Nimbus\Admin;

use Nimbus\App;
use Nimbus\Database\ArcanoManager;
use Nimbus\View\ViewFactory;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * The admin panel of an application: its resources, texts and templates. Create it with
 * `Admin::panel()`; resources can be added until the application starts serving.
 *
 * Routes (under the prefix): `GET /` dashboard, `GET /{resource}` list (`q`, `sort`, `dir`,
 * `f[field]`, `page`), `GET /{resource}/create`, `POST /{resource}`, `GET /{resource}/{id}/edit`,
 * `PUT /{resource}/{id}`, `DELETE /{resource}/{id}` and `POST /{resource}/bulk`.
 */
final class AdminPanel
{
    private const array MESSAGES = [
        'en' => [
            'lang' => 'en',
            'dashboard' => 'Dashboard',
            'records' => ':count records',
            'search' => 'Search',
            'filter' => 'Filter',
            'clear' => 'Clear',
            'all' => 'All',
            'yes' => 'Yes',
            'no' => 'No',
            'new' => 'New :resource',
            'edit' => 'Edit :title',
            'edit_link' => 'Edit',
            'save' => 'Save',
            'cancel' => 'Cancel',
            'delete' => 'Delete',
            'delete_confirm' => 'Delete :title? This cannot be undone.',
            'bulk_delete_confirm' => 'Delete the selected records? This cannot be undone.',
            'actions' => 'Actions',
            'apply' => 'Apply',
            'with_selected' => 'With selected',
            'select_all' => 'Select all rows',
            'select_row' => 'Select :title',
            'empty' => 'No records.',
            'created' => ':title created.',
            'updated' => ':title saved.',
            'deleted' => ':title deleted.',
            'bulk_deleted' => ':count records deleted.',
            'bulk_done' => 'Action applied to :count records.',
            'nothing_selected' => 'Select at least one record.',
            'required' => 'The :field field is required.',
            'invalid' => 'The :field field is invalid.',
            'too_long' => 'The :field field must have at most :max characters.',
            'page' => 'Page :current of :last',
            'previous' => 'Previous',
            'next' => 'Next',
            'results' => ':total results',
            'theme' => 'Dark mode',
            'skip' => 'Skip to content',
            'forbidden' => 'You are not allowed to do this.',
            'not_found' => 'This page does not exist.',
            'back' => 'Back to the dashboard',
            'fix_errors' => 'Some fields need attention.',
            'navigation' => 'Resources',
            'none' => 'None',
            'choose' => 'Choose...',
        ],
        'pt_BR' => [
            'lang' => 'pt-BR',
            'dashboard' => 'Painel',
            'records' => ':count registros',
            'search' => 'Buscar',
            'filter' => 'Filtrar',
            'clear' => 'Limpar',
            'all' => 'Todos',
            'yes' => 'Sim',
            'no' => 'Não',
            'new' => 'Criar :resource',
            'edit' => 'Editar :title',
            'edit_link' => 'Editar',
            'save' => 'Salvar',
            'cancel' => 'Cancelar',
            'delete' => 'Excluir',
            'delete_confirm' => 'Excluir :title? Não dá para desfazer.',
            'bulk_delete_confirm' => 'Excluir os registros selecionados? Não dá para desfazer.',
            'actions' => 'Ações',
            'apply' => 'Aplicar',
            'with_selected' => 'Com os selecionados',
            'select_all' => 'Selecionar todas as linhas',
            'select_row' => 'Selecionar :title',
            'empty' => 'Nenhum registro.',
            'created' => ':title criado.',
            'updated' => ':title salvo.',
            'deleted' => ':title excluído.',
            'bulk_deleted' => ':count registros excluídos.',
            'bulk_done' => 'Ação aplicada a :count registros.',
            'nothing_selected' => 'Selecione ao menos um registro.',
            'required' => 'O campo :field é obrigatório.',
            'invalid' => 'O campo :field é inválido.',
            'too_long' => 'O campo :field aceita no máximo :max caracteres.',
            'page' => 'Página :current de :last',
            'previous' => 'Anterior',
            'next' => 'Próxima',
            'results' => ':total resultados',
            'theme' => 'Modo escuro',
            'skip' => 'Pular para o conteúdo',
            'forbidden' => 'Você não tem permissão para isso.',
            'not_found' => 'Esta página não existe.',
            'back' => 'Voltar ao painel',
            'fix_errors' => 'Alguns campos precisam de correção.',
            'navigation' => 'Recursos',
            'none' => 'Nenhum',
            'choose' => 'Selecione...',
        ],
    ];

    public readonly string $prefix;

    /** @var array<string, AdminResource> By slug. */
    private array $resources = [];

    /** @var array<string, string> */
    private array $messages;

    private ?ViewFactory $views = null;

    private ?AdminController $controller = null;

    public function __construct(
        private readonly App $app,
        string $prefix = '/admin',
        private ?string $loginUrl = null,
        private string $title = 'Admin',
        private readonly ?string $viewPath = null,
    ) {
        $this->prefix = '/' . trim($prefix, '/');
        if ($this->prefix === '/') {
            throw new \InvalidArgumentException('The admin panel needs a path prefix such as /admin.');
        }
        $this->messages = self::MESSAGES['en'];
        $this->registerRoutes();
    }

    /**
     * Adds a resource: an AdminResource subclass, an instance, or an entity class for the defaults.
     *
     * @param AdminResource|class-string $resource
     */
    public function resource(AdminResource|string $resource): self
    {
        if (is_string($resource)) {
            $resource = is_a($resource, AdminResource::class, true)
                ? $this->instantiate($resource)
                : new AdminResource($resource);
        }
        $slug = $resource->slug();
        if (preg_match('/^[a-z0-9][a-z0-9_-]*$/', $slug) !== 1 || $slug === '_assets') {
            throw new \InvalidArgumentException("Invalid admin resource slug {$slug}.");
        }
        if (isset($this->resources[$slug])) {
            throw new \LogicException("The admin resource {$slug} is already registered.");
        }
        $this->resources[$slug] = $resource;
        return $this;
    }

    /** Built-in texts: `en` (default) or `pt_BR`. */
    public function locale(string $locale): self
    {
        $this->messages = self::MESSAGES[$locale] ?? throw new \InvalidArgumentException("No admin texts for {$locale}; use messages().");
        return $this;
    }

    /** @param array<string, string> $messages Replaces built-in texts by key (see the `en` set). */
    public function messages(array $messages): self
    {
        $this->messages = $messages + $this->messages;
        return $this;
    }

    public function title(string $title): self
    {
        $this->title = $title;
        return $this;
    }

    public function loginUrl(?string $url): self
    {
        $this->loginUrl = $url;
        return $this;
    }

    /** @return array<string, AdminResource> */
    public function resources(): array
    {
        return $this->resources;
    }

    /** @internal */
    public function resourceBySlug(string $slug): ?AdminResource
    {
        return $this->resources[$slug] ?? null;
    }

    /** @internal The resource registered for an entity class, or a default one. */
    public function resourceFor(string $class): AdminResource
    {
        foreach ($this->resources as $resource) {
            if ($resource->entity() === $class || is_a($class, $resource->entity(), true)) {
                return $resource;
            }
        }
        if (!class_exists($class)) {
            throw new \InvalidArgumentException("{$class} is not a class.");
        }
        return new AdminResource($class);
    }

    /**
     * @internal
     * @param array<string, int|string|float> $replace
     */
    public function text(string $key, array $replace = []): string
    {
        $text = $this->messages[$key] ?? $key;
        foreach ($replace as $name => $value) {
            $text = str_replace(':' . $name, (string) $value, $text);
        }
        return $text;
    }

    /** @internal */
    public function displayTitle(): string
    {
        return $this->title;
    }

    /** @internal */
    public function login(): ?string
    {
        return $this->loginUrl;
    }

    /** @internal */
    public function url(string $path = ''): string
    {
        return $this->prefix . ($path === '' ? '' : '/' . ltrim($path, '/'));
    }

    /** @internal */
    public function views(): ViewFactory
    {
        if ($this->views === null) {
            $paths = [dirname(__DIR__, 2) . '/resources/admin/views'];
            if ($this->viewPath !== null) {
                array_unshift($paths, $this->viewPath);
            }
            $this->views = new ViewFactory($paths, $this->app->container(), '');
        }
        return $this->views;
    }

    private function registerRoutes(): void
    {
        $assets = $this->prefix . '/_assets/';
        foreach (['admin.css' => 'text/css; charset=utf-8', 'admin.js' => 'text/javascript; charset=utf-8'] as $file => $type) {
            $this->app->get($assets . $file, static function (Response $response) use ($file, $type): Response {
                return $response
                    ->header('content-type', $type)
                    ->header('cache-control', 'public, max-age=31536000, immutable')
                    ->header('x-content-type-options', 'nosniff')
                    ->send(AdminController::asset($file));
            });
        }
        $routes = $this->app->prefix($this->prefix)->middleware(new AdminMiddleware($this, $this->app->container()));
        $controller = function (): AdminController {
            $container = $this->app->container();
            $arcano = $container->get(ArcanoManager::class);
            if (!$arcano instanceof ArcanoManager) {
                throw new \LogicException('The admin panel requires Nimbus\\Database\\ArcanoServiceProvider.');
            }
            return $this->controller ??= new AdminController($this, $container, $arcano);
        };
        $routes->get('', static fn (Request $request, Response $response): Response => $controller()->dashboard($request, $response));
        $routes->get('{resource}', static fn (Request $request, Response $response): Response => $controller()->index($request, $response));
        $routes->get('{resource}/create', static fn (Request $request, Response $response): Response => $controller()->create($request, $response));
        $routes->post('{resource}', static fn (Request $request, Response $response): Response => $controller()->store($request, $response));
        $routes->post('{resource}/bulk', static fn (Request $request, Response $response): Response => $controller()->bulk($request, $response));
        $routes->get('{resource}/{id}/edit', static fn (Request $request, Response $response): Response => $controller()->edit($request, $response));
        $routes->put('{resource}/{id}', static fn (Request $request, Response $response): Response => $controller()->update($request, $response));
        $routes->delete('{resource}/{id}', static fn (Request $request, Response $response): Response => $controller()->destroy($request, $response));
    }

    /** @param class-string<AdminResource> $class */
    private function instantiate(string $class): AdminResource
    {
        $container = $this->app->container();
        $resource = $container->has($class) ? $container->get($class) : new $class();
        if (!$resource instanceof AdminResource) {
            throw new \LogicException("{$class} must extend " . AdminResource::class . '.');
        }
        return $resource;
    }
}
