<?php

declare(strict_types=1);

namespace Nimbus\View;

use Nimbus\Container\Container;
use Nimbus\Translation\Translator;

/**
 * Finds, compiles and renders `.nim.php` templates. Each template is compiled once per
 * worker and kept in memory as a closure; renders share nothing else, so concurrent
 * requests (fibers) are isolated. `products.index` is `{path}/products/index.nim.php`,
 * `mail::welcome` looks in the `mail` namespace, and `<x-alert>` is the class
 * `{componentNamespace}\Alert` or the template `components/alert.nim.php`.
 */
final class ViewFactory
{
    public const string EXTENSION = '.nim.php';

    private static ?self $default = null;

    /** @var array<string, list<string>> */
    private array $paths = [];

    /** @var array<string, mixed> */
    private array $shared = [];

    /** @var array<string, array{string, \Closure(Renderer, array<string, mixed>): string, int}> */
    private array $compiled = [];

    /** @var array<string, array{'class'|'view', string}> */
    private array $components = [];

    /** @var array<class-string, array{params: list<array{name: string, kebab: string, class: ?string, optional: bool, default: mixed, nullable: bool}>, properties: list<string>, methods: list<string>}> */
    private array $componentPlans = [];

    /** @param string|list<string> $paths */
    public function __construct(
        string|array $paths,
        private readonly ?Container $container = null,
        private readonly string $componentNamespace = 'App\\Views',
        private readonly bool $reload = false,
        private ?Translator $translator = null,
    ) {
        $this->paths[''] = self::normalizePaths($paths);
    }

    /** The factory used by the `view()` helper (set by `useAsDefault()`). */
    public static function default(): self
    {
        return self::$default ?? throw new \LogicException('No view factory: register Nimbus\\View\\ViewServiceProvider.');
    }

    public function useAsDefault(): self
    {
        self::$default = $this;
        return $this;
    }

    /** @param string|list<string> $paths */
    public function addNamespace(string $namespace, string|array $paths): self
    {
        if ($namespace === '' || str_contains($namespace, ':')) {
            throw new \InvalidArgumentException('A view namespace must be a non-empty name without ":".');
        }
        $this->paths[$namespace] = self::normalizePaths($paths);
        return $this;
    }

    /** Data available in every template of the worker (not per request). */
    public function share(string $key, mixed $value): self
    {
        $this->shared[$key] = $value;
        return $this;
    }

    /** @return array<string, mixed> */
    public function shared(): array
    {
        return $this->shared;
    }

    public function setTranslator(?Translator $translator): self
    {
        $this->translator = $translator;
        return $this;
    }

    public function translator(): ?Translator
    {
        return $this->translator;
    }

    public function container(): ?Container
    {
        return $this->container;
    }

    /** @param array<string, mixed> $data */
    public function make(string $view, array $data = []): View
    {
        return new View($this, $view, $data);
    }

    /** @param array<string, mixed> $data */
    public function render(string $view, array $data = []): string
    {
        return (new Renderer($this, $this->container))->render($view, $data);
    }

    /**
     * Renders template source that is not in a file (tests, e-mails built at runtime).
     *
     * @param array<string, mixed> $data
     */
    public function renderString(string $source, array $data = []): string
    {
        $key = 'string:' . hash('xxh128', $source);
        if (!isset($this->compiled[$key])) {
            $this->compiled[$key] = ['inline', self::evaluate(Compiler::compile($source, 'inline')->code, 'inline'), 0];
        }
        return (new Renderer($this, $this->container))->render($key, $data);
    }

    public function exists(string $view): bool
    {
        return isset($this->compiled[$view]) || $this->find($view) !== null;
    }

    /** The file of a view. */
    public function path(string $view): string
    {
        return $this->find($view) ?? throw new \InvalidArgumentException("View [{$view}] not found.");
    }

    /**
     * The compiled closure of a view, compiled on first use.
     *
     * @internal
     * @return array{string, \Closure(Renderer, array<string, mixed>): string}
     */
    public function compiled(string $view): array
    {
        $entry = $this->compiled[$view] ?? null;
        if ($entry !== null && (!$this->reload || $entry[2] === 0 || @filemtime($entry[0]) === $entry[2])) {
            return [$entry[0], $entry[1]];
        }
        $path = $this->path($view);
        $source = @file_get_contents($path);
        if (!is_string($source)) {
            throw new \RuntimeException("Could not read the view {$path}.");
        }
        $closure = self::evaluate(Compiler::compile($source, $path)->code, $path);
        $this->compiled[$view] = [$path, $closure, (int) @filemtime($path)];
        return [$path, $closure];
    }

    /**
     * Compiles PHP code produced by the Compiler into its closure.
     *
     * @return \Closure(Renderer, array<string, mixed>): string
     */
    public static function evaluate(string $code, string $template): \Closure
    {
        try {
            $closure = eval($code);
        } catch (\ParseError $error) {
            throw new ViewException('Syntax error: ' . $error->getMessage(), $template, $error->getLine(), $error);
        }
        if (!$closure instanceof \Closure) {
            throw new ViewException('The template did not compile to a closure.', $template, 1);
        }
        /** @var \Closure(Renderer, array<string, mixed>): string $closure */
        return $closure;
    }

    /**
     * Every template file of the factory's paths, by view name.
     *
     * @return array<string, string>
     */
    public function templates(): array
    {
        $templates = [];
        foreach ($this->paths as $namespace => $paths) {
            foreach (array_reverse($paths) as $root) {
                if (!is_dir($root)) {
                    continue;
                }
                $iterator = new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($root, \FilesystemIterator::SKIP_DOTS));
                foreach ($iterator as $file) {
                    if (!$file instanceof \SplFileInfo || !str_ends_with($file->getFilename(), self::EXTENSION)) {
                        continue;
                    }
                    $relative = substr($file->getPathname(), strlen($root) + 1, -strlen(self::EXTENSION));
                    $name = str_replace(DIRECTORY_SEPARATOR, '.', $relative);
                    $templates[($namespace === '' ? '' : $namespace . '::') . $name] = $file->getPathname();
                }
            }
        }
        ksort($templates);
        return $templates;
    }

    /**
     * How `<x-name>` resolves: a Component class or an anonymous template.
     *
     * @return array{'class'|'view', string}
     */
    public function component(string $name): array
    {
        if (isset($this->components[$name])) {
            return $this->components[$name];
        }
        $segments = explode('.', $name);
        $class = rtrim($this->componentNamespace, '\\') . '\\' . implode('\\', array_map(
            static fn (string $segment): string => str_replace(' ', '', ucwords(str_replace(['-', '_'], ' ', $segment))),
            $segments,
        ));
        if ($this->componentNamespace !== '' && class_exists($class) && is_subclass_of($class, Component::class)) {
            return $this->components[$name] = ['class', $class];
        }
        foreach (["components.{$name}", "components.{$name}.index"] as $view) {
            if ($this->find($view) !== null) {
                return $this->components[$name] = ['view', $view];
            }
        }
        throw new \InvalidArgumentException("Component <x-{$name}> not found: no class {$class} and no view components.{$name}.");
    }

    /**
     * @internal
     * @param array<string, mixed> $attributes
     * @param array<string, Slot> $slots
     */
    public function renderComponent(Renderer $renderer, string $name, array $attributes, array $slots): string
    {
        [$kind, $target] = $this->component($name);
        if ($kind === 'view') {
            return $renderer->render($target, ['attributes' => new AttributeBag($attributes)] + $slots + self::variables($attributes));
        }
        /** @var class-string<Component> $target */
        [$component, $remaining] = $this->makeComponent($target, $attributes);
        $component->withAttributes(new AttributeBag($remaining));
        if (!$component->shouldRender()) {
            return '';
        }
        $data = $this->componentData($component) + ['attributes' => $component->attributes, 'component' => $component] + $slots;
        $view = $component->render();
        return match (true) {
            $view instanceof View => $renderer->render($view->name, $view->data + $data),
            $view instanceof Htmlable => $view->toHtml(),
            default => $renderer->render($view, $data),
        };
    }

    /**
     * @template T of Component
     * @param class-string<T> $class
     * @param array<string, mixed> $attributes
     * @return array{T, array<string, mixed>}
     */
    private function makeComponent(string $class, array $attributes): array
    {
        $plan = $this->componentPlan($class);
        $arguments = [];
        foreach ($plan['params'] as $parameter) {
            foreach ([$parameter['name'], $parameter['kebab']] as $key) {
                if (array_key_exists($key, $attributes)) {
                    $arguments[$parameter['name']] = $attributes[$key];
                    unset($attributes[$parameter['name']], $attributes[$parameter['kebab']]);
                    continue 2;
                }
            }
            if ($parameter['class'] !== null && $this->container !== null && ($this->container->has($parameter['class']) || class_exists($parameter['class']))) {
                $arguments[$parameter['name']] = $this->container->get($parameter['class']);
            } elseif ($parameter['optional']) {
                $arguments[$parameter['name']] = $parameter['default'];
            } elseif ($parameter['nullable']) {
                $arguments[$parameter['name']] = null;
            } else {
                throw new \InvalidArgumentException("Component {$class} requires the \"{$parameter['kebab']}\" attribute.");
            }
        }
        return [new $class(...$arguments), $attributes];
    }

    /** @return array<string, mixed> */
    private function componentData(Component $component): array
    {
        $plan = $this->componentPlan($component::class);
        $data = [];
        foreach ($plan['properties'] as $property) {
            $data[$property] = $component->{$property};
        }
        foreach ($plan['methods'] as $method) {
            $data[$method] = $component->{$method}(...);
        }
        return $data;
    }

    /**
     * @param class-string<Component> $class
     * @return array{params: list<array{name: string, kebab: string, class: ?string, optional: bool, default: mixed, nullable: bool}>, properties: list<string>, methods: list<string>}
     */
    private function componentPlan(string $class): array
    {
        if (isset($this->componentPlans[$class])) {
            return $this->componentPlans[$class];
        }
        $reflection = new \ReflectionClass($class);
        $params = [];
        foreach ($reflection->getConstructor()?->getParameters() ?? [] as $parameter) {
            $type = $parameter->getType();
            $params[] = [
                'name' => $parameter->getName(),
                'kebab' => strtolower((string) preg_replace('/(?<!^)[A-Z]/', '-$0', $parameter->getName())),
                'class' => $type instanceof \ReflectionNamedType && !$type->isBuiltin() ? $type->getName() : null,
                'optional' => $parameter->isDefaultValueAvailable(),
                'default' => $parameter->isDefaultValueAvailable() ? $parameter->getDefaultValue() : null,
                'nullable' => $parameter->allowsNull(),
            ];
        }
        $properties = [];
        foreach ($reflection->getProperties(\ReflectionProperty::IS_PUBLIC) as $property) {
            if (!$property->isStatic() && $property->getName() !== 'attributes') {
                $properties[] = $property->getName();
            }
        }
        $methods = [];
        foreach ($reflection->getMethods(\ReflectionMethod::IS_PUBLIC) as $method) {
            if (!$method->isStatic() && !str_starts_with($method->getName(), '__')
                && $method->getDeclaringClass()->getName() !== Component::class) {
                $methods[] = $method->getName();
            }
        }
        return $this->componentPlans[$class] = ['params' => $params, 'properties' => $properties, 'methods' => $methods];
    }

    private function find(string $view): ?string
    {
        $namespace = '';
        $name = $view;
        $separator = strpos($view, '::');
        if ($separator !== false) {
            $namespace = substr($view, 0, $separator);
            $name = substr($view, $separator + 2);
        }
        if ($name === '' || str_contains($name, '..') || str_contains($name, "\0")) {
            return null;
        }
        $relative = str_replace('.', '/', $name) . self::EXTENSION;
        foreach ($this->paths[$namespace] ?? [] as $root) {
            if (is_file($root . '/' . $relative)) {
                return $root . '/' . $relative;
            }
        }
        return null;
    }

    /**
     * Attributes that are valid variable names, for anonymous components without `@props`.
     *
     * @param array<string, mixed> $attributes
     * @return array<string, mixed>
     */
    private static function variables(array $attributes): array
    {
        $variables = [];
        foreach ($attributes as $name => $value) {
            $camel = lcfirst(str_replace(' ', '', ucwords(str_replace(['-', '_'], ' ', $name))));
            if (preg_match('/^[a-zA-Z_]\w*$/', $camel) === 1 && $camel !== 'this') {
                $variables[$camel] = $value;
            }
        }
        return $variables;
    }

    /**
     * @param string|list<string> $paths
     * @return list<string>
     */
    private static function normalizePaths(string|array $paths): array
    {
        $normalized = [];
        foreach ((array) $paths as $path) {
            if ($path === '') {
                throw new \InvalidArgumentException('A view path cannot be empty.');
            }
            $normalized[] = rtrim($path, '/\\');
        }
        if ($normalized === []) {
            throw new \InvalidArgumentException('At least one view path is required.');
        }
        return $normalized;
    }
}
