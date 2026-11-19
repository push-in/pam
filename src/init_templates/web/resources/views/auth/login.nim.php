<x-layout :title="__('auth.login.title')">
    <section class="card">
        <h1>{{ __('auth.login.title') }}</h1>
        <x-status />
        <form method="POST" action="/login">
            @csrf
            <x-field name="email" type="email" :label="__('auth.email')" autocomplete="email" required autofocus />
            <x-field name="password" type="password" :label="__('auth.password')" :keep="false" autocomplete="current-password" required />
            <button type="submit">{{ __('auth.login.submit') }}</button>
        </form>
        <p class="links">
            <a href="/forgot-password">{{ __('auth.login.forgot') }}</a>
            <a href="/register">{{ __('auth.login.register') }}</a>
        </p>
    </section>
</x-layout>
