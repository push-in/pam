<?php

declare(strict_types=1);

namespace Nimbus\View;

/**
 * Compiles every template of a factory without rendering it, for CI: syntax errors,
 * unbalanced directives and component tags, and literal `@extends`/`@include`/`<x-...>`
 * names that do not exist are reported with file and line.
 */
final readonly class ViewChecker
{
    public function __construct(private ViewFactory $factory)
    {
    }

    /** @return list<ViewException> */
    public function check(): array
    {
        $problems = [];
        foreach ($this->factory->templates() as $path) {
            $source = @file_get_contents($path);
            if (!is_string($source)) {
                $problems[] = new ViewException('The file cannot be read', $path, 0);
                continue;
            }
            try {
                $compiled = Compiler::compile($source, $path);
                ViewFactory::evaluate($compiled->code, $path);
            } catch (ViewException $problem) {
                $problems[] = $problem;
                continue;
            }
            foreach ($compiled->references as $reference) {
                $missing = match ($reference['kind']) {
                    'component' => !$this->componentExists($reference['name']),
                    default => !$this->factory->exists($reference['name']),
                };
                if ($missing) {
                    $label = $reference['kind'] === 'component' ? "Component <x-{$reference['name']}>" : "View [{$reference['name']}] in @{$reference['kind']}";
                    $problems[] = new ViewException("{$label} not found", $path, $reference['line']);
                }
            }
        }
        return $problems;
    }

    private function componentExists(string $name): bool
    {
        try {
            $this->factory->component($name);
            return true;
        } catch (\InvalidArgumentException) {
            return false;
        }
    }
}
