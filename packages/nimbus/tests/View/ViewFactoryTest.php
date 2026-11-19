<?php

declare(strict_types=1);

namespace Nimbus\Tests\View;

use Nimbus\App;
use Nimbus\Testing\TestClient;
use Nimbus\View\AttributeBag;
use Nimbus\View\Component;
use Nimbus\View\HtmlString;
use Nimbus\View\Loop;
use Nimbus\View\Slot;
use Nimbus\View\View;
use Nimbus\View\ViewChecker;
use Nimbus\View\ViewException;
use Nimbus\View\ViewFactory;
use Nimbus\View\ViewServiceProvider;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(ViewFactory::class)]
#[CoversClass(View::class)]
#[CoversClass(ViewChecker::class)]
#[CoversClass(ViewServiceProvider::class)]
#[CoversClass(AttributeBag::class)]
#[CoversClass(Component::class)]
#[CoversClass(HtmlString::class)]
#[CoversClass(Slot::class)]
#[CoversClass(Loop::class)]
final class ViewFactoryTest extends TestCase
{
    private const string COMPONENTS = 'Nimbus\\Tests\\Fixtures\\ViewComponents';

    private ViewFactory $views;

    protected function setUp(): void
    {
        $this->views = new ViewFactory(self::path(), componentNamespace: self::COMPONENTS);
    }

    public function testLayoutsWithSectionsStacksAndIncludes(): void
    {
        $html = $this->views->render('pages.home', ['name' => '<Ana>']);

        self::assertStringContainsString('<title>Home &amp; co</title>', $html);
        self::assertStringContainsString('<meta name="page" content="home">', $html);
        self::assertMatchesRegularExpression('~<nav>layout-nav</nav>\s*<p>child-sidebar</p>~', $html);
        self::assertStringContainsString('<h1>Hello, &lt;Ana&gt;</h1>', $html);
        self::assertStringContainsString('<li>included for &lt;Ana&gt;</li>', $html);
        self::assertStringNotContainsString('##nimbus-parent##', $html);
        self::assertStringStartsWith('<html>', $html);
    }

    public function testComponentsWithPropsAttributesAndSlots(): void
    {
        $html = $this->views->render('pages.components', ['id' => 7, 'text' => '<i>']);

        self::assertStringContainsString('<html><title>Components</title><body>', $html);
        self::assertStringContainsString('<div class="alert alert-error mb-4" role="alert" data-id="7">', $html);
        self::assertStringContainsString('<strong>Heads up</strong>', $html);
        self::assertStringContainsString('Body <b>&lt;i&gt;</b>', $html);
        self::assertStringContainsString('<button>x</button>', $html);
        self::assertStringContainsString('<footer class="small">Footer</footer>', $html);
        self::assertStringContainsString('<label>E-mail<input name="email" value="" type="email"></label>', $html);
        self::assertStringContainsString('<span class="badge badge-red" >3 items</span>', $html);
        self::assertStringNotContainsString('type="error"', $html);
    }

    public function testTemplatesCompileOncePerWorker(): void
    {
        $directory = sys_get_temp_dir() . '/nimbus-views-' . bin2hex(random_bytes(4));
        mkdir($directory);
        try {
            file_put_contents($directory . '/page.nim.php', 'first {{ $n }}');
            $views = new ViewFactory($directory);
            self::assertSame('first 1', $views->render('page', ['n' => 1]));
            file_put_contents($directory . '/page.nim.php', 'second {{ $n }}');
            self::assertSame('first 2', $views->render('page', ['n' => 2]));

            $reloading = new ViewFactory($directory, reload: true);
            self::assertSame('second 3', $reloading->render('page', ['n' => 3]));
        } finally {
            @unlink($directory . '/page.nim.php');
            @rmdir($directory);
        }
    }

    public function testRuntimeErrorsPointAtTheTemplateFileAndLine(): void
    {
        try {
            $this->views->render('partials.item', ['item' => new \stdClass()]);
            self::fail('Printing an object must fail.');
        } catch (ViewException $error) {
            self::assertSame(self::path() . '/partials/item.nim.php', $error->template);
            self::assertSame(1, $error->templateLine);
            self::assertStringContainsString('stdClass', $error->getMessage());
        }

        $this->expectException(\InvalidArgumentException::class);
        $this->views->render('pages.missing');
    }

    public function testTheCheckerReportsEveryBrokenTemplate(): void
    {
        $problems = (new ViewChecker($this->views))->check();

        self::assertCount(1, $problems);
        self::assertSame(self::path() . '/pages/broken.nim.php', $problems[0]->template);
        self::assertSame(4, $problems[0]->templateLine);
        self::assertStringContainsString('Unexpected @endforeach', $problems[0]->reason());

        $directory = sys_get_temp_dir() . '/nimbus-views-' . bin2hex(random_bytes(4));
        mkdir($directory);
        try {
            file_put_contents($directory . '/a.nim.php', "\n@include('nope')\n<x-ghost />");
            $missing = (new ViewChecker(new ViewFactory($directory)))->check();
            self::assertSame(['View [nope] in @include not found', 'Component <x-ghost> not found'], array_map(
                static fn (ViewException $problem): string => $problem->reason(),
                $missing,
            ));
            self::assertSame([2, 3], array_map(static fn (ViewException $problem): int => $problem->templateLine, $missing));
        } finally {
            @unlink($directory . '/a.nim.php');
            @rmdir($directory);
        }
    }

    public function testTheCommandLineCheckerExitsWithTheProblems(): void
    {
        $command = escapeshellarg(PHP_BINARY) . ' ' . escapeshellarg(dirname(__DIR__, 2) . '/bin/nimbus-views')
            . ' check ' . escapeshellarg(self::path()) . ' ' . escapeshellarg(self::COMPONENTS) . ' 2>&1';
        exec($command, $output, $status);

        self::assertSame(1, $status);
        self::assertStringContainsString('pages/broken.nim.php:4: Unexpected @endforeach', implode("\n", $output));
    }

    public function testViewsAreResponsesAndTheHelperUsesTheDefaultFactory(): void
    {
        $app = new App(discoverPackages: false);
        $app->provider(new ViewServiceProvider(self::path(), self::COMPONENTS));
        $app->get('/home', static fn (): View => view('pages.home', ['name' => 'Bia']));
        $app->get('/injected', static fn (ViewFactory $views): View => $views->make('partials.item')->with('item', 'x')->withStatus(201));

        $client = new TestClient($app);
        $client->get('/home')->assertOk()->assertHeader('content-type', 'text/html; charset=utf-8')->assertSee('Hello, Bia');
        $client->get('/injected')->assertCreated()->assertSee('<li>x</li>', false);
        self::assertSame('<li>y</li>', trim((string) view('partials.item', ['item' => 'y'])));
    }

    public function testAttributeBag(): void
    {
        $bag = new AttributeBag(['class' => 'b', 'id' => 'x', 'disabled' => true, 'hidden' => false, 'data-a' => '"q"']);

        self::assertSame('class="a b" id="x" disabled data-a="&quot;q&quot;"', (string) $bag->merge(['class' => 'a', 'id' => 'default']));
        self::assertSame('class="p-4 b"', $bag->only('class')->class(['p-4', 'm-2' => false])->toHtml());
        self::assertSame(['data-a' => '"q"'], $bag->whereStartsWith('data-')->getAttributes());
        self::assertSame(4, $bag->except('class')->count());
        self::assertTrue((new Slot(' '))->isEmpty());
        self::assertTrue((new HtmlString('<b>'))->isNotEmpty());
    }

    private static function path(): string
    {
        return dirname(__DIR__) . '/Fixtures/views';
    }
}
