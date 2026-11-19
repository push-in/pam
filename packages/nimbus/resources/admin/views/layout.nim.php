<!doctype html>
<html lang="{{ $t('lang') }}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>@yield('title') · {{ $admin->displayTitle() }}</title>
<link rel="stylesheet" href="{{ $admin->url('_assets/admin.css') }}?v={{ \Nimbus\Admin\AdminController::assetVersion() }}">
<script src="{{ $admin->url('_assets/admin.js') }}?v={{ \Nimbus\Admin\AdminController::assetVersion() }}" defer></script>
</head>
<body>
<a class="skip" href="#main">{{ $t('skip') }}</a>
<div class="shell">
<aside class="sidebar">
<div class="brand"><a href="{{ $admin->url() }}">{{ $admin->displayTitle() }}</a></div>
<nav aria-label="{{ $t('navigation') }}">
<ul>
<li><a href="{{ $admin->url() }}"@if ($current === null) aria-current="page"@endif>{{ $t('dashboard') }}</a></li>
@foreach ($navigation as $item)
<li><a href="{{ $item['url'] }}"@if ($current === $item['slug']) aria-current="page"@endif>{{ $item['label'] }}</a></li>
@endforeach
</ul>
</nav>
<button type="button" class="theme" data-theme-toggle aria-pressed="false" hidden>{{ $t('theme') }}</button>
</aside>
<main id="main" tabindex="-1">
@if ($flash !== null)
<div class="flash" role="status">{{ $flash }}</div>
@endif
@yield('content')
</main>
</div>
</body>
</html>
