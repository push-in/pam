<?php

declare(strict_types=1);

namespace Nimbus\View;

/**
 * Compiles a `.nim.php` template into the body of a PHP closure. Output is appended to a
 * string (never `echo`/output buffering, which fibers would interleave), and every source
 * line stays on the same line of the compiled code, so PHP errors point at the template line.
 */
final class Compiler
{
    private const string TOKEN = '~\{\{--|@\{\{|@\{!!|\{!!|\{\{|(?<![\w@])@@?[a-zA-Z_]\w*|</?x-[\w\-:.]+~';

    private const array CLOSERS = [
        'if' => ['endif'],
        'unless' => ['endunless'],
        'isset' => ['endisset'],
        'empty' => ['endempty'],
        'foreach' => ['endforeach'],
        'forelse' => ['endforelse'],
        'for' => ['endfor'],
        'while' => ['endwhile'],
        'auth' => ['endauth'],
        'guest' => ['endguest'],
        'can' => ['endcan'],
        'cannot' => ['endcannot'],
        'error' => ['enderror'],
        'section' => ['endsection', 'stop', 'show', 'append'],
        'push' => ['endpush'],
        'prepend' => ['endprepend'],
    ];

    private const array CONDITIONALS = ['if', 'unless', 'isset', 'empty', 'auth', 'guest', 'can', 'cannot', 'error'];

    /** Directives that take an argument list in parentheses (required or optional). */
    private const array ARGUMENTS = [
        'if', 'elseif', 'unless', 'isset', 'empty', 'foreach', 'forelse', 'for', 'while', 'continue', 'break',
        'auth', 'guest', 'can', 'elsecan', 'cannot', 'elsecannot', 'error', 'include', 'includeIf', 'includeWhen',
        'push', 'prepend', 'stack', 'php', 'extends', 'section', 'yield', 'hasSection', 'sectionMissing', 'props',
        'json', 'class', 'checked', 'selected', 'disabled', 'readonly', 'required', 'method', 'lang',
    ];

    private const array REQUIRED = [
        'if', 'elseif', 'unless', 'isset', 'foreach', 'forelse', 'for', 'while', 'can', 'elsecan', 'cannot',
        'elsecannot', 'error', 'include', 'includeIf', 'includeWhen', 'push', 'prepend', 'stack', 'extends', 'section',
        'yield', 'hasSection', 'sectionMissing', 'props', 'json', 'class', 'checked', 'selected', 'disabled', 'readonly',
        'required', 'method', 'lang',
    ];

    private string $code = '';

    private string $text = '';

    /** @var list<array{type: string, line: int, name?: string, empty?: bool, id?: int}> */
    private array $blocks = [];

    /** @var list<array{kind: string, name: string, line: int}> */
    private array $references = [];

    private int $counter = 0;

    private function __construct(
        private readonly string $source,
        private readonly string $template,
    ) {
    }

    /** @throws ViewException with the template line of the first error. */
    public static function compile(string $source, string $template = 'inline'): CompiledTemplate
    {
        return (new self($source, $template))->run();
    }

    private function run(): CompiledTemplate
    {
        $source = $this->source;
        $offset = 0;
        $length = strlen($source);
        while ($offset < $length && preg_match(self::TOKEN, $source, $match, PREG_OFFSET_CAPTURE, $offset) === 1) {
            [$token, $position] = $match[0];
            $this->text .= substr($source, $offset, $position - $offset);
            $offset = $this->token($token, $position);
        }
        $this->text .= substr($source, $offset);
        $this->flush();
        if ($this->blocks !== []) {
            $block = end($this->blocks);
            $label = $block['type'] === 'component' ? '<x-' . ($block['name'] ?? '') . '>' : '@' . $block['type'];
            throw new ViewException("{$label} is never closed", $this->template, $block['line']);
        }
        $body = 'return static function (\\Nimbus\\View\\Renderer $__env, array $__data): string { '
            . 'extract($__data, EXTR_SKIP); $__out = \'\'; '
            . $this->code
            . "\nreturn \$__out; };";
        return new CompiledTemplate($body, $this->references);
    }

    /** Compiles the token at $position and returns the offset after it. */
    private function token(string $token, int $position): int
    {
        $source = $this->source;
        if ($token === '{{--') {
            $end = strpos($source, '--}}', $position + 4);
            if ($end === false) {
                throw $this->error('Unclosed {{-- comment', $position);
            }
            $this->flush();
            $this->code .= str_repeat("\n", substr_count($source, "\n", $position, $end + 4 - $position));
            return $end + 4;
        }
        if ($token === '@{{' || $token === '@{!!') {
            $close = $token === '@{{' ? '}}' : '!!}';
            $end = $this->find($close, $position + strlen($token)) ?? throw $this->error("Unclosed {$token}", $position);
            $this->text .= substr($source, $position + 1, $end + strlen($close) - $position - 1);
            return $end + strlen($close);
        }
        if ($token === '{{' || $token === '{!!') {
            $close = $token === '{{' ? '}}' : '!!}';
            $start = $position + strlen($token);
            $end = $this->find($close, $start) ?? throw $this->error("Unclosed {$token}", $position);
            $expression = trim(substr($source, $start, $end - $start));
            if ($expression === '') {
                throw $this->error("Empty {$token} {$close}", $position);
            }
            $this->php($token === '{{' ? "\$__out .= \$__env->e({$expression});" : "\$__out .= (string) ({$expression});", substr($source, $start, $end - $start));
            return $end + strlen($close);
        }
        if (str_starts_with($token, '</x-')) {
            return $this->closeComponent($token, $position);
        }
        if (str_starts_with($token, '<x-')) {
            return $this->openComponent($token, $position);
        }
        if (str_starts_with($token, '@@')) {
            $this->text .= substr($token, 1);
            return $position + strlen($token);
        }
        return $this->directive(substr($token, 1), $position);
    }

    private function directive(string $name, int $position): int
    {
        $after = $position + 1 + strlen($name);
        if ($name === 'verbatim') {
            $end = strpos($this->source, '@endverbatim', $after);
            if ($end === false) {
                throw $this->error('@verbatim is never closed', $position);
            }
            $this->text .= substr($this->source, $after, $end - $after);
            return $end + strlen('@endverbatim');
        }
        if (!$this->known($name)) {
            $this->text .= '@' . $name;
            return $after;
        }
        $arguments = null;
        $raw = '';
        if (in_array($name, self::ARGUMENTS, true)) {
            $open = $after;
            while ($open < strlen($this->source) && ($this->source[$open] === ' ' || $this->source[$open] === "\t")) {
                ++$open;
            }
            if (($this->source[$open] ?? '') === '(') {
                $close = $this->matchParenthesis($open);
                $raw = substr($this->source, $open, $close + 1 - $open);
                $arguments = substr($this->source, $open + 1, $close - $open - 1);
                $after = $close + 1;
            }
        }
        if ($name === 'php' && $arguments === null) {
            $end = strpos($this->source, '@endphp', $after);
            if ($end === false) {
                throw $this->error('@php is never closed', $position);
            }
            $this->php(substr($this->source, $after, $end - $after) . ';', '');
            return $end + strlen('@endphp');
        }
        if ($arguments !== null && trim($arguments) === '' && in_array($name, self::REQUIRED, true)) {
            throw $this->error("@{$name} requires arguments", $position);
        }
        if ($arguments === null && in_array($name, self::REQUIRED, true)) {
            throw $this->error("@{$name} requires arguments in parentheses", $position);
        }
        $this->php($this->compileDirective($name, $arguments, $position), $raw);
        return $after;
    }

    private function compileDirective(string $name, ?string $arguments, int $position): string
    {
        $a = $arguments ?? '';
        switch ($name) {
            case 'if':
            case 'hasSection':
            case 'sectionMissing':
                $this->open('if', $position);
                return match ($name) {
                    'if' => "if ({$a}):",
                    'hasSection' => "if (\$__env->hasSection({$a})):",
                    default => "if (!\$__env->hasSection({$a})):",
                };
            case 'elseif':
                $this->expectInside(['if'], $name, $position);
                return "elseif ({$a}):";
            case 'else':
                $this->expectInside(self::CONDITIONALS, $name, $position);
                return 'else:';
            case 'unless':
                $this->open('unless', $position);
                return "if (!({$a})):";
            case 'isset':
                $this->open('isset', $position);
                return "if (isset({$a})):";
            case 'empty':
                $top = end($this->blocks);
                if ($arguments === null) {
                    if ($top === false || $top['type'] !== 'forelse' || ($top['empty'] ?? false)) {
                        throw $this->error('@empty without arguments must be inside @forelse', $position);
                    }
                    array_pop($this->blocks);
                    $top['empty'] = true;
                    $this->blocks[] = $top;
                    $id = $top['id'] ?? 0;
                    return "endforeach; \$__env->popLoop(); \$loop = \$__env->lastLoop(); if (\$__empty_{$id}):";
                }
                $this->open('empty', $position);
                return "if (empty({$a})):";
            case 'foreach':
            case 'forelse':
                [$subject, $iteration] = $this->splitForeach($a, $position);
                $id = ++$this->counter;
                $this->open($name, $position, ['id' => $id, 'empty' => false]);
                $empty = $name === 'forelse' ? "\$__empty_{$id} = true; " : '';
                $mark = $name === 'forelse' ? "\$__empty_{$id} = false; " : '';
                return "{$empty}\$__loopData_{$id} = {$subject}; \$__env->addLoop(\$__loopData_{$id}); "
                    . "foreach (\$__loopData_{$id} as {$iteration}): {$mark}\$__env->incrementLoopIndices(); \$loop = \$__env->lastLoop();";
            case 'for':
                $this->open('for', $position);
                return "for ({$a}):";
            case 'while':
                $this->open('while', $position);
                return "while ({$a}):";
            case 'continue':
            case 'break':
                $this->expectInside(['foreach', 'forelse', 'for', 'while'], $name, $position, true);
                return $arguments === null ? "{$name};" : "if ({$a}) { {$name}; }";
            case 'auth':
                $this->open('auth', $position);
                return 'if ($__env->check()):';
            case 'guest':
                $this->open('guest', $position);
                return 'if (!$__env->check()):';
            case 'can':
                $this->open('can', $position);
                return "if (\$__env->can({$a})):";
            case 'cannot':
                $this->open('cannot', $position);
                return "if (!\$__env->can({$a})):";
            case 'elsecan':
                $this->expectInside(['can', 'cannot'], $name, $position);
                return "elseif (\$__env->can({$a})):";
            case 'elsecannot':
                $this->expectInside(['can', 'cannot'], $name, $position);
                return "elseif (!\$__env->can({$a})):";
            case 'error':
                $this->open('error', $position);
                return "if ((\$__error = \$__env->errorMessage({$a})) !== null): \$message = \$__error;";
            case 'csrf':
                return '$__out .= $__env->csrfField();';
            case 'method':
                return "\$__out .= \$__env->methodField({$a});";
            case 'include':
            case 'includeIf':
                $this->reference('include', $a, $position, $name === 'include');
                return "\$__out .= \$__env->{$name}(get_defined_vars(), {$a});";
            case 'includeWhen':
                $parts = $this->splitArguments($a);
                if (isset($parts[1])) {
                    $this->reference('include', $parts[1], $position);
                }
                return "\$__out .= \$__env->includeWhen(get_defined_vars(), {$a});";
            case 'extends':
                $this->reference('extends', $a, $position);
                return "\$__env->extend({$a});";
            case 'section':
                if (count($this->splitArguments($a)) > 1) {
                    return "\$__env->inlineSection({$a});";
                }
                $this->open('section', $position);
                return "\$__env->startSection(\$__out, {$a}); \$__out = '';";
            case 'endsection':
            case 'stop':
            case 'append':
                $this->close($name, $position);
                return '$__out = $__env->stopSection($__out);';
            case 'show':
                $this->close($name, $position);
                return '$__out = $__env->showSection($__out);';
            case 'yield':
                return "\$__out .= \$__env->yieldContent({$a});";
            case 'parent':
                return '$__out .= $__env->parentPlaceholder();';
            case 'push':
            case 'prepend':
                $this->open($name, $position);
                return "\$__env->startPush(\$__out, " . ($name === 'prepend' ? 'true' : 'false') . ", {$a}); \$__out = '';";
            case 'endpush':
            case 'endprepend':
                $this->close($name, $position);
                return '$__out = $__env->stopPush($__out);';
            case 'stack':
                return "\$__out .= \$__env->stack({$a});";
            case 'php':
                return "{$a};";
            case 'props':
                return "extract(\$__env->props({$a}, \$attributes ?? null));";
            case 'json':
                return "\$__out .= \$__env->json({$a});";
            case 'class':
                return "\$__out .= 'class=\"' . \$__env->classes({$a}) . '\"';";
            case 'checked':
            case 'selected':
            case 'disabled':
            case 'readonly':
            case 'required':
                return "if ({$a}) { \$__out .= '{$name}'; }";
            case 'lang':
                return "\$__out .= \$__env->e(\$__env->trans({$a}));";
            case 'enderror':
                $this->close($name, $position);
                return 'endif; unset($message, $__error);';
            case 'endforelse':
                $block = $this->close($name, $position);
                return ($block['empty'] ?? false) ? 'endif;' : 'endforeach; $__env->popLoop(); $loop = $__env->lastLoop();';
            case 'endforeach':
                $this->close($name, $position);
                return 'endforeach; $__env->popLoop(); $loop = $__env->lastLoop();';
            default:
                $block = $this->close($name, $position);
                return match ($block['type']) {
                    'for' => 'endfor;',
                    'while' => 'endwhile;',
                    default => 'endif;',
                };
        }
    }

    private function known(string $name): bool
    {
        static $closers = null;
        $closers ??= array_merge(...array_values(self::CLOSERS));
        return in_array($name, self::ARGUMENTS, true)
            || in_array($name, $closers, true)
            || in_array($name, ['else', 'csrf', 'parent', 'endif', 'endphp'], true);
    }

    private function openComponent(string $token, int $position): int
    {
        $name = substr($token, 3);
        [$attributes, $selfClosing, $end] = $this->parseTag($position + strlen($token), $position);
        $newlines = str_repeat("\n", substr_count($this->source, "\n", $position, $end - $position));
        $slotName = null;
        if ($name === 'slot' || str_starts_with($name, 'slot:')) {
            $slotName = $name === 'slot' ? null : substr($name, 5);
            if ($slotName === null) {
                $rest = [];
                foreach ($attributes as [$attribute, $value]) {
                    if ($attribute === "'name'") {
                        $slotName = $value;
                    } else {
                        $rest[] = [$attribute, $value];
                    }
                }
                $attributes = $rest;
            } else {
                $slotName = var_export(self::camel($slotName), true);
            }
            if ($slotName === null) {
                throw $this->error('<x-slot> needs a name (<x-slot:footer> or name="footer")', $position);
            }
            if (!$this->insideComponent()) {
                throw $this->error('<x-slot> must be inside a component', $position);
            }
            $this->php("\$__env->startSlot(\$__out, {$slotName}, " . self::attributeArray($attributes) . "); \$__out = '';" . $newlines, '');
            if ($selfClosing) {
                $this->php('$__out = $__env->endSlot($__out);', '');
            } else {
                $this->open('slot', $position, ['name' => $name]);
            }
            return $end;
        }
        $this->references[] = ['kind' => 'component', 'name' => $name, 'line' => $this->line($position)];
        $this->php('$__env->startComponent($__out, ' . var_export($name, true) . ', ' . self::attributeArray($attributes) . "); \$__out = '';" . $newlines, '');
        if ($selfClosing) {
            $this->php("\$__out = \$__env->renderComponent('');", '');
        } else {
            $this->open('component', $position, ['name' => $name]);
        }
        return $end;
    }

    private function closeComponent(string $token, int $position): int
    {
        $name = substr($token, 4);
        $end = $position + strlen($token);
        while ($end < strlen($this->source) && ctype_space($this->source[$end])) {
            ++$end;
        }
        if (($this->source[$end] ?? '') !== '>') {
            throw $this->error("Malformed closing tag </x-{$name}", $position);
        }
        $top = end($this->blocks);
        $isSlot = $name === 'slot' || str_starts_with($name, 'slot:');
        if ($top === false || $top['type'] !== ($isSlot ? 'slot' : 'component') || (!$isSlot && ($top['name'] ?? '') !== $name)) {
            $expected = $top === false ? 'nothing' : ($top['type'] === 'component' ? '</x-' . ($top['name'] ?? '') . '>' : ($top['type'] === 'slot' ? '</x-slot>' : '@end' . $top['type']));
            throw $this->error("Unexpected </x-{$name}>, expected {$expected}", $position);
        }
        array_pop($this->blocks);
        $this->php($isSlot ? '$__out = $__env->endSlot($__out);' : '$__out = $__env->renderComponent($__out);', '');
        return $end + 1;
    }

    /**
     * Parses the attributes of a component tag.
     *
     * @return array{list<array{string, string}>, bool, int} [[name code, value code]], self-closing, end offset
     */
    private function parseTag(int $offset, int $position): array
    {
        $source = $this->source;
        $length = strlen($source);
        $attributes = [];
        while (true) {
            while ($offset < $length && ctype_space($source[$offset])) {
                ++$offset;
            }
            if ($offset >= $length) {
                throw $this->error('Unclosed component tag', $position);
            }
            if (substr($source, $offset, 2) === '/>') {
                return [$attributes, true, $offset + 2];
            }
            if ($source[$offset] === '>') {
                return [$attributes, false, $offset + 1];
            }
            if (substr($source, $offset, 2) === '{{') {
                $end = $this->find('}}', $offset + 2) ?? throw $this->error('Unclosed {{ in component tag', $offset);
                $attributes[] = ['...', '(' . trim(substr($source, $offset + 2, $end - $offset - 2)) . ')->getAttributes()'];
                $offset = $end + 2;
                continue;
            }
            if (preg_match('~[^\s=/>"\']+~A', $source, $match, 0, $offset) !== 1) {
                throw $this->error('Malformed attribute in component tag', $offset);
            }
            $name = $match[0];
            $offset += strlen($name);
            $value = null;
            if (($source[$offset] ?? '') === '=') {
                ++$offset;
                $quote = $source[$offset] ?? '';
                if ($quote === '"' || $quote === "'") {
                    $close = strpos($source, $quote, $offset + 1);
                    if ($close === false) {
                        throw $this->error("Unclosed attribute {$name}", $offset);
                    }
                    $value = substr($source, $offset + 1, $close - $offset - 1);
                    $offset = $close + 1;
                } else {
                    $value = preg_match('~[^\s>]+~A', $source, $bare, 0, $offset) === 1 ? $bare[0] : '';
                    $offset += strlen($value);
                }
            }
            if (str_starts_with($name, '::')) {
                $attributes[] = [var_export(substr($name, 1), true), $value === null ? 'true' : self::interpolate($value)];
            } elseif (str_starts_with($name, ':')) {
                $attributes[] = [var_export(substr($name, 1), true), $value === null || trim($value) === '' ? 'true' : '(' . $value . ')'];
            } else {
                $attributes[] = [var_export($name, true), $value === null ? 'true' : self::interpolate($value)];
            }
        }
    }

    /** @param list<array{string, string}> $attributes */
    private static function attributeArray(array $attributes): string
    {
        $items = [];
        foreach ($attributes as [$name, $value]) {
            $items[] = $name === '...' ? '...' . $value : "{$name} => {$value}";
        }
        return '[' . implode(', ', $items) . ']';
    }

    /** An attribute value with `{{ }}` parts as a PHP expression. */
    private static function interpolate(string $value): string
    {
        if (preg_match('~^\{\{\s*(.+?)\s*\}\}$~s', $value, $only) === 1) {
            return '(' . $only[1] . ')';
        }
        $parts = preg_split('~\{\{\s*(.+?)\s*\}\}~s', $value, -1, PREG_SPLIT_DELIM_CAPTURE) ?: [$value];
        $code = [];
        foreach ($parts as $index => $part) {
            if ($index % 2 === 1) {
                $code[] = '(string) (' . $part . ')';
            } elseif ($part !== '') {
                $code[] = var_export($part, true);
            }
        }
        return $code === [] ? "''" : implode(' . ', $code);
    }

    private static function camel(string $name): string
    {
        return lcfirst(str_replace(' ', '', ucwords(str_replace(['-', '_'], ' ', $name))));
    }

    private function insideComponent(): bool
    {
        $top = end($this->blocks);
        return $top !== false && $top['type'] === 'component';
    }

    /** @return array{string, string} */
    private function splitForeach(string $arguments, int $position): array
    {
        if (preg_match('~^(.+?)\s+as\s+(.+)$~s', trim($arguments), $match) !== 1) {
            throw $this->error('@foreach expects "$items as $item"', $position);
        }
        return [$match[1], $match[2]];
    }

    /** @return list<string> */
    private function splitArguments(string $arguments): array
    {
        $parts = [];
        $depth = 0;
        $current = '';
        $length = strlen($arguments);
        for ($i = 0; $i < $length; ++$i) {
            $char = $arguments[$i];
            if ($char === '"' || $char === "'") {
                $end = self::stringEnd($arguments, $i);
                $current .= substr($arguments, $i, $end - $i + 1);
                $i = $end;
                continue;
            }
            if ($char === '(' || $char === '[' || $char === '{') {
                ++$depth;
            } elseif ($char === ')' || $char === ']' || $char === '}') {
                --$depth;
            } elseif ($char === ',' && $depth === 0) {
                $parts[] = trim($current);
                $current = '';
                continue;
            }
            $current .= $char;
        }
        if (trim($current) !== '') {
            $parts[] = trim($current);
        }
        return $parts;
    }

    private function reference(string $kind, string $arguments, int $position, bool $required = true): void
    {
        if ($required && preg_match('~^\s*([\'"])([^\'"$]+)\1~', $arguments, $match) === 1) {
            $this->references[] = ['kind' => $kind, 'name' => $match[2], 'line' => $this->line($position)];
        }
    }

    /** @param array{name?: string, empty?: bool, id?: int} $extra */
    private function open(string $type, int $position, array $extra = []): void
    {
        $this->blocks[] = ['type' => $type, 'line' => $this->line($position)] + $extra;
    }

    /** @return array{type: string, line: int, name?: string, empty?: bool, id?: int} */
    private function close(string $directive, int $position): array
    {
        $block = array_pop($this->blocks);
        if ($block === null || !in_array($directive, self::CLOSERS[$block['type']] ?? [], true)) {
            $expected = match (true) {
                $block === null => 'no open block',
                $block['type'] === 'component' => '</x-' . ($block['name'] ?? '') . '>',
                $block['type'] === 'slot' => '</x-slot>',
                default => '@' . (self::CLOSERS[$block['type']][0] ?? 'end') . " (for @{$block['type']} on line {$block['line']})",
            };
            throw $this->error("Unexpected @{$directive}, expected {$expected}", $position);
        }
        return $block;
    }

    /** @param list<string> $types */
    private function expectInside(array $types, string $directive, int $position, bool $anywhere = false): void
    {
        $blocks = $anywhere ? array_reverse($this->blocks) : array_slice($this->blocks, -1);
        foreach ($blocks as $block) {
            if (in_array($block['type'], $types, true)) {
                return;
            }
        }
        throw $this->error("@{$directive} outside @" . implode('/@', $types), $position);
    }

    private function matchParenthesis(int $open): int
    {
        $depth = 0;
        $length = strlen($this->source);
        for ($i = $open; $i < $length; ++$i) {
            $char = $this->source[$i];
            if ($char === '"' || $char === "'") {
                $i = self::stringEnd($this->source, $i);
                continue;
            }
            if ($char === '(') {
                ++$depth;
            } elseif ($char === ')' && --$depth === 0) {
                return $i;
            }
        }
        throw $this->error('Unclosed parenthesis', $open);
    }

    /** Offset of $needle after $offset, skipping PHP string literals. */
    private function find(string $needle, int $offset): ?int
    {
        $length = strlen($this->source);
        for ($i = $offset; $i < $length; ++$i) {
            $char = $this->source[$i];
            if ($char === '"' || $char === "'") {
                $end = self::stringEnd($this->source, $i);
                if ($end >= $length) {
                    $plain = strpos($this->source, $needle, $offset);
                    return $plain === false ? null : $plain;
                }
                $i = $end;
                continue;
            }
            if (substr_compare($this->source, $needle, $i, strlen($needle)) === 0) {
                return $i;
            }
        }
        return null;
    }

    /** Offset of the quote that closes the string starting at $start (or the end of the text). */
    private static function stringEnd(string $text, int $start): int
    {
        $quote = $text[$start];
        $length = strlen($text);
        for ($i = $start + 1; $i < $length; ++$i) {
            if ($text[$i] === '\\') {
                ++$i;
            } elseif ($text[$i] === $quote) {
                return $i;
            }
        }
        return $length;
    }

    /** Appends compiled PHP; $source is the original text it replaces, for its line breaks. */
    private function php(string $code, string $source): void
    {
        $this->flush();
        $missing = substr_count($source, "\n") - substr_count($code, "\n");
        $this->code .= $code . ($missing > 0 ? str_repeat("\n", $missing) : '') . ' ';
    }

    private function flush(): void
    {
        if ($this->text !== '') {
            $this->code .= "\$__out .= '" . addcslashes($this->text, "\\'") . "'; ";
            $this->text = '';
        }
    }

    private function line(int $position): int
    {
        return substr_count($this->source, "\n", 0, min($position, strlen($this->source))) + 1;
    }

    private function error(string $message, int $position): ViewException
    {
        return new ViewException($message, $this->template, $this->line($position));
    }
}
