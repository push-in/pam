<?php

declare(strict_types=1);

namespace Pam\Api\OpenApi;

use Pam\Api\Route;
use Pam\Api\Router;

final readonly class OpenApiGenerator
{
    public function __construct(
        private Router $router,
        private string $title = 'PAM API',
        private string $version = '1.0.0',
    ) {
    }

    /** @return array<string, mixed> */
    public function generate(): array
    {
        $paths = [];
        foreach ($this->router->routes() as $route) {
            $paths[$route->path][strtolower($route->method)] = $this->operation($route);
        }
        ksort($paths, SORT_STRING);
        return [
            'openapi' => '3.1.0',
            'info' => ['title' => $this->title, 'version' => $this->version],
            'paths' => $paths,
        ];
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
        if ($route->input !== null) {
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

    /** @param class-string $class */
    private static function shortName(string $class): string
    {
        return (new \ReflectionClass($class))->getShortName();
    }
}
