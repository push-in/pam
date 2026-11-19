<?php

declare(strict_types=1);

namespace Nimbus\OpenApi;

use Nimbus\Route;
use Nimbus\Router;
use Nimbus\Validation\Input;

final readonly class OpenApiGenerator
{
    public function __construct(
        private Router $router,
        private string $title = 'Nimbus API',
        private string $version = '1.0.0',
    ) {
    }

    /** @return array<string, mixed> */
    public function generate(): array
    {
        $paths = [];
        $schemas = [];
        foreach ($this->router->routes() as $route) {
            $route->input ??= self::inputParameter($route);
            $paths[$route->path][strtolower($route->method)] = $this->operation($route);
            foreach ([$route->input, $route->output] as $class) {
                if ($class !== null) {
                    $schemas[self::shortName($class)] = is_a($class, Input::class, true)
                        ? $class::schema()
                        : ['type' => 'object', 'x-pam-class' => $class];
                }
            }
        }
        ksort($paths, SORT_STRING);
        $document = [
            'openapi' => '3.1.0',
            'info' => ['title' => $this->title, 'version' => $this->version],
            'paths' => $paths,
        ];
        if ($schemas !== []) {
            ksort($schemas, SORT_STRING);
            $document['components'] = ['schemas' => $schemas];
        }
        return $document;
    }

    public function toJson(): string
    {
        return json_encode(
            $this->generate(),
            JSON_THROW_ON_ERROR | JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES,
        ) . "\n";
    }

    public function client(ClientLanguage $language): string
    {
        return (new ClientGenerator($this->generate()))->generate($language);
    }

    /** @return array<string, mixed> */
    private function operation(Route $route): array
    {
        $operation = [
            'operationId' => $route->name ?? strtolower($route->method) . str_replace(['/', '{', '}'], ['.', '', ''], $route->path),
            'responses' => [
                '200' => ['description' => 'Successful response'],
                '422' => ['description' => 'Validation failed'],
                '500' => ['description' => 'Internal server error'],
            ],
        ];
        if ($route->summary !== null) {
            $operation['summary'] = $route->summary;
        }
        if ($route->tags !== []) {
            $operation['tags'] = $route->tags;
        }
        if ($route->parameterNames !== []) {
            $operation['parameters'] = array_map(
                static fn (string $name): array => [
                    'name' => $name,
                    'in' => 'path',
                    'required' => true,
                    'schema' => ['type' => 'string'],
                ],
                $route->parameterNames,
            );
        }
        if ($route->input !== null && is_a($route->input, Input::class, true) && in_array($route->method, ['GET', 'HEAD', 'DELETE'], true)) {
            $schema = $route->input::schema();
            $required = is_array($schema['required'] ?? null) ? $schema['required'] : [];
            $properties = is_array($schema['properties'] ?? null) ? $schema['properties'] : [];
            foreach ($properties as $name => $property) {
                $operation['parameters'][] = [
                    'name' => $name,
                    'in' => 'query',
                    'required' => in_array($name, $required, true),
                    'schema' => $property,
                ];
            }
        } elseif ($route->input !== null) {
            $operation['requestBody'] = [
                'required' => true,
                'content' => [
                    'application/json' => [
                        'schema' => ['$ref' => '#/components/schemas/' . self::shortName($route->input)],
                    ],
                ],
                'x-pam-class' => $route->input,
            ];
        }
        if ($route->output !== null) {
            $operation['responses']['200']['content']['application/json']['schema'] = [
                '$ref' => '#/components/schemas/' . self::shortName($route->output),
            ];
            $operation['responses']['200']['x-pam-class'] = $route->output;
        }
        return $operation;
    }

    /**
     * The Input a handler receives, if any; read once when the document is generated.
     *
     * @return ?class-string<Input>
     */
    private static function inputParameter(Route $route): ?string
    {
        $handler = $route->sourceHandler;
        try {
            $function = match (true) {
                is_array($handler) && is_string($handler[0]) => new \ReflectionMethod($handler[0], $handler[1]),
                is_string($handler) && method_exists($handler, '__invoke') => new \ReflectionMethod($handler, '__invoke'),
                $handler instanceof \Closure => new \ReflectionFunction($handler),
                default => null,
            };
        } catch (\ReflectionException) {
            return null;
        }
        foreach ($function?->getParameters() ?? [] as $parameter) {
            $type = $parameter->getType();
            if ($type instanceof \ReflectionNamedType) {
                $class = $type->getName();
                if (is_a($class, Input::class, true)) {
                    return $class;
                }
            }
        }
        return null;
    }

    /** @param class-string $class */
    private static function shortName(string $class): string
    {
        return (new \ReflectionClass($class))->getShortName();
    }
}
