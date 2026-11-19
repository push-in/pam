<x-layout :title="__('auth.reset.title')">
    <section class="card">
        <h1>{{ __('auth.reset.title') }}</h1>
        <form method="POST" action="/reset-password">
            @csrf
            <input type="hidden" name="token" value="{{ $token }}">
            <x-field name="password" type="password" :label="__('auth.reset.password')" :keep="false" autocomplete="new-password" minlength="8" required autofocus />
            <x-field name="password_confirmation" type="password" :label="__('auth.password_confirmation')" :keep="false" autocomplete="new-password" required />
            <button type="submit">{{ __('auth.reset.submit') }}</button>
        </form>
    </section>
</x-layout>
