<?php

declare(strict_types=1);

namespace Nimbus\Tests\Translation;

use Nimbus\App;
use Nimbus\Container\Container;
use Nimbus\Session\MemorySessionStore;
use Nimbus\Session\Session;
use Nimbus\Session\SessionServiceProvider;
use Nimbus\Testing\TestClient;
use Nimbus\Translation\LangDirectoryLoader;
use Nimbus\Translation\Locale;
use Nimbus\Translation\LocaleMiddleware;
use Nimbus\Translation\TranslationServiceProvider;
use Nimbus\Translation\Translator;
use Nimbus\View\ViewFactory;
use Nimbus\View\ViewServiceProvider;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Translator::class)]
#[CoversClass(LangDirectoryLoader::class)]
#[CoversClass(Locale::class)]
#[CoversClass(LocaleMiddleware::class)]
#[CoversClass(TranslationServiceProvider::class)]
final class TranslatorTest extends TestCase
{
    public function testKeysPlaceholdersPluralsAndFallbacks(): void
    {
        $translator = new Translator(self::path(), 'en');

        self::assertSame(['en', 'pt_BR'], $translator->supportedLocales());
        self::assertSame('Welcome, Ana!', $translator->get('messages.welcome', ['name' => 'Ana']));
        self::assertSame('Bem-vindo, Ana!', $translator->get('messages.welcome', ['name' => 'Ana'], 'pt_BR'));
        self::assertSame('Bem-vindo, ANA!', $translator->get('Bem-vindo, :NAME!', ['name' => 'ana'], 'pt_BR'));
        self::assertSame('Título aninhado', $translator->get('messages.nested.title', [], 'pt-br'));
        self::assertSame('Only in English', $translator->get('messages.only_en', [], 'pt_BR'));
        self::assertSame('Salvar alterações', $translator->get('Save changes', [], 'pt_BR'));
        self::assertSame('Olá Bia', $translator->get('Hello {name}', ['name' => 'Bia'], 'pt_BR'));
        self::assertSame('missing.key', $translator->get('missing.key'));
        self::assertSame('Nenhuma maçã', $translator->choice('messages.apples', 0, [], 'pt_BR'));
        self::assertSame('One apple', $translator->choice('messages.apples', 1));
        self::assertSame('5 maçãs', $translator->choice('messages.apples', 5, [], 'pt_BR'));
        self::assertTrue($translator->has('messages.welcome'));
        self::assertFalse($translator->has('messages.nope'));
    }

    public function testAcceptLanguageNegotiation(): void
    {
        $translator = new Translator(self::path(), 'en');

        self::assertSame('pt_BR', $translator->negotiate('pt-BR,pt;q=0.9,en;q=0.8'));
        self::assertSame('pt_BR', $translator->negotiate('pt'));
        self::assertSame('en', $translator->negotiate('fr;q=0.9, en-US;q=0.5'));
        self::assertSame('en', $translator->negotiate('pt;q=0.4, en;q=0.6'));
        self::assertNull($translator->negotiate('fr, de'));
        self::assertNull($translator->negotiate(null));
        self::assertSame('pt_BR', Locale::normalize('PT-br'));
    }

    public function testTheLocaleBelongsToTheRequest(): void
    {
        $container = new Container();
        $translator = new Translator(self::path(), 'en', [], $container);
        $results = [];
        $fibers = [];
        foreach (['pt_BR', 'en'] as $locale) {
            $fibers[] = $fiber = new \Fiber(static function () use ($container, $translator, $locale, &$results): void {
                $container->beginScope();
                $container->scopedInstance(Locale::class, new Locale($locale));
                \Fiber::suspend();
                $results[$locale] = $translator->get('messages.welcome', ['name' => 'X']);
                $container->endScope();
            });
            $fiber->start();
        }
        foreach ($fibers as $fiber) {
            $fiber->resume();
        }

        self::assertSame(['pt_BR' => 'Bem-vindo, X!', 'en' => 'Welcome, X!'], $results);
        self::assertSame('en', $translator->locale());
    }

    public function testMiddlewareHelpersAndViews(): void
    {
        $app = new App(discoverPackages: false);
        $app->provider(new SessionServiceProvider(new MemorySessionStore(), middleware: true));
        $app->provider(new TranslationServiceProvider(self::path(), 'en'));
        $app->provider(new ViewServiceProvider(dirname(__DIR__) . '/Fixtures/views'));
        $app->get('/hello', static fn (): string => __('messages.welcome', ['name' => 'Ana']));
        $app->get('/apples', static fn (): string => trans_choice('messages.apples', 2));
        $app->get('/save', static fn (): string => trans('Save changes'));
        $app->get('/locale', static function (Session $session): string {
            $session->put('locale', 'pt_BR');
            return 'ok';
        });
        $app->get('/view', static fn (ViewFactory $views): string => $views->renderString("@lang('messages.welcome', ['name' => 'Bia']) {{ __('Save changes') }}"));
        $client = new TestClient($app);

        $client->get('/hello')->assertSee('Welcome, Ana!')->assertHeader('content-language', 'en');
        $client->get('/hello', ['accept-language' => 'pt-BR,pt;q=0.9'])->assertSee('Bem-vindo, Ana!')->assertHeader('content-language', 'pt-BR');
        $client->get('/hello?lang=pt_BR')->assertSee('Bem-vindo, Ana!');
        $client->get('/hello?lang=xx')->assertSee('Welcome, Ana!');
        $client->get('/apples', ['accept-language' => 'pt'])->assertSee('2 maçãs');
        $client->get('/save', ['accept-language' => 'pt'])->assertSee('Salvar alterações');
        $client->get('/view', ['accept-language' => 'pt-BR'])->assertSee('Bem-vindo, Bia! Salvar alterações');

        $session = $client->get('/locale');
        $client->withCookiesFrom($session)->get('/hello', ['accept-language' => 'en'])->assertSee('Bem-vindo, Ana!');
    }

    private static function path(): string
    {
        return dirname(__DIR__) . '/Fixtures/lang';
    }
}
