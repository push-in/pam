# Views `.nim.php`

O Nimbus tem um motor de templates próprio com a sintaxe do Blade. Os templates ficam em `resources/views/**/*.nim.php`, são compilados uma vez por worker em closures que montam a string de saída (sem output buffering, seguro com Fibers) e mantêm as linhas do arquivo de origem nas mensagens de erro (`ViewException` com arquivo e linha).

## Registrar

```php
use Nimbus\View\ViewServiceProvider;

$production = getenv('PAM_ENV') === 'production';

$app->provider(new ViewServiceProvider(__DIR__ . '/resources/views', reload: !$production));
```

`reload: true` recompila o template quando o arquivo muda (desenvolvimento). O segundo argumento, `componentNamespace`, é o namespace dos componentes com classe (`App\Views` por padrão).

## Renderizar

```php
$router->get('products', static fn (ProductRepository $products) =>
    view('products.index', ['products' => $products->all()]));
```

`view('products.index')` aponta para `resources/views/products/index.nim.php`. Uma `View` é `Responsable`, então pode ser devolvida pelo handler. `ViewFactory` (injetável) tem `make()`, `render()` e `share()` para variáveis disponíveis em todas as views.

```bash
pam make:view products.index    # cria o arquivo, dentro de <x-layout> quando o app tem esse componente
```

## Sintaxe

```blade
<x-layout title="Produtos">
    @foreach ($products as $product)
        <x-card :product="$product" class="mb-4">
            <x-slot:footer>{{ __('products.price', ['value' => $product->price]) }}</x-slot>
        </x-card>
    @endforeach

    <form method="POST" action="/products">
        @csrf
        <input name="name" value="{{ old('name') }}">
        @error('name') <span class="error">{{ $message }}</span> @enderror
    </form>
</x-layout>
```

- <span v-pre>`{{ $valor }}`</span> escapa HTML; `{!! $html !!}` imprime sem escapar.
- Atributos de componente com `:` recebem expressões PHP (`:product="$product"`); sem `:` são texto.

### Diretivas

| Grupo | Diretivas |
| --- | --- |
| Condições | `@if`, `@elseif`, `@else`, `@unless`, `@isset`, `@empty` |
| Laços | `@foreach` (com `$loop`), `@forelse`, `@for`, `@while`, `@continue`, `@break` |
| Autenticação | `@auth`, `@guest`, `@can`, `@cannot` |
| Formulários | `@csrf`, `@method('PUT')`, `@error('campo')`, `@checked`, `@selected`, `@disabled` |
| Inclusão | `@include`, `@includeIf`, `@includeWhen` |
| Pilhas | `@push`, `@prepend`, `@stack` |
| Herança | `@extends`, `@section`, `@yield`, `@show`, `@parent` |
| Outros | `@props`, `@json`, `@class`, `@lang`, `@php`, `@verbatim` |

## Componentes anônimos

Um arquivo em `resources/views/components/` vira uma tag `<x-nome>`. `@props` declara as propriedades; o resto dos atributos fica em `$attributes`. O conteúdo da tag chega em `$slot`, e slots nomeados chegam como variáveis:

```blade
{{-- resources/views/components/card.nim.php --}}
@props(['product'])
<div {{ $attributes->merge(['class' => 'card']) }}>
    {{ $product->name }}
    {{ $footer ?? '' }}
</div>
```

Componente com valor padrão, do template web:

```blade
{{-- resources/views/components/field.nim.php --}}
@props(['name', 'label', 'type' => 'text', 'keep' => true])
<label>
    {{ $label }}
    <input name="{{ $name }}" type="{{ $type }}" @if ($keep) value="{{ old($name) }}" @endif {{ $attributes }}>
    @error($name)<span class="error">{{ $message }}</span>@enderror
</label>
```

## Componentes com classe

`<x-alert>` procura `App\Views\Alert`. Os parâmetros do construtor vêm dos atributos (kebab-case vira camelCase; parâmetros tipados com classe vêm do container); as propriedades e métodos públicos viram variáveis do template:

```php
namespace App\Views;

use Nimbus\View\Component;

final class Alert extends Component
{
    public function __construct(public string $type = 'info', public string $message = '') {}

    public function render(): string
    {
        return 'components.alert';
    }

    public function shouldRender(): bool
    {
        return $this->message !== '';
    }
}
```

## Conferir no CI

```bash
pam views:check
pam views:check resources/admin-views 'App\Admin\Views'
```

Compila todos os templates e imprime cada problema como `arquivo:linha: mensagem`; sai com `1` se algum falhar. Equivale a `vendor/bin/nimbus-views check`.
