<?php

declare(strict_types=1);

namespace Nimbus\Tests\View;

use Nimbus\View\Compiler;
use Nimbus\View\Renderer;
use Nimbus\View\ViewException;
use Nimbus\View\ViewFactory;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

#[CoversClass(Compiler::class)]
#[CoversClass(Renderer::class)]
#[CoversClass(ViewFactory::class)]
final class CompilerTest extends TestCase
{
    private ViewFactory $views;

    protected function setUp(): void
    {
        $this->views = new ViewFactory(dirname(__DIR__) . '/Fixtures/views', componentNamespace: 'Nimbus\\Tests\\Fixtures\\ViewComponents');
    }

    public function testEchoesEscapeAndRawEchoesDoNot(): void
    {
        self::assertSame(
            '&lt;b&gt;&quot;x&quot;&amp;&#039; <b>raw</b> {{ $kept }} @if',
            $this->render('{{ $html }} {!! $raw !!} @{{ $kept }} @@if', ['html' => '<b>"x"&\'', 'raw' => '<b>raw</b>']),
        );
        self::assertSame('a}}b', $this->render('{{ "a}}b" }}'));
        self::assertSame('', $this->render('{{-- a comment {{ $x }} --}}'));
        self::assertSame('user@example.com', $this->render('user@example.com'));
        self::assertSame('@media (x) {}', $this->render('@media (x) {}'));
        self::assertSame('{{ $raw }}', $this->render('@verbatim{{ $raw }}@endverbatim'));
    }

    public function testConditionals(): void
    {
        $template = '@if ($n > 1)many @elseif ($n === 1)one @else none @endif'
            . ' @unless ($n)zero @endunless @isset($missing)set @endisset @empty($list)empty @endempty';
        self::assertSame('many    ', $this->render($template, ['n' => 2, 'list' => [1]]));
        self::assertSame('one    ', $this->render($template, ['n' => 1, 'list' => [1]]));
        self::assertSame(' none  zero   empty ', $this->render($template, ['n' => 0, 'list' => []]));
        self::assertSame('user@endif', $this->render('user@endif'), 'A directive needs a non-word character before it.');
    }

    public function testLoopsExposeTheLoopVariable(): void
    {
        $template = '@foreach ($items as $key => $item){{ $loop->iteration }}/{{ $loop->count }}:{{ $key }}={{ $item }}'
            . ' @if ($loop->first)F @endif @if ($loop->last)L @endif;@endforeach';
        self::assertSame('1/2:a=x F  ;2/2:b=y  L ;', $this->render($template, ['items' => ['a' => 'x', 'b' => 'y']]));

        $nested = '@foreach ($rows as $row)@foreach ($row as $cell){{ $loop->depth }}{{ $loop->parent?->index }}{{ $cell }} @endforeach @endforeach';
        self::assertSame('200 201  211  ', $this->render($nested, ['rows' => [[0, 1], [1]]]));

        $forelse = '@forelse ($items as $item){{ $item }}@empty nothing @endforelse';
        self::assertSame('ab', $this->render($forelse, ['items' => ['a', 'b']]));
        self::assertSame(' nothing ', $this->render($forelse, ['items' => []]));

        self::assertSame('0 1 3 ', $this->render('@for ($i = 0; $i < 5; $i++)@continue($i === 2)@break($i === 4){{ $i }} @endfor'));
        self::assertSame('21', $this->render('@php($i = 2)@while ($i > 0){{ $i-- }}@endwhile'));
    }

    public function testPhpBlocksAndHelpersDirectives(): void
    {
        self::assertSame('6', $this->render("@php\n\$a = 2;\n\$b = 3;\n@endphp{{ \$a * \$b }}"));
        self::assertSame('{"a":"\u003C\/b\u003E"}', $this->render('@json($data)', ['data' => ['a' => '</b>']]));
        self::assertSame('class="p-4 bold"', $this->render("@class(['p-4', 'bold' => true, 'hidden' => false])"));
        self::assertSame('<option selected>', $this->render('<option @selected($on)>', ['on' => true]));
        self::assertSame('<input type="hidden" name="_method" value="DELETE">', $this->render("@method('delete')"));
    }

    public function testCompiledLinesMatchTheTemplate(): void
    {
        $source = "line 1\n{{ \$a }}\n@if (\$a)\n<x-alert\n type=\"x\"\n/>\n@endif\n{{ \$x->missing() }}";
        try {
            $this->views->renderString($source, ['a' => true, 'x' => null]);
            self::fail('The template must fail.');
        } catch (ViewException $error) {
            self::assertSame(8, $error->templateLine);
            self::assertSame('inline', $error->template);
        }
    }

    /** @return iterable<string, array{string, string, int}> */
    public static function invalidTemplates(): iterable
    {
        yield 'unclosed if' => ["a\n@if (\$x)\nb", '@if is never closed', 2];
        yield 'wrong closer' => ["@if (\$x)\n@endforeach", 'Unexpected @endforeach, expected @endif (for @if on line 1)', 2];
        yield 'else outside' => ["\n\n@else", '@else outside', 3];
        yield 'unclosed echo' => ["\n{{ \$x", 'Unclosed {{', 2];
        yield 'empty outside forelse' => ['@empty', '@empty without arguments must be inside @forelse', 1];
        yield 'mismatched component' => ["<x-alert>\n</x-card>", 'Unexpected </x-card>, expected </x-alert>', 2];
        yield 'slot outside component' => ['<x-slot:footer>x</x-slot>', '<x-slot> must be inside a component', 1];
        yield 'missing arguments' => ['@include', '@include requires arguments', 1];
        yield 'unclosed component' => ["\n<x-alert>", '<x-alert> is never closed', 2];
        yield 'syntax error' => ["\n\n{{ \$a + }}", 'Syntax error', 3];
    }

    #[DataProvider('invalidTemplates')]
    public function testCompileErrorsCarryTheLine(string $source, string $message, int $line): void
    {
        try {
            ViewFactory::evaluate(Compiler::compile($source, 'test.nim.php')->code, 'test.nim.php');
            self::fail('The template must not compile.');
        } catch (ViewException $error) {
            self::assertStringContainsString($message, $error->reason());
            self::assertSame($line, $error->templateLine);
            self::assertSame('test.nim.php', $error->template);
        }
    }

    public function testReferencesAreRecorded(): void
    {
        $compiled = Compiler::compile("@extends('layouts.app')\n@include('partials.item')\n<x-alert />\n@include(\$dynamic)");

        self::assertSame([
            ['kind' => 'extends', 'name' => 'layouts.app', 'line' => 1],
            ['kind' => 'include', 'name' => 'partials.item', 'line' => 2],
            ['kind' => 'component', 'name' => 'alert', 'line' => 3],
        ], $compiled->references);
    }

    /** @param array<string, mixed> $data */
    private function render(string $source, array $data = []): string
    {
        return $this->views->renderString($source, $data);
    }
}
