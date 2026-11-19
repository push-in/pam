<?php

declare(strict_types=1);

namespace Nimbus\Admin;

use Arcano\Database\Injection\Parameter;
use Arcano\ORM\Select;
use Nimbus\Auth\Gate;
use Nimbus\Container\Container;
use Nimbus\Database\ArcanoManager;
use Nimbus\Http\FormData;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Pagination\Paginator;
use Nimbus\Session\Session;
use Nimbus\Validation\Input;
use Nimbus\Validation\ValidationException;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * @internal The admin routes. Field definitions and entity metadata are built once per
 * resource and worker.
 */
final class AdminController
{
    private const string FLASH = '_admin.status';

    private const int MAX_BULK = 1000;

    private const int MAX_OPTIONS = 500;

    /** @var array<string, string> */
    private static array $assets = [];

    /** @var array<class-string, EntityMetadata> */
    private array $metadata = [];

    /** @var array<string, array<string, AdminField>> */
    private array $fields = [];

    public function __construct(
        private readonly AdminPanel $panel,
        private readonly Container $container,
        private readonly ArcanoManager $arcano,
    ) {
    }

    public static function asset(string $file): string
    {
        return self::$assets[$file] ??= (string) file_get_contents(dirname(__DIR__, 2) . '/resources/admin/' . basename($file));
    }

    /** Cache-busting version of the assets. */
    public static function assetVersion(): string
    {
        return substr(hash('xxh3', self::asset('admin.css') . self::asset('admin.js')), 0, 10);
    }

    public function dashboard(Request $request, Response $response): Response
    {
        $cards = [];
        foreach ($this->panel->resources() as $slug => $resource) {
            if (!$this->allowed('viewAny', $resource->entity())) {
                continue;
            }
            $cards[] = [
                'label' => $resource->label(),
                'url' => $this->panel->url($slug),
                'count' => $resource->query($this->arcano->select($resource->entity()), $request)->count(),
            ];
        }
        return $this->page($response, 'dashboard', ['cards' => $cards, 'current' => null]);
    }

    public function index(Request $request, Response $response): Response
    {
        [$slug, $resource] = $this->resource($request);
        $this->authorize('viewAny', $resource->entity());
        $metadata = $this->metadata($resource);
        $fields = $this->fields($slug, $resource);
        $listed = array_filter($fields, static fn (AdminField $field): bool => $field->onList);
        $query = $request->query();

        $select = $resource->query($this->arcano->select($resource->entity()), $request);
        foreach ($listed as $field) {
            if ($field->type === 'belongsTo') {
                $select->load($field->name);
            }
        }
        $search = is_string($query['q'] ?? null) ? trim(mb_substr($query['q'], 0, 200)) : '';
        $searchable = array_keys(array_filter($fields, static fn (AdminField $field): bool => $field->searchable));
        if ($search !== '' && $searchable !== []) {
            $operator = $this->arcano->database()->getType() === 'Postgres' ? 'ILIKE' : 'LIKE';
            $pattern = '%' . addcslashes($search, '%_') . '%';
            $select->where(['@or' => array_map(static fn (string $name): array => [$name => [$operator => $pattern]], $searchable)]);
        }

        $filters = [];
        $active = is_array($query['f'] ?? null) ? $query['f'] : [];
        foreach ($fields as $name => $field) {
            if (!$field->filterable) {
                continue;
            }
            $options = match ($field->type) {
                'boolean' => ['1' => $this->panel->text('yes'), '0' => $this->panel->text('no')],
                'belongsTo' => $this->relationOptions($field),
                default => $field->options,
            };
            $value = $active[$name] ?? null;
            $value = is_string($value) && array_key_exists($value, $options) ? $value : '';
            if ($value !== '') {
                match ($field->type) {
                    'boolean' => $select->where($name, $value === '1'),
                    'belongsTo' => $select->where($metadata->innerKeys[$name] ?? $name . '_id', $value),
                    default => $select->where($name, $value),
                };
            }
            $filters[] = ['name' => $name, 'label' => $field->label, 'options' => $options, 'value' => $value];
        }

        [$sort, $direction] = $resource->defaultSort() ?? [$metadata->primaryKey, 'desc'];
        $requested = is_string($query['sort'] ?? null) ? $query['sort'] : null;
        if ($requested !== null && isset($listed[$requested]) && $listed[$requested]->sortable) {
            $sort = $requested;
            $direction = ($query['dir'] ?? 'asc') === 'desc' ? 'desc' : 'asc';
        }
        $select->orderBy($sort !== '' ? $sort : $metadata->primaryKey, strtoupper($direction));

        $page = Paginator::paginate($select, $request, max(1, min(Paginator::MAX_PER_PAGE, $resource->perPage())));
        $rows = [];
        $canDelete = false;
        foreach ($page->items as $entity) {
            $id = $this->identifier($entity, $metadata);
            $deletable = $this->allowed('delete', $entity);
            $canDelete = $canDelete || $deletable;
            $rows[] = [
                'id' => $id,
                'title' => $resource->title($entity),
                'cells' => array_map(fn (AdminField $field): string => $this->display($field, $entity), $listed),
                'edit' => $this->allowed('update', $entity) ? $this->panel->url("{$slug}/" . rawurlencode($id) . '/edit') : null,
                'delete' => $deletable ? $this->panel->url("{$slug}/" . rawurlencode($id)) : null,
            ];
        }
        $columns = [];
        foreach ($listed as $name => $field) {
            $sorted = $sort === $name ? $direction : null;
            $columns[] = [
                'label' => $field->label,
                'type' => $field->type,
                'sorted' => $sorted,
                'url' => $field->sortable ? $this->link($request, ['sort' => $name, 'dir' => $sorted === 'asc' ? 'desc' : 'asc', 'page' => null]) : null,
            ];
        }
        $actions = [];
        foreach ($resource->actions() as $name => $action) {
            $actions[$name] = $action->label;
        }
        if ($canDelete) {
            $actions['delete'] = $this->panel->text('delete');
        }

        return $this->page($response, 'index', [
            'current' => $slug,
            'resource' => $resource,
            'columns' => $columns,
            'rows' => $rows,
            'meta' => $page->meta,
            'links' => $page->links,
            'search' => $search,
            'back' => ($back = http_build_query($query)) !== '' ? '?' . $back : '',
            'searchable' => $searchable !== [],
            'filters' => $filters,
            'actions' => $actions,
            'indexUrl' => $this->panel->url($slug),
            'bulkUrl' => $this->panel->url("{$slug}/bulk"),
            'createUrl' => $this->allowed('create', $resource->entity()) ? $this->panel->url("{$slug}/create") : null,
        ]);
    }

    public function create(Request $request, Response $response): Response
    {
        [$slug, $resource] = $this->resource($request);
        $this->authorize('create', $resource->entity());
        return $this->form($response, $slug, $resource, null);
    }

    public function store(Request $request, Response $response): Response
    {
        [$slug, $resource] = $this->resource($request);
        $this->authorize('create', $resource->entity());
        $entity = $this->newEntity($resource->entity());
        $invalid = $this->save($request, $response, $slug, $resource, $entity, true);
        if ($invalid !== null) {
            return $invalid;
        }
        $this->flash($this->panel->text('created', ['title' => $resource->title($entity)]));
        return $this->redirect($response, $this->panel->url($slug));
    }

    public function edit(Request $request, Response $response): Response
    {
        [$slug, $resource] = $this->resource($request);
        $entity = $this->find($request, $resource);
        $this->authorize('update', $entity);
        return $this->form($response, $slug, $resource, $entity);
    }

    public function update(Request $request, Response $response): Response
    {
        [$slug, $resource] = $this->resource($request);
        $entity = $this->find($request, $resource);
        $this->authorize('update', $entity);
        $invalid = $this->save($request, $response, $slug, $resource, $entity, false);
        if ($invalid !== null) {
            return $invalid;
        }
        $this->flash($this->panel->text('updated', ['title' => $resource->title($entity)]));
        return $this->redirect($response, $this->panel->url($slug));
    }

    public function destroy(Request $request, Response $response): Response
    {
        [$slug, $resource] = $this->resource($request);
        $entity = $this->find($request, $resource);
        $this->authorize('delete', $entity);
        $title = $resource->title($entity);
        $this->arcano->entityManager()->delete($entity)->run();
        $this->flash($this->panel->text('deleted', ['title' => $title]));
        return $this->redirect($response, $this->panel->url($slug));
    }

    public function bulk(Request $request, Response $response): Response
    {
        [$slug, $resource] = $this->resource($request);
        $form = FormData::parse($request);
        $name = is_string($form['action'] ?? null) ? $form['action'] : '';
        $actions = $resource->actions();
        if ($name !== 'delete' && !isset($actions[$name])) {
            throw new HttpException(404, ProblemCode::NotFound, 'Unknown admin action.');
        }
        $ids = array_values(array_unique(array_filter(
            is_array($form['ids'] ?? null) ? $form['ids'] : [],
            static fn (mixed $id): bool => is_string($id) && $id !== '' && strlen($id) <= 64,
        )));
        $back = $this->panel->url($slug) . (is_string($form['_back'] ?? null) && str_starts_with($form['_back'], '?') ? $form['_back'] : '');
        if ($ids === []) {
            $this->flash($this->panel->text('nothing_selected'));
            return $this->redirect($response, $back);
        }
        if (count($ids) > self::MAX_BULK) {
            throw new ValidationException(['ids' => ['Too many records selected.']]);
        }
        $metadata = $this->metadata($resource);
        $entities = [...$resource->query($this->arcano->select($resource->entity()), $request)
            ->where([$metadata->primaryKey => ['in' => new Parameter($ids)]])
            ->fetchAll()];
        $ability = $name === 'delete' ? 'delete' : $actions[$name]->ability;
        foreach ($entities as $entity) {
            $this->authorize($ability, $entity);
        }
        if ($name === 'delete') {
            $manager = $this->arcano->entityManager();
            foreach ($entities as $entity) {
                $manager->delete($entity);
            }
            $manager->run();
            $message = $this->panel->text('bulk_deleted', ['count' => count($entities)]);
        } else {
            $message = ($actions[$name]->handler)($entities, $this->arcano)
                ?? $this->panel->text('bulk_done', ['count' => count($entities)]);
        }
        $this->flash($message);
        return $this->redirect($response, $back);
    }

    private function form(Response $response, string $slug, AdminResource $resource, ?object $entity): Response
    {
        $metadata = $this->metadata($resource);
        $session = $this->session();
        $errors = $session->errors();
        $old = $session->old();
        $old = is_array($old) && $old !== [] ? $old : null;
        $inputs = [];
        foreach ($this->fields($slug, $resource) as $name => $field) {
            if (!$field->onForm || ($entity === null && $field->readonly)) {
                continue;
            }
            $value = $old !== null && !$field->readonly
                ? self::scalar($old[$name] ?? null)
                : ($entity === null ? '' : $this->formValue($field, $entity));
            $inputs[] = [
                'field' => $field,
                'id' => 'field-' . $name,
                'value' => $value,
                'options' => $field->type === 'belongsTo' ? $this->relationOptions($field) : $field->options,
                'error' => $errors->first($name),
            ];
        }
        $id = $entity === null ? null : $this->identifier($entity, $metadata);
        return $this->page($response, 'form', [
            'current' => $slug,
            'resource' => $resource,
            'heading' => $entity === null
                ? $this->panel->text('new', ['resource' => $resource->singularLabel()])
                : $this->panel->text('edit', ['title' => $resource->title($entity)]),
            'inputs' => $inputs,
            'hasErrors' => $errors->any(),
            'action' => $id === null ? $this->panel->url($slug) : $this->panel->url("{$slug}/" . rawurlencode($id)),
            'method' => $id === null ? null : 'PUT',
            'indexUrl' => $this->panel->url($slug),
            'deleteUrl' => $entity !== null && $this->allowed('delete', $entity) ? $this->panel->url("{$slug}/" . rawurlencode((string) $id)) : null,
            'title' => $entity === null ? '' : $resource->title($entity),
        ]);
    }

    /** Writes and persists the form; on invalid input, returns the redirect back to the form. */
    private function save(Request $request, Response $response, string $slug, AdminResource $resource, object $entity, bool $creating): ?Response
    {
        $form = FormData::parse($request);
        $metadata = $this->metadata($resource);
        $fields = array_filter(
            $this->fields($slug, $resource),
            static fn (AdminField $field): bool => $field->onForm && !$field->readonly,
        );
        $errors = [];
        $values = [];
        $input = $resource->input();
        if ($input !== null) {
            try {
                $validated = $input::from(array_intersect_key($form, $fields));
            } catch (ValidationException $invalid) {
                return $this->invalid($response, $slug, $entity, $metadata, $invalid->errors, $form);
            }
            foreach ($fields as $name => $field) {
                if (property_exists($validated, $name)) {
                    $value = (new \ReflectionProperty($validated, $name))->getValue($validated);
                    $values[$name] = $field->type === 'belongsTo' ? $this->cast($field, $value, $errors) : $value;
                }
            }
        } else {
            foreach ($fields as $name => $field) {
                $values[$name] = $this->cast($field, $form[$name] ?? null, $errors);
            }
        }
        if ($errors !== []) {
            return $this->invalid($response, $slug, $entity, $metadata, $errors, $form);
        }
        foreach ($values as $name => $value) {
            self::write($entity, $name, $value);
        }
        $resource->saving($entity, $creating);
        $this->arcano->entityManager()->persist($entity)->run();
        return null;
    }

    /**
     * @param array<string, list<string>> $errors
     * @param array<array-key, mixed> $form
     */
    private function invalid(Response $response, string $slug, object $entity, EntityMetadata $metadata, array $errors, array $form): Response
    {
        $this->session()->flashErrors($errors)->flashInput($form);
        $id = $this->identifier($entity, $metadata);
        $url = $id === '' ? $this->panel->url("{$slug}/create") : $this->panel->url("{$slug}/" . rawurlencode($id) . '/edit');
        return $this->redirect($response, $url);
    }

    /** @param array<string, list<string>> $errors */
    private function cast(AdminField $field, mixed $raw, array &$errors): mixed
    {
        $name = $field->name;
        /** @param array<string, int|string> $replace */
        $fail = function (string $key, array $replace = []) use (&$errors, $name, $field): null {
            $errors[$name][] = $this->panel->text($key, ['field' => $field->label] + $replace);
            return null;
        };
        if ($field->type === 'boolean') {
            return in_array($raw, ['1', 'on', 'true', true, 1], true);
        }
        if (is_int($raw) || is_float($raw)) {
            $raw = (string) $raw;
        }
        if (!is_string($raw) || trim($raw) === '') {
            if ($field->required || (!$field->nullable && !in_array($field->type, ['text', 'textarea', 'email', 'url', 'password'], true))) {
                return $fail('required');
            }
            return $field->nullable ? null : '';
        }
        if (in_array($field->type, ['text', 'email', 'url', 'textarea', 'password'], true)) {
            $raw = str_replace("\r\n", "\n", $field->type === 'textarea' ? $raw : trim($raw));
        } else {
            $raw = trim($raw);
        }
        $value = match ($field->type) {
            'number' => preg_match('/^-?\d{1,18}$/', $raw) === 1 ? (int) $raw : $fail('invalid'),
            'decimal' => is_numeric($raw) ? (float) $raw : $fail('invalid'),
            'email' => filter_var($raw, FILTER_VALIDATE_EMAIL) !== false ? $raw : $fail('invalid'),
            'url' => filter_var($raw, FILTER_VALIDATE_URL) !== false && preg_match('#^https?://#i', $raw) === 1 ? $raw : $fail('invalid'),
            'date' => self::date('Y-m-d', $raw) ?? $fail('invalid'),
            'datetime' => self::date('Y-m-d\TH:i', $raw) ?? self::date('Y-m-d\TH:i:s', $raw) ?? $fail('invalid'),
            'time' => preg_match('/^\d{2}:\d{2}(:\d{2})?$/', $raw) === 1 ? $raw : $fail('invalid'),
            'json' => json_validate($raw) ? json_decode($raw, true) : $fail('invalid'),
            'select' => array_key_exists($raw, $field->options)
                ? ($field->enum !== null ? $field->enum::tryFrom(is_numeric($raw) && is_int(array_key_first($field->options)) ? (int) $raw : $raw) : $raw)
                : $fail('invalid'),
            'belongsTo' => $this->related($field, $raw) ?? $fail('invalid'),
            default => $raw,
        };
        if ($value === null) {
            return null;
        }
        if (is_string($value) && $field->maxLength !== null && mb_strlen($value) > $field->maxLength) {
            return $fail('too_long', ['max' => $field->maxLength]);
        }
        foreach ($field->rules as $rule) {
            $message = $rule->validate($field->label, $value instanceof \BackedEnum ? $value->value : $value);
            if ($message !== null) {
                $errors[$name][] = $message;
            }
        }
        return $value;
    }

    private function related(AdminField $field, string $id): ?object
    {
        if ($field->target === null) {
            return null;
        }
        $metadata = $this->metadataOf($field->target);
        return $this->arcano->select($field->target)->where($metadata->primaryKey, $id)->fetchOne();
    }

    /** @return array<string, string> */
    private function relationOptions(AdminField $field): array
    {
        if ($field->target === null) {
            return [];
        }
        $target = $this->panel->resourceFor($field->target);
        $metadata = $this->metadataOf($field->target);
        $options = [];
        foreach ($this->arcano->select($field->target)->orderBy($metadata->primaryKey)->limit(self::MAX_OPTIONS)->fetchAll() as $entity) {
            $options[$this->identifier($entity, $metadata)] = $target->title($entity);
        }
        return $options;
    }

    private function display(AdminField $field, object $entity): string
    {
        $value = self::read($entity, $field->name);
        if ($field->format !== null) {
            return ($field->format)($value, $entity);
        }
        if ($value === null) {
            return '';
        }
        return match (true) {
            $field->type === 'belongsTo' && is_object($value) => $this->panel->resourceFor($value::class)->title($value),
            is_bool($value) => $this->panel->text($value ? 'yes' : 'no'),
            $value instanceof \DateTimeInterface => $value->format(match ($field->type) {
                'date' => 'Y-m-d',
                'time' => 'H:i',
                default => 'Y-m-d H:i',
            }),
            $value instanceof \BackedEnum => $field->options[$value->value] ?? (string) $value->value,
            $field->type === 'select' && (is_string($value) || is_int($value)) => $field->options[$value] ?? (string) $value,
            is_scalar($value) => mb_strimwidth((string) $value, 0, 80, '…'),
            $value instanceof \Stringable => mb_strimwidth((string) $value, 0, 80, '…'),
            default => mb_strimwidth((string) json_encode($value, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES), 0, 80, '…'),
        };
    }

    private function formValue(AdminField $field, object $entity): string
    {
        $value = self::read($entity, $field->name);
        return match (true) {
            $value === null => '',
            $field->type === 'belongsTo' && is_object($value) => $this->identifier($value, $this->metadataOf($value::class)),
            is_bool($value) => $value ? '1' : '0',
            $value instanceof \DateTimeInterface => $value->format(match ($field->type) {
                'date' => 'Y-m-d',
                'time' => 'H:i',
                default => 'Y-m-d\TH:i',
            }),
            $value instanceof \BackedEnum => (string) $value->value,
            is_scalar($value) => (string) $value,
            $value instanceof \Stringable => (string) $value,
            default => (string) json_encode($value, JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES),
        };
    }

    /** @return array{string, AdminResource} */
    private function resource(Request $request): array
    {
        $slug = (string) $request->route('resource');
        $resource = $this->panel->resourceBySlug($slug)
            ?? throw new HttpException(404, ProblemCode::NotFound, 'Unknown admin resource.');
        return [$slug, $resource];
    }

    private function find(Request $request, AdminResource $resource): object
    {
        $id = (string) $request->route('id');
        $metadata = $this->metadata($resource);
        $entity = $id === '' || strlen($id) > 64 ? null : $resource
            ->query($this->arcano->select($resource->entity()), $request)
            ->where($metadata->primaryKey, $id)
            ->fetchOne();
        return $entity ?? throw new HttpException(404, ProblemCode::NotFound, 'Record not found.');
    }

    /** @return array<string, AdminField> */
    private function fields(string $slug, AdminResource $resource): array
    {
        if (!isset($this->fields[$slug])) {
            $fields = [];
            foreach ($resource->fields($this->metadata($resource)->fields) as $name => $field) {
                $fields[$field->name] = $field;
            }
            $this->fields[$slug] = $fields;
        }
        return $this->fields[$slug];
    }

    private function metadata(AdminResource $resource): EntityMetadata
    {
        return $this->metadataOf($resource->entity());
    }

    private function metadataOf(string $class): EntityMetadata
    {
        $class = $this->panel->resourceFor($class)->entity();
        return $this->metadata[$class] ??= EntityMetadata::of($this->arcano->orm()->getSchema(), $class);
    }

    private function identifier(object $entity, EntityMetadata $metadata): string
    {
        return self::scalar(self::read($entity, $metadata->primaryKey));
    }

    /**
     * Whether the ability is granted; resources without a policy or Gate rule only need `viewAdmin`.
     *
     * @param object|class-string $subject
     */
    private function allowed(string $ability, object|string $subject): bool
    {
        $gate = $this->container->get(Gate::class);
        if (!$gate instanceof Gate) {
            return false;
        }
        if ($gate->policyFor($subject) === null && !$gate->has($ability)) {
            return true;
        }
        return $gate->check($gate->principal(), $ability, [$subject]);
    }

    /** @param object|class-string $subject */
    private function authorize(string $ability, object|string $subject): void
    {
        if (!$this->allowed($ability, $subject)) {
            throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
        }
    }

    /** @param array<string, mixed> $data */
    private function page(Response $response, string $view, array $data): Response
    {
        $navigation = [];
        foreach ($this->panel->resources() as $slug => $resource) {
            if ($this->allowed('viewAny', $resource->entity())) {
                $navigation[] = ['slug' => $slug, 'label' => $resource->label(), 'url' => $this->panel->url($slug)];
            }
        }
        $flash = $this->session()->get(self::FLASH);
        $html = $this->panel->views()->render($view, $data + [
            'admin' => $this->panel,
            't' => $this->panel->text(...),
            'navigation' => $navigation,
            'flash' => is_string($flash) ? $flash : null,
        ]);
        return $response->header('content-type', 'text/html; charset=utf-8')->send($html);
    }

    private function redirect(Response $response, string $location): Response
    {
        return $response->status(303)->header('location', $location)->send(null);
    }

    private function flash(string $message): void
    {
        $this->session()->flash(self::FLASH, $message);
    }

    private function session(): Session
    {
        $session = $this->container->scopedValue(Session::class);
        return $session instanceof Session ? $session : throw new \LogicException('The admin panel requires a session.');
    }

    /** @param array<string, string|null> $replace */
    private function link(Request $request, array $replace): string
    {
        $query = array_filter(array_replace($request->query(), $replace), static fn (mixed $value): bool => $value !== null && $value !== '');
        return $request->path . ($query === [] ? '' : '?' . http_build_query($query));
    }

    /** @param class-string $class */
    private function newEntity(string $class): object
    {
        $reflection = new \ReflectionClass($class);
        $constructor = $reflection->getConstructor();
        return $constructor === null || $constructor->getNumberOfRequiredParameters() === 0
            ? $reflection->newInstance()
            : $reflection->newInstanceWithoutConstructor();
    }

    private static function read(object $entity, string $property): mixed
    {
        if (!property_exists($entity, $property)) {
            return null;
        }
        $reflection = new \ReflectionProperty($entity, $property);
        return $reflection->isInitialized($entity) ? $reflection->getValue($entity) : null;
    }

    private static function write(object $entity, string $property, mixed $value): void
    {
        if (property_exists($entity, $property)) {
            (new \ReflectionProperty($entity, $property))->setValue($entity, $value);
        }
    }

    private static function date(string $format, string $value): ?\DateTimeImmutable
    {
        $date = \DateTimeImmutable::createFromFormat('!' . $format, $value);
        return $date !== false && $date->format($format) === $value ? $date : null;
    }

    private static function scalar(mixed $value): string
    {
        return is_scalar($value) || $value instanceof \Stringable ? (string) $value : '';
    }
}
