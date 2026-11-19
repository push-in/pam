# Tradução

A tradução usa o `symfony/translation`:

```bash
pam composer require symfony/translation
```

## Registrar

```php
use Nimbus\Translation\TranslationServiceProvider;

$app->provider(new TranslationServiceProvider(
    __DIR__ . '/lang',
    getenv('APP_LOCALE') ?: 'pt_BR',          // locale padrão
    fallbackLocales: ['en'],
    supportedLocales: ['pt_BR', 'en'],
));
```

O provider registra o `LocaleMiddleware`, que escolhe o locale de cada requisição nesta ordem: `?lang=`, a chave `locale` da sessão, o cabeçalho `Accept-Language` e o padrão. Só locales suportados são aceitos. A resposta leva `Content-Language` e `Vary: Accept-Language`. O locale fica no escopo da requisição (`Nimbus\Translation\Locale`), então requisições simultâneas no mesmo worker não se misturam.

## Arquivos

```text
lang/
├── pt_BR/
│   ├── auth.php
│   └── messages.php
├── en/
│   └── ...
└── pt_BR.json       chaves em texto livre
```

```php
// lang/pt_BR/messages.php
return [
    'welcome' => 'Olá, :name',
    'apples' => '{0} Nenhuma maçã|{1} Uma maçã|[2,*] :count maçãs',
    'login' => [
        'title' => 'Entrar',
    ],
];
```

## Usar

```php
__('messages.welcome', ['name' => $user->name]);   // "Olá, Ada"
__('messages.login.title');
trans_choice('messages.apples', 5);                 // "5 maçãs"
```

Placeholders aceitam `:name`, `{name}` e `%name%`. Nas views: <span v-pre>`{{ __('messages.welcome', ['name' => $name]) }}`</span> ou `@lang('messages.login.title')`. O `Translator` também é injetável (`get()`, `choice()`, `has()`, `locale()`).

Para trocar o idioma pela sessão, como faz o template web:

```php
$router->get('locale/{locale}', static function (Request $request, Session $session): Redirect {
    $locale = (string) $request->route('locale');
    if (in_array($locale, ['pt_BR', 'en'], true)) {
        $session->put('locale', $locale);
    }
    return Redirect::to('/login');
});
```
