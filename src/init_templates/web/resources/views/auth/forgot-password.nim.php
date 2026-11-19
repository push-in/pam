<x-layout :title="__('auth.forgot.title')">
    <section class="card">
        <h1>{{ __('auth.forgot.title') }}</h1>
        <p class="muted">{{ __('auth.forgot.help') }}</p>
        <x-status />
        <form method="POST" action="/forgot-password">
            @csrf
            <x-field name="email" type="email" :label="__('auth.email')" autocomplete="email" required autofocus />
            <button type="submit">{{ __('auth.forgot.submit') }}</button>
        </form>
        <p class="links"><a href="/login">{{ __('auth.register.login') }}</a></p>
    </section>
</x-layout>
