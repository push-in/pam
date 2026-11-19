<x-layout :title="__('dashboard.title')">
    <section class="card wide">
        <h1>{{ __('dashboard.title') }}</h1>
        <p>{{ __('dashboard.signed_in_as', ['name' => $user->name, 'email' => $user->email]) }}</p>
        <p class="muted">{{ __('dashboard.member_since', ['date' => $user->createdAt->format('Y-m-d')]) }}</p>
    </section>
</x-layout>
