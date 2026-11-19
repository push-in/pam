@props(['title'])
<!DOCTYPE html>
<html lang="{{ __('app.html_lang') }}">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{{ $title }} · {{ __('app.name') }}</title>
    <style>
        :root { --bg: #f4f5f7; --surface: #ffffff; --text: #1d2330; --muted: #5b6475; --line: #d8dce3; --accent: #1f5fbf; --accent-text: #ffffff; --danger: #b42318; --ok: #0f7a3f; }
        @media (prefers-color-scheme: dark) {
            :root { --bg: #14171c; --surface: #1d2129; --text: #e7e9ee; --muted: #a0a8b8; --line: #323844; --accent: #4d8ae8; --accent-text: #0d1117; --danger: #f07167; --ok: #4cc38a; }
        }
        * { box-sizing: border-box; }
        body { margin: 0; min-height: 100vh; background: var(--bg); color: var(--text); font: 16px/1.5 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }
        header, footer { display: flex; justify-content: space-between; align-items: center; max-width: 960px; margin: 0 auto; padding: 16px 20px; }
        header a { color: var(--text); font-weight: 600; text-decoration: none; }
        footer { color: var(--muted); font-size: 14px; }
        main { max-width: 960px; margin: 0 auto; padding: 0 20px 40px; }
        .card { max-width: 420px; margin: 32px auto; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; padding: 28px; }
        .card.wide { max-width: none; }
        h1 { font-size: 24px; margin: 0 0 20px; }
        label { display: block; font-weight: 500; margin: 0 0 16px; }
        input { display: block; width: 100%; margin-top: 6px; padding: 10px 12px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--line); border-radius: 6px; }
        input:focus { outline: 2px solid var(--accent); outline-offset: 1px; }
        button { width: 100%; padding: 11px 14px; font: inherit; font-weight: 600; color: var(--accent-text); background: var(--accent); border: 0; border-radius: 6px; cursor: pointer; }
        button.link { width: auto; padding: 0; color: var(--accent); background: none; font-weight: 500; }
        a { color: var(--accent); }
        .error { display: block; color: var(--danger); font-size: 14px; margin-top: 4px; font-weight: 400; }
        .status { color: var(--ok); margin: 0 0 16px; }
        .muted { color: var(--muted); }
        .links { display: flex; justify-content: space-between; gap: 12px; margin-top: 18px; font-size: 14px; }
    </style>
</head>
<body>
    <header>
        <a href="/">{{ __('app.name') }}</a>
        @auth
            <form method="POST" action="/logout">@csrf<button class="link" type="submit">{{ __('auth.logout') }}</button></form>
        @endauth
    </header>
    <main>{{ $slot }}</main>
    <footer>
        <span>{{ __('app.name') }}</span>
        <span><a href="/locale/pt_BR">Português</a> · <a href="/locale/en">English</a></span>
    </footer>
</body>
</html>
