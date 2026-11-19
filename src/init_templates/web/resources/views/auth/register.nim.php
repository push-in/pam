<x-layout :title="__('auth.register.title')">
    <section class="card">
        <h1>{{ __('auth.register.title') }}</h1>
        <form method="POST" action="/register">
            @csrf
            <x-field name="name" :label="__('auth.name')" autocomplete="name" required autofocus />
            <x-field name="email" type="email" :label="__('auth.email')" autocomplete="email" required />
            <x-field name="password" type="password" :label="__('auth.password')" :keep="false" autocomplete="new-password" minlength="8" required />
            <x-field name="password_confirmation" type="password" :label="__('auth.password_confirmation')" :keep="false" autocomplete="new-password" required />
            <button type="submit">{{ __('auth.register.submit') }}</button>
        </form>
        <p class="links"><a href="/login">{{ __('auth.register.login') }}</a></p>
    </section>
</x-layout>
