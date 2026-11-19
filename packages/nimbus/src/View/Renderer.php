<?php

declare(strict_types=1);

namespace Nimbus\View;

use Nimbus\Auth\AuthContext;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Gate;
use Nimbus\Auth\Principal;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Session\ErrorBag;
use Nimbus\Session\Session;
use Pam\Http\Request;

/**
 * The state of one render (sections, stacks, loops, components), created per call of
 * ViewFactory::render(), so concurrent requests in other fibers never share it.
 * Compiled templates call it as `$__env`.
 *
 * @internal
 */
final class Renderer
{
    private const string PARENT = '##nimbus-parent##';

    /** @var array<string, string> */
    private array $sections = [];

    /** @var list<array{string, string}> [name, outer buffer] */
    private array $sectionStack = [];

    /** @var array<string, list<string>> */
    private array $stacks = [];

    /** @var list<array{string, bool, string}> [name, prepend, outer buffer] */
    private array $pushStack = [];

    /** @var list<Loop> */
    private array $loops = [];

    /** @var list<array{outer: string, name: string, attributes: array<string, mixed>, slots: array<string, Slot>}> */
    private array $components = [];

    /** @var list<array{string, array<string, mixed>, string}> [name, attributes, outer buffer] */
    private array $slotStack = [];

    /** @var list<array{string, array<string, mixed>}|null> Parent layout of each template being rendered. */
    private array $parents = [];

    public function __construct(
        private readonly ViewFactory $factory,
        private readonly ?Container $container = null,
    ) {
    }

    /** @param array<string, mixed> $data */
    public function render(string $view, array $data = []): string
    {
        $data += $this->factory->shared();
        if (!array_key_exists('errors', $data)) {
            $data['errors'] = $this->errors();
        }
        $this->parents[] = null;
        try {
            $output = $this->evaluate($view, $data);
        } finally {
            $parent = array_pop($this->parents);
        }
        if ($parent !== null) {
            return $this->render($parent[0], $parent[1] + $data);
        }
        return $output;
    }

    public static function escape(mixed $value): string
    {
        return htmlspecialchars(self::string($value), ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8');
    }

    public function e(mixed $value): string
    {
        return $value instanceof Htmlable ? $value->toHtml() : self::escape($value);
    }

    /** @param array<string, mixed> $data */
    public function extend(string $view, array $data = []): void
    {
        array_pop($this->parents);
        $this->parents[] = [$view, $data];
    }

    public function startSection(string $outer, string $name): void
    {
        $this->sectionStack[] = [$name, $outer];
    }

    public function inlineSection(string $name, mixed $content): void
    {
        $this->sections[$name] ??= self::escape($content);
    }

    public function stopSection(string $content): string
    {
        [$name, $outer] = array_pop($this->sectionStack) ?? throw new \LogicException('No section is open.');
        // The first definition wins (the child renders before its layout); @parent in it
        // is replaced by the later (parent) definition.
        $this->sections[$name] = isset($this->sections[$name])
            ? str_replace(self::PARENT, $content, $this->sections[$name])
            : $content;
        return $outer;
    }

    public function showSection(string $content): string
    {
        $name = end($this->sectionStack)[0] ?? throw new \LogicException('No section is open.');
        return $this->stopSection($content) . $this->yieldContent($name);
    }

    public function yieldContent(string $name, mixed $default = ''): string
    {
        return isset($this->sections[$name])
            ? str_replace(self::PARENT, '', $this->sections[$name])
            : ($default instanceof Htmlable ? $default->toHtml() : self::escape($default));
    }

    public function hasSection(string $name): bool
    {
        return isset($this->sections[$name]) && trim($this->sections[$name]) !== '';
    }

    public function parentPlaceholder(): string
    {
        return self::PARENT;
    }

    public function startPush(string $outer, bool $prepend, string $stack): void
    {
        $this->pushStack[] = [$stack, $prepend, $outer];
    }

    public function stopPush(string $content): string
    {
        [$stack, $prepend, $outer] = array_pop($this->pushStack) ?? throw new \LogicException('No push is open.');
        $this->stacks[$stack] ??= [];
        if ($prepend) {
            array_unshift($this->stacks[$stack], $content);
        } else {
            $this->stacks[$stack][] = $content;
        }
        return $outer;
    }

    public function stack(string $name, string $default = ''): string
    {
        return isset($this->stacks[$name]) ? implode('', $this->stacks[$name]) : $default;
    }

    public function addLoop(mixed $data): void
    {
        $count = is_array($data) || $data instanceof \Countable ? count($data) : null;
        $parent = $this->lastLoop();
        $this->loops[] = new Loop($count, count($this->loops) + 1, $parent);
    }

    public function incrementLoopIndices(): void
    {
        $this->lastLoop()?->advance();
    }

    public function popLoop(): void
    {
        array_pop($this->loops);
    }

    public function lastLoop(): ?Loop
    {
        return $this->loops === [] ? null : $this->loops[array_key_last($this->loops)];
    }

    /** @param array<string, mixed> $attributes */
    public function startComponent(string $outer, string $name, array $attributes): void
    {
        $this->components[] = ['outer' => $outer, 'name' => $name, 'attributes' => $attributes, 'slots' => []];
    }

    /** @param array<string, mixed> $attributes */
    public function startSlot(string $outer, string $name, array $attributes): void
    {
        $this->slotStack[] = [$name, $attributes, $outer];
    }

    public function endSlot(string $content): string
    {
        [$name, $attributes, $outer] = array_pop($this->slotStack) ?? throw new \LogicException('No slot is open.');
        $index = array_key_last($this->components) ?? throw new \LogicException('A slot must be inside a component.');
        $this->components[$index]['slots'][$name] = new Slot(trim($content), new AttributeBag($attributes));
        return $outer;
    }

    public function renderComponent(string $content): string
    {
        $component = array_pop($this->components) ?? throw new \LogicException('No component is open.');
        $slots = $component['slots'];
        $slots['slot'] ??= new Slot(trim($content));
        return $component['outer'] . $this->factory->renderComponent($this, $component['name'], $component['attributes'], $slots);
    }

    /**
     * @param array<string, mixed> $scope
     * @param array<string, mixed> $data
     */
    public function include(array $scope, string $view, array $data = []): string
    {
        return $this->render($view, $data + self::scope($scope));
    }

    /**
     * @param array<string, mixed> $scope
     * @param array<string, mixed> $data
     */
    public function includeIf(array $scope, string $view, array $data = []): string
    {
        return $this->factory->exists($view) ? $this->include($scope, $view, $data) : '';
    }

    /**
     * @param array<string, mixed> $scope
     * @param array<string, mixed> $data
     */
    public function includeWhen(array $scope, mixed $condition, string $view, array $data = []): string
    {
        return $condition ? $this->include($scope, $view, $data) : '';
    }

    /**
     * Resolves `@props`: values come from the attributes (camelCase or kebab-case names) or
     * the defaults; what is left stays in `$attributes`.
     *
     * @param array<array-key, mixed> $definitions
     * @return array<string, mixed>
     */
    public function props(array $definitions, ?AttributeBag $attributes): array
    {
        $remaining = $attributes?->getAttributes() ?? [];
        $values = [];
        foreach ($definitions as $key => $default) {
            $name = is_int($key) ? (is_string($default) ? $default : null) : $key;
            if ($name === null) {
                continue;
            }
            $kebab = strtolower((string) preg_replace('/(?<!^)[A-Z]/', '-$0', $name));
            if (array_key_exists($name, $remaining)) {
                $values[$name] = $remaining[$name];
            } elseif (array_key_exists($kebab, $remaining)) {
                $values[$name] = $remaining[$kebab];
            } elseif (!is_int($key)) {
                $values[$name] = $default;
            } else {
                $values[$name] = null;
            }
            unset($remaining[$name], $remaining[$kebab]);
        }
        $values['attributes'] = new AttributeBag($remaining);
        return $values;
    }

    public function json(mixed $value, int $flags = 0): string
    {
        return json_encode($value, $flags | JSON_HEX_TAG | JSON_HEX_APOS | JSON_HEX_AMP | JSON_HEX_QUOT | JSON_THROW_ON_ERROR);
    }

    /** @param array<array-key, mixed> $classes */
    public function classes(array $classes): string
    {
        return self::escape(AttributeBag::classList($classes));
    }

    /** Whether a principal is authenticated (the route's, or the bound Authenticator's). */
    public function check(): bool
    {
        return $this->principal() !== null;
    }

    public function can(string $ability, mixed ...$arguments): bool
    {
        $principal = $this->principal();
        if ($principal === null || $this->container === null) {
            return false;
        }
        $gate = $this->container->get(Gate::class);
        return $gate instanceof Gate && $gate->check($principal, $ability, array_values($arguments));
    }

    public function csrfToken(): string
    {
        return $this->session()?->token()
            ?? throw new \LogicException('@csrf needs a session: add StartSessionMiddleware.');
    }

    public function csrfField(): string
    {
        return '<input type="hidden" name="_token" value="' . self::escape($this->csrfToken()) . '" autocomplete="off">';
    }

    public function methodField(string $method): string
    {
        return '<input type="hidden" name="_method" value="' . self::escape(strtoupper($method)) . '">';
    }

    public function errors(): ErrorBag
    {
        return $this->session()?->errors() ?? new ErrorBag();
    }

    public function errorMessage(string $field, ?string $bag = null): ?string
    {
        return $this->errors()->first($field);
    }

    /** @param array<string, mixed> $replace */
    public function trans(string $key, array $replace = [], ?string $locale = null): string
    {
        $translator = $this->factory->translator();
        return $translator === null ? $key : $translator->get($key, $replace, $locale);
    }

    /** @param array<string, mixed> $data */
    private function evaluate(string $view, array $data): string
    {
        [$path, $closure] = $this->factory->compiled($view);
        try {
            return $closure($this, $data);
        } catch (ViewException | HttpException $error) {
            throw $error;
        } catch (\Throwable $error) {
            throw new ViewException($error->getMessage(), $path, self::templateLine($error), $error);
        }
    }

    private static function templateLine(\Throwable $error): int
    {
        if (str_contains($error->getFile(), "eval()'d code")) {
            return $error->getLine();
        }
        foreach ($error->getTrace() as $frame) {
            if (str_contains($frame['file'] ?? '', "eval()'d code")) {
                return $frame['line'] ?? 0;
            }
        }
        return 0;
    }

    private function principal(): ?Principal
    {
        if ($this->container === null) {
            return null;
        }
        $auth = $this->container->scopedValue(AuthContext::class);
        if ($auth instanceof AuthContext) {
            return $auth->principal;
        }
        $principal = $this->container->scopedValue(Principal::class);
        if ($principal instanceof Principal) {
            return $principal;
        }
        $request = $this->container->scopedValue(Request::class);
        if (!$request instanceof Request || !$this->container->has(Authenticator::class)) {
            return null;
        }
        $authenticator = $this->container->get(Authenticator::class);
        $principal = $authenticator instanceof Authenticator ? $authenticator->authenticate($request) : null;
        if ($principal !== null) {
            $this->container->scopedInstance(Principal::class, $principal);
        }
        return $principal;
    }

    private function session(): ?Session
    {
        $session = $this->container?->scopedValue(Session::class);
        return $session instanceof Session ? $session : Session::current();
    }

    /**
     * @param array<string, mixed> $scope
     * @return array<string, mixed>
     */
    private static function scope(array $scope): array
    {
        unset($scope['__env'], $scope['__data'], $scope['__out'], $scope['loop'], $scope['__error']);
        foreach (array_keys($scope) as $name) {
            if (str_starts_with($name, '__loopData_') || str_starts_with($name, '__empty_')) {
                unset($scope[$name]);
            }
        }
        return $scope;
    }

    private static function string(mixed $value): string
    {
        return match (true) {
            $value === null => '',
            is_bool($value) => $value ? '1' : '',
            is_scalar($value) => (string) $value,
            $value instanceof \BackedEnum => (string) $value->value,
            $value instanceof \Stringable => (string) $value,
            default => throw new \InvalidArgumentException('Cannot print a value of type ' . get_debug_type($value) . '.'),
        };
    }
}
